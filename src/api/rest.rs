use axum::{
    routing::{get, post},
    extract::{Query, State, Json},
    Router,
};
use std::sync::Arc;
use crate::storage::Storage;
use serde::{Deserialize, Serialize};

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

pub async fn start_rest_server(storage: Arc<Storage>, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/v1/proof", get(get_proof))
        .route("/v1/verify-state", post(verify_state))
        .route("/health", get(health_check))
        .with_state(storage);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("REST server listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_proof(
    State(_storage): State<Arc<Storage>>,
    Query(params): Query<ProofParams>,
) -> Json<ProofResponse> {
    // Placeholder implementation
    Json(ProofResponse {
        hash: format!("hash_for_{}", params.key),
        proof: "dummy_proof".to_string(),
    })
}

async fn verify_state(
    State(_storage): State<Arc<Storage>>,
    Json(payload): Json<VerifyStateRequest>,
) -> Json<VerifyStateResponse> {
    // Placeholder implementation
    Json(VerifyStateResponse {
        valid: payload.state_root.starts_with("0x"),
    })
}
async fn health_check() -> &'static str { "OK" }
