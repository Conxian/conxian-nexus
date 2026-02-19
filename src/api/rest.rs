use axum::{
    routing::{get, post},
    extract::{Query, State, Json},
    Router,
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use crate::storage::Storage;
use crate::state::NexusState;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub nexus_state: Arc<NexusState>,
}

#[derive(Deserialize)]
pub struct ProofParams {
    key: String,
}

#[derive(Serialize)]
pub struct ProofResponse {
    hash: String,
    proof: String,
}

#[derive(Deserialize)]
pub struct VerifyStateRequest {
    state_root: String,
}

#[derive(Serialize)]
pub struct VerifyStateResponse {
    valid: bool,
}

#[derive(Serialize)]
pub struct StatusResponse {
    state_root: String,
    processed_height: u64,
    safety_mode: bool,
    drift: u64,
}

pub async fn start_rest_server(storage: Arc<Storage>, nexus_state: Arc<NexusState>, port: u16) -> anyhow::Result<()> {
    let state = AppState {
        storage,
        nexus_state,
    };

    let app = Router::new()
        .route("/v1/proof", get(get_proof))
        .route("/v1/verify-state", post(verify_state))
        .route("/v1/status", get(get_status))
        .route("/v1/services", get(get_services_status))
        .route("/health", get(health_check))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("REST server listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_proof(
    State(state): State<AppState>,
    Query(params): Query<ProofParams>,
) -> impl IntoResponse {
    let (hash, proof) = state.nexus_state.generate_proof(&params.key);
    Json(ProofResponse {
        hash,
        proof,
    })
}

async fn verify_state(
    State(state): State<AppState>,
    Json(payload): Json<VerifyStateRequest>,
) -> impl IntoResponse {
    let current_root = state.nexus_state.get_state_root();
    Json(VerifyStateResponse {
        valid: current_root == payload.state_root,
    })
}

async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let state_root = state.nexus_state.get_state_root();

    let row = sqlx::query("SELECT MAX(height) as max_height FROM stacks_blocks")
        .fetch_one(&state.storage.pg_pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error in get_status: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let processed_height: Option<i64> = row.get("max_height");

    let mut conn = state.storage.redis_client.get_multiplexed_async_connection().await
        .map_err(|e| {
            tracing::error!("Redis connection error in get_status: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let safety_mode: bool = redis::cmd("GET")
        .arg("nexus:safety_mode")
        .query_async(&mut conn)
        .await
        .unwrap_or(false);

    let drift: u64 = redis::cmd("GET")
        .arg("nexus:drift")
        .query_async(&mut conn)
        .await
        .unwrap_or(0);

    Ok(Json(StatusResponse {
        state_root,
        processed_height: processed_height.unwrap_or(0) as u64,
        safety_mode,
        drift,
    }))
}

async fn get_services_status() -> impl IntoResponse {
    Json(crate::api::services::get_all_services_status())
}

async fn health_check() -> &'static str { "OK" }
