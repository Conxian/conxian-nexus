use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use rand::RngCore;
use hex;

use crate::api::rest::AppState;

#[derive(Deserialize)]
pub struct GenerateKeyRequest {
    pub developer_email: String,
    pub project_name: String,
}

#[derive(Serialize)]
pub struct GenerateKeyResponse {
    pub api_key: String,
    pub status: String,
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
}

pub fn billing_routes() -> Router<AppState> {
    Router::new()
        .route("/generate-key", post(generate_developer_key))
        .route("/telemetry/track-signature", post(track_signature))
}

/// [NEXUS-01] Developer API Key Generation
/// Creates a new API Key for B2B SDK clients and initializes their billing counter in Redis
async fn generate_developer_key(
    State(state): State<AppState>,
    Json(payload): Json<GenerateKeyRequest>,
) -> impl IntoResponse {
    let mut rng = rand::thread_rng();
    let mut raw_key = [0u8; 32];
    rng.fill_bytes(&mut raw_key);
    
    let mut hasher = Sha256::new();
    hasher.update(&raw_key);
    let api_key = format!("cxl_{}", hex::encode(hasher.finalize()));

    let mut conn = match state.storage.redis_client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to connect to Redis for key generation: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(GenerateKeyResponse {
                api_key: "".to_string(),
                status: "Internal Database Error".to_string(),
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
        status: "Key Generated. Free Tier: 50,000 Signatures".to_string()
    }).into_response()
}

/// [NEXUS-02] Signature Telemetry Ingestion Endpoint
/// Called asynchronously by the lib-conclave-sdk every time a signature is successfully generated.
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
        tracing::warn!("API Key {} has exceeded the 50k free tier limit. Billing intervention required.", payload.api_key);
        // Here we would typically trigger Stripe / Crypto Billing Webhooks
    }

    Json(TelemetryResponse {
        current_usage: new_usage,
        limit: free_limit,
        status: if new_usage <= free_limit { "OK".to_string() } else { "LIMIT_EXCEEDED".to_string() },
    }).into_response()
}
