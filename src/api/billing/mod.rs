use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use rand::{RngCore, Rng};
use hex;
use chrono::Utc;

use crate::api::rest::AppState;

const GRACE_PERIOD_DURATION_SECONDS: i64 = 24 * 60 * 60; // 24 Hours
const GRACE_PERIOD_EFFICIENCY: f32 = 0.4; // 40% Efficiency

#[derive(Deserialize)]
pub struct GenerateKeyRequest {
    pub developer_email: String,
    pub project_name: String,
}

#[derive(Serialize)]
pub struct GenerateKeyResponse {
    pub api_key: String,
    pub status: String,
    pub grace_period_remaining: Option<i64>,
    pub efficiency: Option<f32>,
}

#[derive(Deserialize)]
pub struct TelemetryRequest {
    pub api_key: String,
    pub signature_hash: String,
}

#[derive(Serialize)]
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
/// Creates a new API Key for B2B SDK clients and initializes their billing counter in Redis
async fn generate_developer_key(
    State(state): State<AppState>,
    Json(payload): Json<GenerateKeyRequest>,
) -> impl IntoResponse {
    let api_key = {
        let mut rng = rand::thread_rng();
        let mut raw_key = [0u8; 32];
        rng.fill_bytes(&mut raw_key);

        let mut hasher = Sha256::new();
        hasher.update(&raw_key);
        format!("cxl_{}", hex::encode(hasher.finalize()))
    };

    let mut conn = match state.storage.redis_client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to connect to Redis for key generation: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(GenerateKeyResponse {
                api_key: "".to_string(),
                status: "Internal Database Error".to_string(),
                grace_period_remaining: None,
                efficiency: None,
            })).into_response();
        }
    };

    // Store API key in Redis with an initial usage count of 0
    let redis_key = format!("apikey:{}", api_key);
    let _: redis::RedisResult<()> = redis::cmd("HSET")
        .arg(&redis_key)
        .arg("email").arg(&payload.developer_email)
        .arg("project").arg(&payload.project_name)
        .arg("usage").arg(0)
        .query_async(&mut conn).await;

    tracing::info!("Generated new API Key for {}", payload.developer_email);

    Json(GenerateKeyResponse {
        api_key,
        status: "Key Generated. Free Tier: 50,000 Signatures".to_string(),
        grace_period_remaining: None,
        efficiency: None,
    }).into_response()
}

/// [NEXUS-02] Signature Telemetry Ingestion Endpoint
/// Called asynchronously by the lib-conclave-sdk every time a signature is successfully generated.
/// Implements CON-19: Sovereign Grace Period (24h @ 40% efficiency)
async fn track_signature(
    State(state): State<AppState>,
    Json(payload): Json<TelemetryRequest>,
) -> impl IntoResponse {
    let mut conn = match state.storage.redis_client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let redis_key = format!("apikey:{}", payload.api_key);
    
    // Verify key exists
    let exists: bool = redis::cmd("EXISTS").arg(&redis_key).query_async(&mut conn).await.unwrap_or(false);
    if !exists {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }

    // Increment usage counter atomically
    let new_usage: u64 = redis::cmd("HINCRBY")
        .arg(&redis_key)
        .arg("usage").arg(1)
        .query_async(&mut conn).await.unwrap_or(0);

    // B2B SDK Limit Enforcement Logic
    let free_limit = 50_000;
    
    if new_usage > free_limit {
        let now = Utc::now().timestamp();

        // Check for grace period start
        let grace_start: Option<i64> = redis::cmd("HGET")
            .arg(&redis_key)
            .arg("grace_period_start")
            .query_async(&mut conn).await.unwrap_or(None);

        let grace_start = match grace_start {
            Some(start) => start,
            None => {
                // Initialize grace period
                let _: redis::RedisResult<()> = redis::cmd("HSET")
                    .arg(&redis_key)
                    .arg("grace_period_start").arg(now)
                    .query_async(&mut conn).await;
                now
            }
        };

        let mut rng = rand::thread_rng();
        let roll: f32 = rng.gen();

        match determine_grace_status(now, grace_start, roll) {
            GraceStatus::Active { remaining, allowed } => {
                if !allowed {
                    tracing::warn!("API Key {} in Grace Period: Request throttled (60% drop rate)", payload.api_key);
                    return (StatusCode::PAYMENT_REQUIRED, Json(TelemetryResponse {
                        current_usage: new_usage,
                        limit: free_limit,
                        status: "GRACE_PERIOD_THROTTLED".to_string(),
                        grace_period_remaining: Some(remaining),
                        efficiency: Some(GRACE_PERIOD_EFFICIENCY),
                    })).into_response();
                }

                tracing::info!("API Key {} in Grace Period: Request allowed (40% efficiency)", payload.api_key);
                return Json(TelemetryResponse {
                    current_usage: new_usage,
                    limit: free_limit,
                    status: "GRACE_PERIOD_ACTIVE".to_string(),
                    grace_period_remaining: Some(remaining),
                    efficiency: Some(GRACE_PERIOD_EFFICIENCY),
                }).into_response();
            },
            GraceStatus::Expired => {
                tracing::error!("API Key {} has expired. Grace period ended.", payload.api_key);
                return (StatusCode::FORBIDDEN, Json(TelemetryResponse {
                    current_usage: new_usage,
                    limit: free_limit,
                    status: "LICENSE_EXPIRED".to_string(),
                    grace_period_remaining: Some(0),
                    efficiency: Some(0.0),
                })).into_response();
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
        let start = now - 3600; // 1 hour ago

        // Active and allowed (roll <= 0.4)
        match determine_grace_status(now, start, 0.3) {
            GraceStatus::Active { remaining, allowed } => {
                assert_eq!(remaining, GRACE_PERIOD_DURATION_SECONDS - 3600);
                assert!(allowed);
            },
            _ => panic!("Expected Active"),
        }

        // Active and throttled (roll > 0.4)
        match determine_grace_status(now, start, 0.5) {
            GraceStatus::Active { remaining, allowed } => {
                assert_eq!(remaining, GRACE_PERIOD_DURATION_SECONDS - 3600);
                assert!(!allowed);
            },
            _ => panic!("Expected Active"),
        }

        // Expired (elapsed > 24h)
        let expired_start = now - (25 * 3600);
        match determine_grace_status(now, expired_start, 0.1) {
            GraceStatus::Expired => {},
            _ => panic!("Expected Expired"),
        }
    }
}
