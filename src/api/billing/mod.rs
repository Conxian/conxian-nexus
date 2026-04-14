//! B2B Billing and License Enforcement Module.
//! Implements CON-19: Sovereign Grace Period (24h @ 40% efficiency).

use crate::api::rest::AppState;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use rand::Rng;

pub mod nostr;

type HmacSha256 = Hmac<Sha256>;

const GRACE_PERIOD_DURATION_SECONDS: i64 = 86400; // 24 hours
const GRACE_PERIOD_EFFICIENCY: f32 = 0.4;
const MAX_ORGANIZATION_ID_LEN: usize = 128;

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
        let mut rng = rand::thread_rng();
        let mut raw_key = [0u8; 32];
        let mut raw_secret = [0u8; 32];
        rng.fill(&mut raw_key);
        rng.fill(&mut raw_secret);

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
        .arg("org_id").arg(organization_id)
        .arg("email").arg(&payload.developer_email)
        .arg("project").arg(&payload.project_name)
        .arg("secret").arg(&api_secret)
        .arg("usage").arg(0)
        .query_async(&mut conn).await;

    Json(GenerateKeyResponse {
        api_key,
        api_secret,
        status: "Key Generated. Free Tier: 50,000 Signatures".to_string(),
        grace_period_remaining: None,
        efficiency: None,
    }).into_response()
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
        .query_async(&mut conn).await.unwrap_or_default();

    if data.is_empty() {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }

    let secret = data.get("secret").cloned().unwrap_or_default();

    // Verify HMAC
    let message = format!("{}:{}", payload.signature_hash, payload.timestamp);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC error");
    mac.update(message.as_bytes());
    let expected_hmac = hex::encode(mac.finalize().into_bytes());

    if expected_hmac != payload.hmac {
        return (StatusCode::UNAUTHORIZED, "Invalid HMAC").into_response();
    }

    // [CON-473] PoC: Publish to Nostr if enabled
    if let Some(nostr) = &state.nostr {
        let _ = nostr.track_signature_nostr(
            &payload.api_key,
            &payload.signature_hash,
            payload.timestamp
        ).await.ok();
    }

    // Increment usage
    let new_usage: u64 = redis::cmd("HINCRBY").arg(&redis_key).arg("usage").arg(1).query_async(&mut conn).await.unwrap_or(0);
    let free_limit = 50_000;

    if new_usage > free_limit {
        let now = Utc::now().timestamp();
        let grace_start: Option<i64> = redis::cmd("HGET").arg(&redis_key).arg("grace_period_start").query_async(&mut conn).await.unwrap_or(None);
        let grace_start = match grace_start {
            Some(s) => s,
            None => {
                let _: () = redis::cmd("HSET").arg(&redis_key).arg("grace_period_start").arg(now).query_async(&mut conn).await.unwrap_or(());
                now
            }
        };

        let roll: f32 = rand::thread_rng().gen();
        match determine_grace_status(now, grace_start, roll) {
            GraceStatus::Active { remaining, allowed } => {
                if !allowed {
                    return (StatusCode::PAYMENT_REQUIRED, Json(TelemetryResponse {
                        current_usage: new_usage,
                        limit: free_limit,
                        status: "THROTTLED".to_string(),
                        grace_period_remaining: Some(remaining),
                        efficiency: Some(GRACE_PERIOD_EFFICIENCY),
                    })).into_response();
                }
            }
            GraceStatus::Expired => {
                return (StatusCode::FORBIDDEN, "License Expired").into_response();
            }
        }
    }

    Json(TelemetryResponse {
        current_usage: new_usage,
        limit: free_limit,
        status: "OK".to_string(),
        grace_period_remaining: None,
        efficiency: None,
    }).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
