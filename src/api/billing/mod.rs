//! B2B Billing and License Enforcement Module.
//! Implements CON-19: Sovereign Grace Period (24h @ 40% efficiency).

use crate::api::rest::AppState;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod nostr;

type HmacSha256 = Hmac<Sha256>;

const GRACE_PERIOD_DURATION_SECONDS: i64 = 86400; // 24 hours
const GRACE_PERIOD_EFFICIENCY: f32 = 0.4;
const MAX_ORGANIZATION_ID_LEN: usize = 128;
const FREE_TIER_SIGNATURE_LIMIT: u64 = 50_000;

#[derive(Debug, Deserialize)]
pub struct GenerateKeyRequest {
    pub organization_id: String,
    pub developer_email: String,
    pub project_name: String,
}

#[derive(Debug, Serialize)]
pub struct GenerateKeyResponse {
    pub api_key: String,
    pub api_secret: String,
    pub status: String,
    pub grace_period_remaining: Option<i64>,
    pub efficiency: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct TelemetryRequest {
    pub api_key: String,
    pub signature_hash: String,
    pub timestamp: i64,
    pub hmac: String,
}

#[derive(Debug, Serialize)]
pub struct TelemetryResponse {
    pub current_usage: u64,
    pub limit: u64,
    pub status: String,
    pub grace_period_remaining: Option<i64>,
    pub efficiency: Option<f32>,
}

pub fn billing_routes() -> Router<AppState> {
    Router::new()
        .route("/generate-key", post(generate_developer_key))
        .route("/telemetry/track-signature", post(track_signature))
}

#[derive(Debug, PartialEq)]
enum GraceStatus {
    Active { remaining: i64, allowed: bool },
    Expired,
}

#[derive(Debug, PartialEq)]
enum TelemetryAuthError {
    InvalidApiKey,
    InvalidHmac,
}

#[derive(Debug, PartialEq)]
enum QuotaDecision {
    WithinLimit,
    GraceAllowed { grace_start_to_set: Option<i64> },
    GraceThrottled { remaining: i64 },
    GraceExpired,
}

fn determine_grace_status(now: i64, grace_start: i64, roll: f32) -> GraceStatus {
    let elapsed = now - grace_start;
    if elapsed < GRACE_PERIOD_DURATION_SECONDS {
        let remaining = GRACE_PERIOD_DURATION_SECONDS - elapsed;
        let allowed = roll <= GRACE_PERIOD_EFFICIENCY;
        GraceStatus::Active { remaining, allowed }
    } else {
        GraceStatus::Expired
    }
}

fn compute_expected_hmac(secret: &str, signature_hash: &str, timestamp: i64) -> String {
    let message = format!("{}:{}", signature_hash, timestamp);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC error");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn validate_telemetry_auth(
    data: &std::collections::HashMap<String, String>,
    payload: &TelemetryRequest,
) -> Result<(), TelemetryAuthError> {
    if data.is_empty() {
        return Err(TelemetryAuthError::InvalidApiKey);
    }

    let secret = data.get("secret").cloned().unwrap_or_default();
    let expected_hmac = compute_expected_hmac(&secret, &payload.signature_hash, payload.timestamp);

    if expected_hmac != payload.hmac {
        return Err(TelemetryAuthError::InvalidHmac);
    }

    Ok(())
}

fn evaluate_quota_decision(
    new_usage: u64,
    now: i64,
    grace_start: Option<i64>,
    roll: f32,
) -> QuotaDecision {
    if new_usage <= FREE_TIER_SIGNATURE_LIMIT {
        return QuotaDecision::WithinLimit;
    }

    let mut grace_start_to_set = None;
    let effective_grace_start = match grace_start {
        Some(start) => start,
        None => {
            grace_start_to_set = Some(now);
            now
        }
    };

    match determine_grace_status(now, effective_grace_start, roll) {
        GraceStatus::Active { remaining, allowed } => {
            if allowed {
                QuotaDecision::GraceAllowed { grace_start_to_set }
            } else {
                QuotaDecision::GraceThrottled { remaining }
            }
        }
        GraceStatus::Expired => QuotaDecision::GraceExpired,
    }
}

/// [NEXUS-01] Developer API Key Generation
async fn generate_developer_key(
    State(state): State<AppState>,
    Json(payload): Json<GenerateKeyRequest>,
) -> impl IntoResponse {
    let organization_id = payload.organization_id.trim();
    if organization_id.is_empty() || organization_id.len() > MAX_ORGANIZATION_ID_LEN {
        return (StatusCode::BAD_REQUEST, "Invalid organization_id").into_response();
    }

    let (api_key, api_secret) = {
        let raw_key: [u8; 32] = rand::random();
        let raw_secret: [u8; 32] = rand::random();

        (
            format!("cxl_{}", hex::encode(Sha256::digest(raw_key))),
            hex::encode(Sha256::digest(raw_secret)),
        )
    };

    let mut conn = match state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to connect to Redis: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Redis Error").into_response();
        }
    };

    let redis_key = format!("apikey:{}", api_key);
    let _: redis::RedisResult<()> = redis::cmd("HSET")
        .arg(&redis_key)
        .arg("org_id")
        .arg(organization_id)
        .arg("email")
        .arg(&payload.developer_email)
        .arg("project")
        .arg(&payload.project_name)
        .arg("secret")
        .arg(&api_secret)
        .arg("usage")
        .arg(0)
        .query_async(&mut conn)
        .await;

    Json(GenerateKeyResponse {
        api_key,
        api_secret,
        status: "Key Generated. Free Tier: 50,000 Signatures".to_string(),
        grace_period_remaining: None,
        efficiency: None,
    })
    .into_response()
}

/// [NEXUS-02] Signature Telemetry Ingestion Endpoint
async fn track_signature(
    State(state): State<AppState>,
    Json(payload): Json<TelemetryRequest>,
) -> impl IntoResponse {
    let mut conn = match state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
    {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let redis_key = format!("apikey:{}", payload.api_key);
    let data: std::collections::HashMap<String, String> = redis::cmd("HGETALL")
        .arg(&redis_key)
        .query_async(&mut conn)
        .await
        .unwrap_or_default();

    match validate_telemetry_auth(&data, &payload) {
        Ok(()) => {}
        Err(TelemetryAuthError::InvalidApiKey) => {
            return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
        }
        Err(TelemetryAuthError::InvalidHmac) => {
            return (StatusCode::UNAUTHORIZED, "Invalid HMAC").into_response();
        }
    }

    // [CON-473] PoC: Publish to Nostr if enabled
    if let Some(nostr) = &state.nostr {
        let _ = nostr
            .track_signature_nostr(&payload.api_key, &payload.signature_hash, payload.timestamp)
            .await
            .ok();
    }

    // Increment usage
    let new_usage: u64 = redis::cmd("HINCRBY")
        .arg(&redis_key)
        .arg("usage")
        .arg(1)
        .query_async(&mut conn)
        .await
        .unwrap_or(0);
    let quota_decision = if new_usage <= FREE_TIER_SIGNATURE_LIMIT {
        QuotaDecision::WithinLimit
    } else {
        let now = Utc::now().timestamp();
        let grace_start: Option<i64> = redis::cmd("HGET")
            .arg(&redis_key)
            .arg("grace_period_start")
            .query_async(&mut conn)
            .await
            .unwrap_or(None);
        let roll: f32 = rand::random();
        evaluate_quota_decision(new_usage, now, grace_start, roll)
    };

    match quota_decision {
        QuotaDecision::WithinLimit => {}
        QuotaDecision::GraceAllowed { grace_start_to_set } => {
            if let Some(start) = grace_start_to_set {
                let _: () = redis::cmd("HSET")
                    .arg(&redis_key)
                    .arg("grace_period_start")
                    .arg(start)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(());
            }
        }
        QuotaDecision::GraceThrottled { remaining } => {
            return (
                StatusCode::PAYMENT_REQUIRED,
                Json(TelemetryResponse {
                    current_usage: new_usage,
                    limit: FREE_TIER_SIGNATURE_LIMIT,
                    status: "THROTTLED".to_string(),
                    grace_period_remaining: Some(remaining),
                    efficiency: Some(GRACE_PERIOD_EFFICIENCY),
                }),
            )
                .into_response();
        }
        QuotaDecision::GraceExpired => {
            return (StatusCode::FORBIDDEN, "License Expired").into_response();
        }
    }

    Json(TelemetryResponse {
        current_usage: new_usage,
        limit: FREE_TIER_SIGNATURE_LIMIT,
        status: "OK".to_string(),
        grace_period_remaining: None,
        efficiency: None,
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_determine_grace_status() {
        let now = 1000000;
        let start = now - 3600;
        match determine_grace_status(now, start, 0.3) {
            GraceStatus::Active { remaining, allowed } => {
                assert_eq!(remaining, GRACE_PERIOD_DURATION_SECONDS - 3600);
                assert!(allowed);
            }
            _ => panic!("Expected Active"),
        }
    }

    #[test]
    fn test_validate_telemetry_auth_rejects_unknown_api_key() {
        let payload = TelemetryRequest {
            api_key: "cxl_unknown".to_string(),
            signature_hash: "abc123".to_string(),
            timestamp: 1_700_000_000,
            hmac: "bad".to_string(),
        };

        let result = validate_telemetry_auth(&HashMap::new(), &payload);
        assert_eq!(result, Err(TelemetryAuthError::InvalidApiKey));
    }

    #[test]
    fn test_validate_telemetry_auth_rejects_invalid_hmac() {
        let payload = TelemetryRequest {
            api_key: "cxl_known".to_string(),
            signature_hash: "abc123".to_string(),
            timestamp: 1_700_000_000,
            hmac: "bad".to_string(),
        };

        let mut data = HashMap::new();
        data.insert("secret".to_string(), "secret123".to_string());

        let result = validate_telemetry_auth(&data, &payload);
        assert_eq!(result, Err(TelemetryAuthError::InvalidHmac));
    }

    #[test]
    fn test_validate_telemetry_auth_accepts_valid_hmac() {
        let signature_hash = "abc123";
        let timestamp = 1_700_000_000;
        let secret = "secret123";
        let payload = TelemetryRequest {
            api_key: "cxl_known".to_string(),
            signature_hash: signature_hash.to_string(),
            timestamp,
            hmac: compute_expected_hmac(secret, signature_hash, timestamp),
        };

        let mut data = HashMap::new();
        data.insert("secret".to_string(), secret.to_string());

        let result = validate_telemetry_auth(&data, &payload);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_evaluate_quota_decision_within_limit() {
        let decision = evaluate_quota_decision(FREE_TIER_SIGNATURE_LIMIT, 1000, Some(900), 0.9);
        assert_eq!(decision, QuotaDecision::WithinLimit);
    }

    #[test]
    fn test_evaluate_quota_decision_sets_grace_start_and_allows() {
        let now = 1_000_000;
        let decision = evaluate_quota_decision(FREE_TIER_SIGNATURE_LIMIT + 1, now, None, 0.3);
        assert_eq!(
            decision,
            QuotaDecision::GraceAllowed {
                grace_start_to_set: Some(now)
            }
        );
    }

    #[test]
    fn test_evaluate_quota_decision_throttles_during_grace() {
        let now = 1_000_000;
        let grace_start = now - 60;
        let decision =
            evaluate_quota_decision(FREE_TIER_SIGNATURE_LIMIT + 1, now, Some(grace_start), 0.95);

        assert_eq!(
            decision,
            QuotaDecision::GraceThrottled {
                remaining: GRACE_PERIOD_DURATION_SECONDS - 60
            }
        );
    }

    #[test]
    fn test_evaluate_quota_decision_expires_after_grace_window() {
        let now = 1_000_000;
        let grace_start = now - (GRACE_PERIOD_DURATION_SECONDS + 1);
        let decision =
            evaluate_quota_decision(FREE_TIER_SIGNATURE_LIMIT + 1, now, Some(grace_start), 0.1);

        assert_eq!(decision, QuotaDecision::GraceExpired);
    }
}
