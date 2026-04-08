//! REST API handlers for Conxian Nexus.

use crate::executor::{ExecutionRequest, NexusExecutor};
use crate::oracle::OracleService;
use crate::state::NexusState;
use crate::storage::Storage;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use lazy_static::lazy_static;
use prometheus::{Encoder, IntGauge, TextEncoder};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::{Arc, OnceLock};

static PROMETHEUS_METRICS_INIT: OnceLock<()> = OnceLock::new();

lazy_static! {
    pub static ref TOTAL_TRANSACTIONS: IntGauge = IntGauge::new(
        "nexus_total_transactions",
        "Total number of transactions processed"
    )
    .expect("nexus_total_transactions metric must be valid");
    pub static ref TOTAL_BLOCKS: IntGauge =
        IntGauge::new("nexus_total_blocks", "Total number of blocks processed")
            .expect("nexus_total_blocks metric must be valid");
    pub static ref SYNC_DRIFT: IntGauge =
        IntGauge::new("nexus_sync_drift", "Current sync drift in blocks")
            .expect("nexus_sync_drift metric must be valid");
    pub static ref SAFETY_MODE: IntGauge = IntGauge::new(
        "nexus_safety_mode",
        "Safety mode status (1 = active, 0 = inactive)"
    )
    .expect("nexus_safety_mode metric must be valid");
}

fn init_prometheus_metrics() {
    PROMETHEUS_METRICS_INIT.get_or_init(|| {
        lazy_static::initialize(&TOTAL_TRANSACTIONS);
        lazy_static::initialize(&TOTAL_BLOCKS);
        lazy_static::initialize(&SYNC_DRIFT);
        lazy_static::initialize(&SAFETY_MODE);

        if let Err(e) = (|| -> prometheus::Result<()> {
            prometheus::register(Box::new(TOTAL_TRANSACTIONS.clone()))?;
            prometheus::register(Box::new(TOTAL_BLOCKS.clone()))?;
            prometheus::register(Box::new(SYNC_DRIFT.clone()))?;
            prometheus::register(Box::new(SAFETY_MODE.clone()))?;
            Ok(())
        })() {
            tracing::error!(error = %e, "Prometheus metrics registration failed");
        }
    });
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub nexus_state: Arc<NexusState>,
    pub executor: Arc<NexusExecutor>,
    pub oracle: Option<Arc<OracleService>>,
}

#[derive(Deserialize)]
pub struct ProofParams {
    pub key: String,
}

#[derive(Serialize)]
pub struct ProofResponse {
    pub hash: String,
    pub proof: String,
}

#[derive(Deserialize)]
pub struct MMRProofParams {
    pub tx_id: Option<String>,
    pub index: Option<usize>,
}

#[derive(Deserialize)]
pub struct VerifyStateRequest {
    pub state_root: String,
}

#[derive(Serialize)]
pub struct VerifyStateResponse {
    pub valid: bool,
    pub mmr_root: String,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub state_root: String,
    pub mmr_root: String,
    pub processed_height: u64,
    pub safety_mode: bool,
    pub drift: u64,
}

#[derive(Serialize)]
pub struct MetricsResponse {
    pub total_transactions: u64,
    pub total_blocks: u64,
    pub safety_mode: bool,
    pub drift: u64,
    pub uptime_seconds: u64,
}

#[derive(Serialize)]
pub struct ExecutionResponse {
    pub tx_id: String,
    pub status: String,
    pub message: String,
}

pub fn app_router(
    storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
    executor: Arc<NexusExecutor>,
    oracle: Option<Arc<OracleService>>,
    experimental_apis_enabled: bool,
) -> Router {
    init_prometheus_metrics();

    let state = AppState {
        storage,
        nexus_state,
        executor,
        oracle,
    };

    let mut router = Router::new()
        .route("/v1/proof", get(get_proof))
        .route("/v1/mmr-proof", get(get_mmr_proof))
        .route("/v1/verify-state", post(verify_state))
        .route("/v1/status", get(get_status))
        .route("/v1/metrics", get(get_metrics))
        .route("/metrics", get(prometheus_metrics))
        .route("/v1/execute", post(execute_tx))
        .route("/v1/services", get(get_services_status))
        .route("/health", get(health_check));

    if experimental_apis_enabled {
        router = router
            .route("/v1/erp/sync", post(crate::api::erp::erp_sync_handler))
            .route(
                "/v1/zkml/verify",
                post(crate::api::zkml::verify_zkml_handler),
            )
            .route(
                "/v1/identity/resolve",
                post(crate::api::identity::resolve_identity_handler),
            )
            .route(
                "/v1/dlc/create-bond",
                post(crate::api::dlc::create_dlc_bond_handler),
            )
            .route(
                "/v1/settlement/trigger",
                post(crate::api::settlement::settlement_trigger_handler),
            );
    }

    router
        .nest("/v1/billing", crate::api::billing::billing_routes())
        .with_state(state)
}

pub async fn start_rest_server(
    storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
    executor: Arc<NexusExecutor>,
    oracle: Option<Arc<OracleService>>,
    port: u16,
    experimental_apis_enabled: bool,
) -> anyhow::Result<()> {
    let app = app_router(
        storage,
        nexus_state,
        executor,
        oracle,
        experimental_apis_enabled,
    );

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("REST server listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_proof(
    State(state): State<AppState>,
    Query(params): Query<ProofParams>,
) -> impl IntoResponse {
    let (hash, proof) = state
        .nexus_state
        .generate_merkle_proof(&params.key)
        .map(|p| {
            (
                p.root.clone(),
                serde_json::to_string(&p).unwrap_or_default(),
            )
        })
        .unwrap_or_else(|| (state.nexus_state.get_state_root(), "{}".to_string()));
    Json(ProofResponse { hash, proof })
}

async fn get_mmr_proof(
    State(state): State<AppState>,
    Query(params): Query<MMRProofParams>,
) -> Result<Json<crate::state::MMRProof>, StatusCode> {
    let index = match (params.index, params.tx_id) {
        (Some(i), _) => Some(i),
        (None, Some(tx_id)) => state.nexus_state.get_leaf_index(&tx_id),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let leaf_index = index.ok_or(StatusCode::NOT_FOUND)?;

    let leaf = state
        .nexus_state
        .get_leaf_by_index(leaf_index)
        .ok_or(StatusCode::NOT_FOUND)?;

    let (leaf_pos, sibling_positions) = state
        .nexus_state
        .get_mmr_proof_metadata(leaf_index)
        .ok_or_else(|| {
            tracing::error!(
                "MMR proof metadata could not be computed for leaf_index {}",
                leaf_index
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut siblings = Vec::new();
    for pos in sibling_positions {
        let row = sqlx::query("SELECT hash FROM mmr_nodes WHERE pos = $1")
            .bind(pos as i64)
            .fetch_optional(&state.storage.pg_pool)
            .await
            .map_err(|e| {
                tracing::error!("DB error fetching MMR node: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        if let Some(r) = row {
            let hash_bytes: Vec<u8> = r.get(0);
            siblings.push((pos, format!("0x{}", hex::encode(hash_bytes))));
        }
    }

    let proof = state
        .nexus_state
        .assemble_mmr_proof(leaf, leaf_pos, siblings);
    Ok(Json(proof))
}

async fn verify_state(
    State(state): State<AppState>,
    Json(payload): Json<VerifyStateRequest>,
) -> impl IntoResponse {
    let current_root = state.nexus_state.get_state_root();
    Json(VerifyStateResponse {
        valid: current_root == payload.state_root,
        mmr_root: state.nexus_state.get_mmr_root(),
    })
}

async fn get_status(State(state): State<AppState>) -> Result<Json<StatusResponse>, StatusCode> {
    let state_root = state.nexus_state.get_state_root();

    let row = sqlx::query(
        "SELECT MAX(height) as max_height FROM stacks_blocks WHERE state != 'orphaned'",
    )
    .fetch_one(&state.storage.pg_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error in get_status: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let processed_height: Option<i64> = row.get("max_height");

    let mut conn = state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
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
        mmr_root: state.nexus_state.get_mmr_root(),
        processed_height: processed_height.unwrap_or(0) as u64,
        safety_mode,
        drift,
    }))
}

async fn get_metrics(State(state): State<AppState>) -> Result<Json<MetricsResponse>, StatusCode> {
    let tx_count: i64 = sqlx::query("SELECT COUNT(*) FROM stacks_transactions t JOIN stacks_blocks b ON t.block_hash = b.hash WHERE b.state != 'orphaned'")
        .fetch_one(&state.storage.pg_pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);

    let block_count: i64 =
        sqlx::query("SELECT COUNT(*) FROM stacks_blocks WHERE state != 'orphaned'")
            .fetch_one(&state.storage.pg_pool)
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);

    let mut conn = state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let safety_mode_active: bool = redis::cmd("GET")
        .arg("nexus:safety_mode")
        .query_async(&mut conn)
        .await
        .unwrap_or(false);
    let drift: u64 = redis::cmd("GET")
        .arg("nexus:drift")
        .query_async(&mut conn)
        .await
        .unwrap_or(0);

    // Update Prometheus metrics
    TOTAL_TRANSACTIONS.set(tx_count);
    TOTAL_BLOCKS.set(block_count);
    SYNC_DRIFT.set(drift as i64);
    SAFETY_MODE.set(if safety_mode_active { 1 } else { 0 });

    let uptime = crate::api::get_uptime();

    Ok(Json(MetricsResponse {
        total_transactions: tx_count as u64,
        total_blocks: block_count as u64,
        safety_mode: safety_mode_active,
        drift,
        uptime_seconds: uptime,
    }))
}

async fn prometheus_metrics() -> impl IntoResponse {
    init_prometheus_metrics();

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    String::from_utf8(buffer).unwrap()
}

async fn execute_tx(
    State(state): State<AppState>,
    Json(request): Json<ExecutionRequest>,
) -> impl IntoResponse {
    match state.executor.validate_transaction(&request).await {
        Ok(true) => {
            TOTAL_TRANSACTIONS.set(TOTAL_TRANSACTIONS.get() + 1);
            // Simulate execution success
            Json(ExecutionResponse {
                tx_id: request.tx_id,
                status: "Success".to_string(),
                message: "Transaction validated by FSOC Sequencer and executed.".to_string(),
            })
            .into_response()
        }
        Ok(false) => (
            StatusCode::BAD_REQUEST,
            Json(ExecutionResponse {
                tx_id: request.tx_id,
                status: "Rejected".to_string(),
                message: "Transaction rejected by FSOC Sequencer (Potential MEV/Front-running)."
                    .to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ExecutionResponse {
                tx_id: request.tx_id,
                status: "Error".to_string(),
                message: format!("Internal error during validation: {}", e),
            }),
        )
            .into_response(),
    }
}

async fn get_services_status() -> impl IntoResponse {
    Json(crate::api::services::get_all_services_status())
}

async fn health_check() -> &'static str {
    "OK"
}
