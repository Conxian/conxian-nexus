use crate::api::analytics::analytics_routes;
use crate::api::billing::billing_routes;
use crate::api::billing::nostr::NostrTelemetry;
use crate::api::dlc::dlc_routes;
use crate::api::erp::erp_routes;
use crate::api::identity::identity_routes;
use crate::api::services::services_routes;
use crate::api::settlement::settlement_routes;
use crate::api::zkml::zkml_routes;
use crate::config::Config;
use crate::executor::rgb::RGBRolloutMode;
use crate::executor::{ExecutionRequest, NexusExecutor};
use crate::oracle::OracleService;
use crate::state::NexusState;
use crate::storage::kwil::KwilAdapter;
use crate::storage::tableland::TablelandAdapter;
use crate::storage::Storage;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use prometheus::{opts, register_int_gauge, IntGauge};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::net::TcpListener;

lazy_static::lazy_static! {
    static ref TX_COUNT: IntGauge = register_int_gauge!(opts!(
        "nexus_transactions_total",
        "Total number of transactions processed by Nexus Glass Node"
    ))
    .unwrap();

    static ref REBALANCE_COUNT: IntGauge = register_int_gauge!(opts!(
        "nexus_rebalances_total",
        "Total number of rebalances executed"
    ))
    .unwrap();
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

#[derive(Deserialize, Debug)]
pub struct ProofParams {
    pub key: String,
}

#[derive(Serialize)]
pub struct ProofResponse {
    pub root: String,
    pub proof: String,
}

#[derive(Deserialize, Debug)]
pub struct MMRProofParams {
    pub index: Option<u64>,
    pub tx_id: Option<String>,
}

#[derive(Deserialize)]
pub struct RGBContractParams {
    pub contract_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub safety_mode: bool,
}

/// Proof manifest for the narrow proof surface (Issue #149)
#[derive(Serialize, Deserialize)]
pub struct ProofManifest {
    pub health: HealthStatus,
    pub proof_routes: ProofRoutes,
    pub state_root: Option<String>,
    pub mmr_info: MmrInfo,
    pub service: ServiceMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub safety_mode: bool,
    pub uptime_seconds: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct ProofRoutes {
    /// GET /v1/proof - Get proof for a specific key
    pub proof_endpoint: String,
    /// GET /v1/mmr-proof - Get MMR inclusion proof
    pub mmr_proof_endpoint: String,
    /// GET /health - Service health check
    pub health_endpoint: String,
    /// POST /v1/submit - Submit transaction
    pub submit_endpoint: String,
}

#[derive(Serialize, Deserialize)]
pub struct MmrInfo {
    /// Current number of leaves in the MMR tree
    pub leaf_count: Option<usize>,
    /// Current MMR peaks
    pub peaks: Vec<String>,
    /// Whether MMR is initialized
    pub initialized: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ServiceMetadata {
    pub version: String,
    pub proof_surface_version: String,
    pub supported_chains: Vec<String>,
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
        config,
    };

    Router::new()
        .route("/health", get(health_handler))
        .route("/v1/proof", get(get_proof))
        .route("/v1/proof/manifest", get(get_proof_manifest))  // Narrow proof surface
        .route("/v1/submit", post(submit_transaction))
        .route("/v1/status", get(health_handler))
        .route("/v1/mmr-proof", get(get_mmr_proof))
        .nest("/v1/analytics", analytics_routes())
        .nest("/v1/billing", billing_routes())
        .nest("/v1/zkml", zkml_routes())
        .nest("/admin/v1", crate::api::admin::admin_routes(state.clone()))
        .nest("/v1/settlement", settlement_routes())
        .nest("/v1/identity", identity_routes())
        .nest("/v1/dlc", dlc_routes())
        .nest("/v1/erp", erp_routes())
        .nest("/v1/services", services_routes())
        .nest("/v1/bitvm2", bitvm_routes())
        .nest("/v1/evm", evm_routes())
        .nest("/v1/cosmos", cosmos_routes())
        .nest("/v1/stacks", stacks_routes())
        .nest("/v1/rgb", rgb_routes())
        .with_state(state)
}

pub fn bitvm_routes() -> Router<AppState> {
    Router::new().route("/verify-state-root", post(verify_bitvm_transition))
}

pub fn evm_routes() -> Router<AppState> {
    Router::new().route("/verify-receipt", post(verify_evm_receipt))
}

pub fn cosmos_routes() -> Router<AppState> {
    Router::new().route("/verify-ibc", post(verify_cosmos_ibc))
}

pub fn stacks_routes() -> Router<AppState> {
    Router::new().route("/verify-tx", post(verify_stacks_tx))
}

pub fn rgb_routes() -> Router<AppState> {
    Router::new().route("/contract", get(get_rgb_contract))
}

async fn get_rgb_contract(
    State(state): State<AppState>,
    Query(params): Query<RGBContractParams>,
) -> impl IntoResponse {
    match state
        .executor
        .rgb_adapter
        .lookup_contract(&params.contract_id)
        .await
    {
        Ok(Some(metadata)) => (StatusCode::OK, Json(metadata)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Contract not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn verify_bitvm_transition(
    State(state): State<AppState>,
    Json(payload): Json<crate::executor::bitvm::BitVMTransition>,
) -> impl IntoResponse {
    match state
        .executor
        .bitvm_adapter
        .verify_transition(&payload)
        .await
    {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn verify_evm_receipt(
    State(state): State<AppState>,
    Json(payload): Json<crate::executor::evm::EVMReceiptProof>,
) -> impl IntoResponse {
    match state
        .executor
        .evm_adapter
        .verify_receipt_proof(&payload)
        .await
    {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn verify_cosmos_ibc(
    State(state): State<AppState>,
    Json(payload): Json<crate::executor::cosmos::IBCClientUpdate>,
) -> impl IntoResponse {
    match state
        .executor
        .cosmos_adapter
        .verify_client_update(&payload)
        .await
    {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn verify_stacks_tx(
    State(state): State<AppState>,
    Json(payload): Json<crate::executor::stacks::StacksTransaction>,
) -> impl IntoResponse {
    match state
        .executor
        .stacks_adapter
        .verify_transaction(&payload)
        .await
    {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
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

pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

#[tracing::instrument(skip(state))]
async fn get_proof(
    State(state): State<AppState>,
    Query(params): Query<ProofParams>,
) -> impl IntoResponse {
    let (root, proof) = state.nexus_state.generate_proof(&params.key);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "root": root, "proof": proof })),
    )
        .into_response()
}

#[tracing::instrument(skip(state))]
async fn get_mmr_proof(
    State(state): State<AppState>,
    Query(params): Query<MMRProofParams>,
) -> impl IntoResponse {
    let leaf_index = if let Some(idx) = params.index {
        Some(idx as usize)
    } else if let Some(tx_id) = params.tx_id {
        if !tx_id.starts_with("0x") || tx_id.len() != 66 {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid tx_id format"})),
            )
                .into_response();
        }
        state.nexus_state.get_leaf_index(&tx_id)
    } else {
        None
    };

    match leaf_index {
        Some(idx) => {
            if let Some(leaf) = state.nexus_state.get_leaf_by_index(idx) {
                if let Some((pos, _)) = state.nexus_state.get_mmr_proof_metadata(idx) {
                    let proof = state.nexus_state.assemble_mmr_proof(leaf, pos, vec![]);
                    return (StatusCode::OK, Json(proof)).into_response();
                }
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to generate MMR proof"})),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Leaf not found"})),
        )
            .into_response(),
    }
}

#[tracing::instrument(skip(state))]
async fn submit_transaction(
    State(state): State<AppState>,
    Json(request): Json<ExecutionRequest>,
) -> impl IntoResponse {
    match state.executor.submit(request).await {
        Ok(tx_id) => {
            TX_COUNT.inc();
            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({ "tx_id": tx_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let safety_mode = crate::safety::is_safety_mode_active(&state.storage)
        .await
        .unwrap_or(false);

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        safety_mode,
    })
}

/// Proof manifest handler for the narrow proof surface (Issue #149)
async fn get_proof_manifest(State(state): State<AppState>) -> impl IntoResponse {
    let safety_mode = crate::safety::is_safety_mode_active(&state.storage)
        .await
        .unwrap_or(false);

    // Get MMR information from nexus state using the public get_mmr_state method
    let (mmr_peaks_raw, mmr_leaf_count) = state.nexus_state.get_mmr_state();
    let mmr_peaks = mmr_peaks_raw.iter().map(hex::encode).collect::<Vec<_>>();

    // Get state root
    let state_root = {
        let (root, _) = state.nexus_state.generate_proof("state_root");
        if root.is_empty() { None } else { Some(root) }
    };

    Json(ProofManifest {
        health: HealthStatus {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            safety_mode,
            uptime_seconds: None,
        },
        proof_routes: ProofRoutes {
            proof_endpoint: "/v1/proof?key=<key>".to_string(),
            mmr_proof_endpoint: "/v1/mmr-proof?index=<n>".to_string(),
            health_endpoint: "/health".to_string(),
            submit_endpoint: "/v1/submit".to_string(),
        },
        state_root,
        mmr_info: MmrInfo {
            leaf_count: Some(mmr_leaf_count),
            peaks: mmr_peaks,
            initialized: true,
        },
        service: ServiceMetadata {
            version: env!("CARGO_PKG_VERSION").to_string(),
            proof_surface_version: "1.0.0".to_string(),
            supported_chains: vec![
                "stacks".to_string(),
                "bitcoin".to_string(),
                "evm".to_string(),
                "cosmos".to_string(),
            ],
        },
    })
}

fn init_prometheus_metrics() {
    let _ = &*TX_COUNT;
    let _ = &*REBALANCE_COUNT;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::rgb::RGBRolloutMode;
    use crate::storage::tableland::TablelandAdapter;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    async fn test_router_with_state(
        enabled: bool,
        rgb_mode: RGBRolloutMode,
        known_contracts: HashSet<String>,
    ) -> axum::Router {
        let mut config = Config::default_test();
        config.experimental_apis_enabled = enabled;
        let config = Arc::new(config);
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
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

    #[tokio::test]
    async fn test_health_check() {
        let app = test_router_with_state(true, RGBRolloutMode::Disabled, HashSet::new()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let res: HealthResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(res.status, "ok");
    }

    /// Test for Issue #149: Narrow proof surface manifest endpoint
    #[tokio::test]
    async fn test_proof_manifest_returns_narrow_surface() {
        let app = test_router_with_state(true, RGBRolloutMode::Disabled, HashSet::new()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/proof/manifest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let manifest: ProofManifest = serde_json::from_slice(&body).unwrap();

        // Verify health status
        assert_eq!(manifest.health.status, "ok");
        assert_eq!(manifest.health.version, env!("CARGO_PKG_VERSION"));

        // Verify proof routes are documented
        assert!(!manifest.proof_routes.proof_endpoint.is_empty());
        assert!(!manifest.proof_routes.mmr_proof_endpoint.is_empty());
        assert!(!manifest.proof_routes.health_endpoint.is_empty());

        // Verify MMR info is present
        assert!(manifest.mmr_info.initialized);
        // When no transactions have been processed, MMR should be empty
        assert_eq!(manifest.mmr_info.leaf_count, Some(0));

        // Verify service metadata
        assert_eq!(manifest.service.proof_surface_version, "1.0.0");
        assert!(!manifest.service.supported_chains.is_empty());
    }

    fn valid_rgb_contract_id() -> &'static str {
        "rgb:test123456_nia_long_enough_id_for_validation"
    }

    fn valid_tx_id() -> String {
        format!("0x{}", "a".repeat(64))
    }

    #[tokio::test]
    async fn test_rgb_contract_lookup_shadow_mode_returns_ok() {
        let app = test_router_with_state(true, RGBRolloutMode::Shadow, HashSet::new()).await;
        let uri = format!("/v1/rgb/contract?contract_id={}", valid_rgb_contract_id());

        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload.get("contract_id").and_then(Value::as_str),
            Some(valid_rgb_contract_id())
        );
    }

    #[tokio::test]
    async fn test_rgb_contract_lookup_disabled_returns_internal_server_error() {
        let app = test_router_with_state(true, RGBRolloutMode::Disabled, HashSet::new()).await;
        let uri = format!("/v1/rgb/contract?contract_id={}", valid_rgb_contract_id());

        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_text = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_text.contains("RGB adapter is disabled"));
    }

    #[tokio::test]
    async fn test_mmr_proof_rejects_invalid_tx_id_format() {
        let app = test_router_with_state(true, RGBRolloutMode::Disabled, HashSet::new()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/mmr-proof?tx_id=not_hex_prefixed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_mmr_proof_returns_not_found_for_missing_tx_id() {
        let app = test_router_with_state(true, RGBRolloutMode::Disabled, HashSet::new()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/mmr-proof?tx_id={}", valid_tx_id()))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mmr_proof_returns_internal_error_for_missing_leaf_index() {
        let app = test_router_with_state(true, RGBRolloutMode::Disabled, HashSet::new()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/mmr-proof?index=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_mmr_proof_returns_ok_for_existing_leaf_index() {
        let mut config = Config::default_test();
        config.experimental_apis_enabled = true;
        let config = Arc::new(config);
        let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
        let nexus_state = Arc::new(NexusState::new());
        let tx_id = valid_tx_id();
        nexus_state.update_state(&tx_id, 100);

        let executor = Arc::new(NexusExecutor::new(
            storage.clone(),
            RGBRolloutMode::Disabled,
            HashSet::new(),
        ));
        let tableland = Arc::new(TablelandAdapter::new(
            storage.clone(),
            config.tableland_base_url.clone(),
        ));

        let app = app_router(
            storage,
            nexus_state,
            executor,
            None,
            tableland,
            None,
            None,
            config,
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/mmr-proof?index=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload.get("leaf").and_then(Value::as_str),
            Some(tx_id.as_str())
        );
        assert_eq!(payload.get("pos").and_then(Value::as_u64), Some(0));
    }
}
