//! REST API handlers for Conxian Nexus.

use crate::api::billing::nostr::NostrTelemetry;
use crate::api::dlc::create_dlc_bond_handler;
use crate::api::identity::resolve_identity_handler;
use crate::config::Config;
use crate::executor::{ExecutionRequest, NexusExecutor};
use crate::oracle::OracleService;
use crate::state::NexusState;
use crate::storage::kwil::KwilAdapter;
use crate::storage::tableland::TablelandAdapter;
use crate::storage::Storage;

use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use lazy_static::lazy_static;
use prometheus::{Encoder, IntGauge, TextEncoder};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

fn register_metric_best_effort(
    metric_name: &'static str,
    collector: Box<dyn prometheus::core::Collector>,
) {
    match prometheus::register(collector) {
        Ok(()) => {}
        Err(prometheus::Error::AlreadyReg) => {}
        Err(e) => {
            tracing::error!(
                error = %e,
                metric = metric_name,
                "Prometheus metrics registration failed"
            );
        }
    }
}

lazy_static! {
    pub static ref TOTAL_TRANSACTIONS: IntGauge = {
        let gauge = IntGauge::new(
            "nexus_total_transactions",
            "Total number of transactions processed",
        )
        .expect("nexus_total_transactions metric must be valid");
        register_metric_best_effort("nexus_total_transactions", Box::new(gauge.clone()));
        gauge
    };
    pub static ref ACTIVE_REBALANCES: IntGauge = {
        let gauge = IntGauge::new(
            "nexus_active_rebalances",
            "Current number of active vault rebalances",
        )
        .expect("nexus_active_rebalances metric must be valid");
        register_metric_best_effort("nexus_active_rebalances", Box::new(gauge.clone()));
        gauge
    };
}

fn init_prometheus_metrics() {
    let _ = *TOTAL_TRANSACTIONS;
    let _ = *ACTIVE_REBALANCES;
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub nexus_state: Arc<NexusState>,
    pub executor: Arc<NexusExecutor>,
    pub oracle: Option<Arc<OracleService>>,
    pub tableland: Arc<TablelandAdapter>,
    pub kwil: Option<Arc<KwilAdapter>>,
    pub nostr: Option<Arc<NostrTelemetry>>,
    pub gateway_url: Option<reqwest::Url>,
    pub http_client: reqwest::Client,
    pub config: Arc<Config>,
}

#[derive(Deserialize)]
pub struct ProofParams {
    pub key: String,
}

#[derive(Serialize)]
pub struct ProofResponse {
    pub hash: String,
    pub proof: String,
    pub root: String,
}

#[derive(Deserialize)]
pub struct MMRProofParams {
    pub index: Option<u64>,
    pub tx_id: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn app_router(
    storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
    executor: Arc<NexusExecutor>,
    oracle: Option<Arc<OracleService>>,
    tableland: Arc<TablelandAdapter>,
    kwil: Option<Arc<KwilAdapter>>,
    nostr: Option<Arc<NostrTelemetry>>,
    config: Arc<Config>,
) -> Router {
    init_prometheus_metrics();

    let gateway_url = config.gateway_url.as_ref().and_then(|s| {
        match reqwest::Url::parse(s) {
            Ok(base) => Some(base),
            Err(err) => {
                tracing::error!(url = %s, error = %err, "Invalid GATEWAY_URL in config");
                None
            }
        }
    });

    let state = AppState {
        storage,
        nexus_state,
        executor,
        oracle,
        tableland,
        kwil,
        nostr,
        gateway_url,
        http_client: reqwest::Client::new(),
        config: config.clone(),
    };

    let mut router = Router::new()
        .route("/v1/proof", get(get_proof))
        .route("/v1/mmr-proof", get(get_mmr_proof))
        .route("/v1/verify-state", post(verify_state))
        .route("/v1/status", get(get_status))
        .route("/v1/metrics", get(get_metrics))
        .route("/metrics", get(prometheus_metrics))
        .route("/v1/execute", post(execute_tx))
        .route("/v1/settlement/trigger", post(crate::api::settlement::settlement_trigger_handler))
        .nest("/v1/analytics", crate::api::analytics::analytics_routes())
        .nest("/v1/zkml", crate::api::zkml::zkml_routes())
        .nest("/v1/erp", crate::api::erp::erp_routes())
        .route("/v1/identity/resolve", get(resolve_identity_handler))
        .route("/v1/dlc/bond", post(create_dlc_bond_handler))
        .nest("/v1/billing", crate::api::billing::billing_routes());

    if config.experimental_apis_enabled {
        router = router.route("/v1/experimental/rebuild-state", post(rebuild_state));
    }

    router
        .layer(DefaultBodyLimit::max(1024 * 1024)) // 1MB limit
        .with_state(state)
}

#[allow(clippy::too_many_arguments)]
pub async fn start_rest_server(
    storage: Arc<Storage>,
    nexus_state: Arc<NexusState>,
    executor: Arc<NexusExecutor>,
    oracle: Option<Arc<OracleService>>,
    tableland: Arc<TablelandAdapter>,
    kwil: Option<Arc<KwilAdapter>>,
    nostr: Option<Arc<NostrTelemetry>>,
    port: u16,
    config: Arc<Config>,
) -> anyhow::Result<()> {
    let app = app_router(
        storage,
        nexus_state,
        executor,
        oracle,
        tableland,
        kwil,
        nostr,
        config,
    );

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("REST API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_proof(
    State(state): State<AppState>,
    Query(params): Query<ProofParams>,
) -> Result<Json<ProofResponse>, (StatusCode, Json<serde_json::Value>)> {
    let proof = state
        .nexus_state
        .generate_merkle_proof(&params.key)
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Key not found in state"})),
        ))?;

    Ok(Json(ProofResponse {
        hash: params.key,
        proof: serde_json::to_string(&proof.path).unwrap_or_default(),
        root: proof.root,
    }))
}

fn mmr_proof_error(code: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (code, Json(serde_json::json!({"error": msg.into()})))
}

async fn get_mmr_proof(
    State(state): State<AppState>,
    Query(params): Query<MMRProofParams>,
) -> Result<Json<crate::state::MMRProof>, (StatusCode, Json<serde_json::Value>)> {
    let index = match (params.index, params.tx_id) {
        (Some(i), _) => Some(i as usize),
        (None, Some(tx_id)) => state.nexus_state.get_leaf_index(&tx_id),
        _ => {
            return Err(mmr_proof_error(
                StatusCode::BAD_REQUEST,
                "provide either `index` or `tx_id`",
            ));
        }
    };

    let leaf_index = index.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "requested transaction was not found in MMR leaves"})),
        )
    })?;

    let leaf = state
        .nexus_state
        .get_leaf_by_index(leaf_index)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("leaf at index {leaf_index} was not found")})),
            )
        })?;

    let (leaf_pos, sibling_positions) = state
        .nexus_state
        .get_mmr_proof_metadata(leaf_index)
        .ok_or_else(|| {
            tracing::error!(
                "MMR proof metadata could not be computed for leaf_index {}",
                leaf_index
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("MMR proof metadata could not be computed for leaf index {leaf_index}")})),
            )
        })?;

    let mut siblings = Vec::new();
    for pos in sibling_positions {
        let row = sqlx::query("SELECT hash FROM mmr_nodes WHERE pos = $1")
            .bind(pos as i64)
            .fetch_optional(&state.storage.pg_pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, pos = pos, "Failed to fetch MMR sibling from DB");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Database error fetching MMR sibling"})),
                )
            })?;

        if let Some(r) = row {
            let hash: String = r.get("hash");
            siblings.push((pos, hash));
        } else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("MMR sibling at pos {pos} is missing from persistent storage")})),
            ));
        }
    }

    let leaf_count = state.nexus_state.leaves.lock().unwrap().len() as u64;
    let peaks_nodes = crate::state::get_mmr_peaks(leaf_count);
    let mut peaks = Vec::new();
    for pos in peaks_nodes {
        let hash = sqlx::query_scalar::<_, String>("SELECT hash FROM mmr_nodes WHERE pos = $1")
            .bind(pos as i64)
            .fetch_one(&state.storage.pg_pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, pos = pos, "Failed to fetch MMR peak from DB");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Database error fetching MMR peak"})),
                )
            })?;
        peaks.push(hash);
    }

    Ok(Json(crate::state::MMRProof {
        leaf,
        pos: leaf_pos,
        siblings,
        peaks,
        root: state.nexus_state.get_state_root(),
    }))
}

async fn verify_state(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let url = state.gateway_url.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": "Gateway validation not configured"})),
    ))?;

    let verify_url = url.join("/v1/verify").unwrap();
    let resp = state
        .http_client
        .post(verify_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Gateway verification request failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "Gateway verification unreachable"})),
            )
        })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();

    Ok(Json(serde_json::json!({
        "gateway_status": status.as_u16(),
        "gateway_response": body
    })))
}

async fn get_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let root = state.nexus_state.get_state_root();
    let height = sqlx::query_scalar::<_, i64>("SELECT MAX(height) FROM stacks_blocks")
        .fetch_optional(&state.storage.pg_pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    Json(serde_json::json!({
        "status": "ALIVE",
        "version": "0.4.11",
        "state_root": root,
        "processed_height": height,
        "experimental_apis": state.config.experimental_apis_enabled,
    }))
}

async fn get_metrics(
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "total_transactions": TOTAL_TRANSACTIONS.get(),
        "active_rebalances": ACTIVE_REBALANCES.get(),
    }))
}

async fn prometheus_metrics() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        buffer,
    )
}

async fn execute_tx(
    State(state): State<AppState>,
    Json(req): Json<ExecutionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    TOTAL_TRANSACTIONS.inc();
    let res = state.executor.submit(req).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    Ok(Json(serde_json::json!({
        "status": "accepted",
        "tx_id": res
    })))
}

async fn rebuild_state(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.nexus_state.set_initial_leaves(Vec::new());
    Ok(Json(serde_json::json!({"status": "rebuild_initiated"})))
}
