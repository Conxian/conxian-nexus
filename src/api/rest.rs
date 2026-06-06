use crate::api::billing::nostr::NostrTelemetry;
use crate::api::identity::resolve_identity_handler;
use crate::api::zkml::zkml_routes;
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
use prometheus::{Encoder, IntCounter, TextEncoder};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

lazy_static::lazy_static! {
    static ref TOTAL_TRANSACTIONS: IntCounter = IntCounter::new("nexus_total_transactions", "Total transactions submitted").unwrap();
    static ref ACTIVE_REBALANCES: IntCounter = IntCounter::new("nexus_active_rebalances", "Number of currently active rebalances").unwrap();
}

fn init_prometheus_metrics() {
    prometheus::register(Box::new(TOTAL_TRANSACTIONS.clone())).ok();
    prometheus::register(Box::new(ACTIVE_REBALANCES.clone())).ok();
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

#[derive(Deserialize)]
pub struct RGBContractParams {
    pub contract_id: String,
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

    let gateway_url = config
        .gateway_url
        .as_ref()
        .and_then(|s| match reqwest::Url::parse(s) {
            Ok(base) => Some(base),
            Err(err) => {
                tracing::error!(url = %s, error = %err, "Invalid GATEWAY_URL in config");
                None
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
        .route(
            "/v1/settlement/trigger",
            post(crate::api::settlement::settlement_trigger_handler),
        )
        .route("/v1/rgb/contract", get(get_rgb_contract))
        .nest("/v1/analytics", crate::api::analytics::analytics_routes())
        .nest("/v1/zkml", zkml_routes())
        .nest("/v1/erp", crate::api::erp::erp_routes())
        .route("/v1/identity/resolve", get(resolve_identity_handler))
        .route(
            "/v1/dlc/bond",
            post(crate::api::dlc::create_dlc_bond_handler),
        )
        .nest("/v1/billing", crate::api::billing::billing_routes())
        .nest("/admin/v1", crate::api::admin::admin_routes())
        .merge(crate::api::admin::public_auth_md_routes());

    if config.experimental_apis_enabled {
        router = router.route("/v1/experimental/rebuild-state", post(rebuild_state));
    }

    router
        .layer(DefaultBodyLimit::max(1024 * 1024))
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

fn mmr_proof_error(
    code: StatusCode,
    msg: impl Into<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    (code, Json(serde_json::json!({"error": msg.into()})))
}

async fn get_mmr_proof(
    State(state): State<AppState>,
    Query(params): Query<MMRProofParams>,
) -> Result<Json<crate::state::MMRProof>, (StatusCode, Json<serde_json::Value>)> {
    let index = match (params.index, params.tx_id) {
        (Some(i), _) => Some(i as usize),
        (None, Some(tx_id)) => {
            if !tx_id.starts_with("0x") || tx_id.len() != 66 {
                return Err(mmr_proof_error(
                    StatusCode::BAD_REQUEST,
                    "Invalid tx_id format: expected 0x-prefixed 32-byte hex string (66 chars)",
                ));
            }
            state.nexus_state.get_leaf_index(&tx_id)
        }
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
                Json(
                    serde_json::json!({"error": format!("MMR sibling at pos {pos} is missing from persistent storage")}),
                ),
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

async fn get_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let root = state.nexus_state.get_state_root();
    let height = sqlx::query_scalar::<_, i64>("SELECT MAX(height) FROM stacks_blocks")
        .fetch_optional(&state.storage.pg_pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

    let mut conn = state
        .storage
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| {
            tracing::error!("Redis connection failed for status check: {}", e);
            e
        })
        .ok();

    let safety_mode = if let Some(ref mut c) = conn {
        redis::cmd("GET")
            .arg("nexus:safety_mode")
            .query_async::<Option<bool>>(c)
            .await
            .unwrap_or(Some(false))
            .unwrap_or(false)
    } else {
        false
    };

    let uptime = crate::api::get_uptime();
    let start_time = crate::api::get_start_time_utc()
        .map(|t| t.to_rfc3339())
        .unwrap_or_default();

    Json(serde_json::json!({
        "status": "ALIVE",
        "version": "0.4.12",
        "state_root": root,
        "processed_height": height,
        "experimental_apis": state.config.experimental_apis_enabled,
        "uptime_secs": uptime,
        "start_time": start_time,
        "safety_mode": safety_mode,
    }))
}

async fn get_metrics(State(_state): State<AppState>) -> Json<serde_json::Value> {
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

async fn get_rgb_contract(
    State(state): State<AppState>,
    Query(params): Query<RGBContractParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let contract = state
        .executor
        .rgb_adapter
        .lookup_contract(&params.contract_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    match contract {
        Some(c) => {
            let json: serde_json::Value = serde_json::from_str(&c).unwrap_or_default();
            Ok(Json(json))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "RGB contract not found"})),
        )),
    }
}

async fn rebuild_state(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.nexus_state.set_initial_leaves(Vec::new());
    Ok(Json(serde_json::json!({"status": "rebuild_initiated"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::executor::rgb::RGBRolloutMode;
    use crate::executor::NexusExecutor;
    use crate::state::NexusState;
    use crate::storage::tableland::TablelandAdapter;
    use crate::storage::Storage;
    use axum::body::Body;
    use axum::http::Request;
    use axum::response::Response;
    use serde_json::json;
    use std::collections::HashSet;
    use tower::ServiceExt;

    fn test_router_with_options(
        enabled: bool,
        rgb_mode: RGBRolloutMode,
        known_contracts: HashSet<String>,
    ) -> axum::Router {
        let mut config_value = Config::default_test();
        config_value.experimental_apis_enabled = enabled;
        let config = Arc::new(config_value);
        let storage = Storage::for_tests();
        let nexus_state = Arc::new(NexusState::new());
        let executor = Arc::new(NexusExecutor::new(
            storage.clone(),
            rgb_mode,
            known_contracts,
        ));
        let tableland = Arc::new(TablelandAdapter::new(
            storage.clone(),
            config.tableland_base_url.clone(),
        ));

        app_router(
            storage,
            nexus_state,
            executor,
            None,
            tableland,
            None,
            None,
            config,
        )
    }

    fn test_router_with_experimental_apis(enabled: bool) -> axum::Router {
        test_router_with_options(enabled, RGBRolloutMode::Disabled, HashSet::new())
    }

    fn test_router() -> axum::Router {
        test_router_with_experimental_apis(true)
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn test_app_router_wires_billing_generate_key_route() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/billing/generate-key")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"organization_id": "   "}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn test_app_router_wires_dlc_bond_route() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/dlc/bond")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "bond_id": "",
                            "principal_sbtc": 0,
                            "expiry_height": 100,
                            "coupon_rate": 0.05
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_rgb_contract_invalid_contract_id_maps_to_500() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/rgb/contract?contract_id=invalid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response_json(response).await;
        assert_eq!(
            body["error"],
            "Invalid RGB contract ID format: must start with rgb: and have sufficient length"
        );
    }

    #[tokio::test]
    async fn test_get_rgb_contract_disabled_adapter_maps_to_500() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/rgb/contract?contract_id=rgb:contract-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response_json(response).await;
        assert_eq!(body["error"], "RGB adapter is disabled");
    }

    #[tokio::test]
    async fn test_get_rgb_contract_returns_ok_with_payload_in_shadow_mode() {
        let app = test_router_with_options(true, RGBRolloutMode::Shadow, HashSet::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/rgb/contract?contract_id=rgb:contract-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["contract_id"], "rgb:contract-123");
        assert_eq!(body["status"], "verified");
        assert_eq!(body["mode"], "shadow");
    }

    #[tokio::test]
    async fn test_get_rgb_contract_returns_not_found_in_active_mode_when_missing() {
        let app = test_router_with_options(true, RGBRolloutMode::Active, HashSet::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/rgb/contract?contract_id=rgb:contract-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response_json(response).await;
        assert_eq!(body["error"], "RGB contract not found");
    }

    #[tokio::test]
    async fn test_app_router_wires_track_signature_route() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/billing/telemetry/track-signature")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "api_key": "cxl_test",
                            "signature_hash": "hash",
                            "timestamp": 1700000000,
                            "hmac": "invalid"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_app_router_excludes_experimental_routes_when_disabled() {
        let app = test_router_with_experimental_apis(false);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/experimental/rebuild-state")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
