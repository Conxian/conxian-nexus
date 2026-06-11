use crate::api::billing::nostr::NostrTelemetry;
use crate::api::identity::resolve_identity_handler;
use crate::api::zkml::zkml_routes;
use crate::config::Config;
use crate::executor::{ExecutionRequest, NexusExecutor};
use crate::oracle::OracleService;
use crate::state::{NexusState, MerkleProof};
use crate::storage::kwil::KwilAdapter;
use crate::storage::tableland::TablelandAdapter;
use crate::storage::Storage;
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use prometheus::{Encoder, IntCounter, TextEncoder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use tokio::net::TcpListener;

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

#[derive(Deserialize)]
pub struct EventFeedParams {
    pub cursor: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Serialize)]
pub struct EventEnvelope {
    pub sequence: u64,
    pub event_id: String,
    pub tx_id: String,
    pub block_height: u64,
    pub finality: String, // "soft" or "hard"
    pub trust_tier: String, // "T1", "T2", "T3"
    pub proof_reference: String,
    pub payload: serde_json::Value,
}

#[derive(Serialize)]
pub struct EventFeedResponse {
    pub events: Vec<EventEnvelope>,
    pub next_cursor: Option<u64>,
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
        .route("/v1/events", get(get_event_feed))
        .nest("/v1/analytics", crate::api::analytics::analytics_routes())
        .nest("/v1/zkml", zkml_routes())
        .route("/health", get(health_check))
        .route("/v1/services", get(get_services))
        .route("/v1/identity/resolve", post(resolve_identity_handler))
        .route(
            "/v1/bitvm2/verify-state-root",
            post(crate::api::rest::verify_bitvm2_state_root),
        )
        .route(
            "/v1/dlc/bond",
            post(crate::api::dlc::create_dlc_bond_handler),
        )
        .nest("/v1/billing", crate::api::billing::billing_routes())
        .nest("/admin/v1", crate::api::admin::admin_routes(state.clone()))
        .merge(crate::api::admin::public_auth_md_routes(state.clone()))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024));

    if config.experimental_apis_enabled {
        router = router.route("/v1/experimental/rebuild-state", post(rebuild_state));
    }

    router.with_state(state)
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

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("REST API server listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_proof(
    State(state): State<AppState>,
    Query(params): Query<ProofParams>,
) -> impl IntoResponse {
    match state.nexus_state.generate_merkle_proof(&params.key) {
        Some(proof) => (StatusCode::OK, Json(proof)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "key not found"})),
        )
            .into_response(),
    }
}

async fn get_mmr_proof(
    State(state): State<AppState>,
    Query(params): Query<MMRProofParams>,
) -> impl IntoResponse {
    let leaf_index = if let Some(index) = params.index {
        Some(index as usize)
    } else if let Some(tx_id) = params.tx_id {
        if tx_id.len() != 66 || !tx_id.starts_with("0x") {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid tx_id format: expected 0x-prefixed 32-byte hex string (66 chars)"})),
            ).into_response();
        }

        let leaves = state.nexus_state.leaves.lock().unwrap();
        leaves.iter().position(|l| l == &tx_id)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "provide either `index` or `tx_id`"})),
        )
            .into_response();
    };

    match leaf_index {
        Some(idx) => match state.nexus_state.get_mmr_proof_metadata(idx) {
            Some(proof) => (StatusCode::OK, Json(proof)).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("leaf at index {} was not found", idx)})),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "requested transaction was not found in MMR leaves"})),
        )
            .into_response(),
    }
}

async fn verify_state(
    State(state): State<AppState>,
    Json(proof): Json<MerkleProof>,
) -> impl IntoResponse {
    let current_root = state.nexus_state.get_state_root();
    if proof.root != current_root {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"valid": false, "error": "root mismatch"})),
        );
    }

    let valid = crate::state::verify_merkle_proof(&proof);
    (StatusCode::OK, Json(json!({ "valid": valid })))
}

async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let uptime_secs = crate::api::get_uptime();
    let start_time = crate::api::get_start_time_utc()
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());

    Json(json!({
        "status": "online",
        "version": env!("CARGO_PKG_VERSION"),
        "state_root": state.nexus_state.get_state_root(),
        "mmr_root": state.nexus_state.get_mmr_root(),
        "safety_mode": crate::safety::is_safety_mode_active(&state.storage).await.unwrap_or(false),
        "uptime_secs": uptime_secs,
        "start_time": start_time,
    }))
}

async fn get_metrics() -> impl IntoResponse {
    Json(json!({
        "total_transactions": TOTAL_TRANSACTIONS.get(),
        "active_rebalances": ACTIVE_REBALANCES.get(),
    }))
}

async fn prometheus_metrics() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(axum::body::Body::from(buffer))
        .unwrap()
}

async fn execute_tx(
    State(state): State<AppState>,
    Json(payload): Json<ExecutionRequest>,
) -> impl IntoResponse {
    match {
        if let Ok(json_payload) = serde_json::from_str::<serde_json::Value>(&payload.payload) {
            if let Some(_routing_policy) = json_payload.get("routing_policy") {
                if let Err(e) = crate::api::settlement::validate_routing_policy_metadata(&json_payload) {
                    tracing::warn!(reason = %e.reason, "Execution blocked by routing policy");
                    return (StatusCode::FORBIDDEN, Json(json!({ "error": format!("Routing policy violation: {}", e.reason) }))).into_response();
                }
            }
        }
        state.executor.submit(payload).await
    } {
        Ok(res) => {
            TOTAL_TRANSACTIONS.inc();
            (StatusCode::OK, Json(res)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn rebuild_state(State(_state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({ "status": "state rebuild is not supported in this version" })),
    )
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn get_services() -> impl IntoResponse {
    Json(crate::api::services::get_all_services_status())
}

async fn verify_bitvm2_state_root(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Some(gateway_url) = &state.gateway_url {
        let verify_url = gateway_url
            .join("/v1/bitvm2/verify-state-root")
            .expect("Invalid gateway URL join");

        match state.http_client.post(verify_url).json(&payload).send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.json::<serde_json::Value>().await.unwrap_or(json!({}));
                (status, Json(body)).into_response()
            }
            Err(err) => (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Gateway error: {}", err) })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "GATEWAY_URL not configured for BitVM2 verification" })),
        )
            .into_response()
    }
}

async fn get_rgb_contract(
    State(state): State<AppState>,
    Query(params): Query<RGBContractParams>,
) -> impl IntoResponse {
    if params.contract_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "missing contract_id"}))).into_response();
    }

    match state.executor.rgb_adapter.lookup_contract(&params.contract_id).await {
        Ok(contract) => match contract {
            Some(c) => {
                let v: serde_json::Value = serde_json::from_str(&c).unwrap_or(json!({ "raw": c }));
                (StatusCode::OK, Json(v)).into_response()
            },
            None => (StatusCode::NOT_FOUND, Json(json!({"error": "RGB contract not found"}))).into_response(),
        },
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({ "error": e.to_string() }))).into_response()
        }
    }
}

async fn get_event_feed(
    State(state): State<AppState>,
    Query(params): Query<EventFeedParams>,
) -> impl IntoResponse {
    let cursor = params.cursor.unwrap_or(0);
    let limit = params.limit.unwrap_or(50).min(250);

    let rows = match sqlx::query(
        "SELECT pos, hash, block_height FROM mmr_nodes
         WHERE pos >= $1 AND block_height > 0
         ORDER BY pos ASC LIMIT $2"
    )
    .bind(cursor as i64)
    .bind(limit as i64)
    .fetch_all(&state.storage.pg_pool)
    .await {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    };

    let events: Vec<EventEnvelope> = rows.into_iter().map(|row| {
        let pos: i64 = row.get("pos");
        let hash: Vec<u8> = row.get("hash");
        let block_height: i64 = row.get("block_height");
        let tx_id = format!("0x{}", hex::encode(hash));

        EventEnvelope {
            sequence: pos as u64,
            event_id: format!("evt_{}", pos),
            tx_id: tx_id.clone(),
            block_height: block_height as u64,
            finality: if block_height > 0 { "hard".to_string() } else { "soft".to_string() },
            trust_tier: "T1".to_string(), // Defaulting to T1 for Nexus-issued L1 events
            proof_reference: format!("/v1/mmr-proof?index={}", pos),
            payload: json!({ "tx_id": tx_id }),
        }
    }).collect();

    let next_cursor = events.last().map(|e| e.sequence + 1);

    Json(EventFeedResponse {
        events,
        next_cursor,
    }).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;
    use serde_json::json;
    use std::collections::HashSet;
    use crate::executor::rgb::RGBRolloutMode;

    fn test_router_with_state(
        enabled: bool,
        rgb_mode: RGBRolloutMode,
        known_contracts: HashSet<String>,
        nexus_state: Arc<NexusState>,
    ) -> axum::Router {
        let config = Arc::new(Config {
            experimental_apis_enabled: enabled,
            ..Config::default_test()
        });
        let storage = Storage::for_tests();
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

    fn test_router_with_options(
        enabled: bool,
        rgb_mode: RGBRolloutMode,
        known_contracts: HashSet<String>,
    ) -> axum::Router {
        test_router_with_state(
            enabled,
            rgb_mode,
            known_contracts,
            Arc::new(NexusState::new()),
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
    async fn test_get_mmr_proof_rejects_missing_index_and_tx_id() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/mmr-proof")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(response).await;
        assert_eq!(body["error"], "provide either `index` or `tx_id`");
    }

    #[tokio::test]
    async fn test_get_mmr_proof_rejects_invalid_tx_id_format() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/mmr-proof?tx_id=tx1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_json(response).await;
        assert_eq!(
            body["error"],
            "Invalid tx_id format: expected 0x-prefixed 32-byte hex string (66 chars)"
        );
    }

    #[tokio::test]
    async fn test_get_mmr_proof_prefers_index_when_both_params_present() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/mmr-proof?index=0&tx_id=bad")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response_json(response).await;
        assert_eq!(body["error"], "leaf at index 0 was not found");
    }

    #[tokio::test]
    async fn test_get_mmr_proof_returns_not_found_for_unknown_mainnet_like_tx() {
        let app = test_router();
        let mainnet_like_tx_id = "0x0000000000000000000000000000000000000000000000000000000000000000";

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/mmr-proof?tx_id={}", mainnet_like_tx_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response_json(response).await;
        assert_eq!(
            body["error"],
            "requested transaction was not found in MMR leaves"
        );
    }

    #[tokio::test]
    async fn test_get_rgb_contract_missing_contract_id_query_is_bad_request() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/rgb/contract")
                    .body(Body::empty())
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
        let contract_id = body.get("contract_id").and_then(|v| v.as_str()).unwrap_or_default();
        assert_eq!(contract_id, "rgb:contract-123");
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

    #[tokio::test]
    async fn test_get_event_feed_wires_correctly() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status() == StatusCode::OK || response.status() == StatusCode::INTERNAL_SERVER_ERROR);
    }
}
