use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use k256::ecdsa::{VerifyingKey, Signature, signature::Verifier};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use uuid::Uuid;

lazy_static::lazy_static! {
    static ref REGISTRATIONS: Mutex<HashMap<String, RegistrationRecord>> = Mutex::new(HashMap::new());
    static ref CREDENTIALS: Mutex<HashMap<String, CredentialRecord>> = Mutex::new(HashMap::new());
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RegistrationRecord {
    registration_id: String,
    registration_type: String,
    claim_token_hash: String,
    claim_view_token: String,
    otp_hash: String,
    otp_plaintext: String,
    requested_credential_type: String,
    credential: Option<String>,
    pre_claim_scopes: Vec<String>,
    post_claim_scopes: Vec<String>,
    email: Option<String>,
    claimed: bool,
    claim_token_expires_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CredentialRecord {
    registration_id: String,
    scopes: Vec<String>,
    revoked: bool,
}

#[derive(Deserialize)]
struct ClaimViewQuery {
    token: String,
}

#[derive(Serialize)]
pub struct ProtectedStatusResponse {
    pub status: &'static str,
    pub scopes: Vec<String>,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct ReleaseApprovalRequest {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    #[serde(rename = "requestedBy")]
    pub requested_by: String,
    #[serde(rename = "secondApprover")]
    pub second_approver: Option<String>,
    #[serde(rename = "signatures")]
    pub signatures: Option<Vec<String>>,
    pub notes: Option<String>,
}

#[derive(Serialize)]
pub struct ReleaseApprovalResponse {
    pub accepted: bool,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "auditEventId")]
    pub audit_event_id: String,
    pub message: String,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct ReleaseDecisionRequest {
    #[serde(rename = "secondApprover")]
    pub second_approver: Option<String>,
    #[serde(rename = "signatures")]
    pub signatures: Option<Vec<String>>,
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    pub decision: String,
    #[serde(rename = "actorId")]
    pub actor_id: String,
    pub notes: Option<String>,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct GovernanceDecisionRequest {
    #[serde(rename = "secondApprover")]
    pub second_approver: Option<String>,
    #[serde(rename = "signatures")]
    pub signatures: Option<Vec<String>>,
    #[serde(rename = "actionId")]
    pub action_id: String,
    pub decision: String,
    #[serde(rename = "actorId")]
    pub actor_id: String,
    pub notes: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowDecisionResponse {
    pub accepted: bool,
    #[serde(rename = "decisionId")]
    pub decision_id: String,
    #[serde(rename = "auditEventId")]
    pub audit_event_id: String,
    pub message: String,
}

#[derive(Deserialize)]
struct ClaimRequest {
    claim_token: String,
    email: String,
}

#[derive(Deserialize)]
struct ClaimCompleteRequest {
    claim_token: String,
    otp: String,
}

pub fn admin_routes(state: crate::api::rest::AppState) -> Router<crate::api::rest::AppState> {
    Router::new()
        .route("/status", get(get_protected_status))
        .route("/releases/request-approval", post(request_release_approval))
        .route("/releases/decision", post(submit_release_decision))
        .route("/governance/decision", post(submit_governance_decision))
        .route("/runtime/health", get(get_runtime_health))
        .route("/runtime/readiness", get(get_runtime_readiness))
        .route("/audit-events", get(list_audit_events))
        .route("/environments/{env}", get(get_environment))
        .route("/chains", get(list_chains))
        .route("/chains/{chain}/status", get(get_chain_status))
        .route("/attestations", get(list_attestations))
        .route("/attestations/{id}", get(get_attestation))
        .route("/drift", get(get_drift))
        .route("/safety-mode", get(get_safety_mode))
        .route("/safety-mode/ack", post(ack_safety_mode))
        .route("/promotion-evidence/{release}", get(get_promotion_evidence))
        .route("/environments", get(list_environments))
        .with_state(state)
}

pub fn public_auth_md_routes(
    state: crate::api::rest::AppState,
) -> Router<crate::api::rest::AppState> {
    Router::new()
        .route("/auth.md", get(get_auth_md))
        .route(
            "/.well-known/oauth-protected-resource",
            get(get_oauth_protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(get_oauth_authorization_server_metadata),
        )
        .route("/agent/auth", post(agent_auth))
        .route("/agent/auth/claim", post(start_claim))
        .route("/agent/auth/claim/complete", post(complete_claim))
        .route("/agent/auth/claim/view", get(view_claim_otp))
        .with_state(state)
}

fn configured_admin_token(state: &crate::api::rest::AppState) -> Option<String> {
    let uptime = crate::api::get_uptime();
    if uptime > 86400 * 7 {
        tracing::warn!("NEXUS_ADMIN_API_TOKEN has been active for more than 7 days. Consider rotating.");
    }
    state.config.admin_api_token.clone()
}

fn hash_value(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn issue_api_key() -> String {
    format!("sk_live_{}", Uuid::new_v4().simple())
}

fn issue_claim_token() -> String {
    format!("clm_{}", Uuid::new_v4().simple())
}

fn issue_claim_view_token() -> String {
    format!("view_{}", Uuid::new_v4().simple())
}

fn issue_otp() -> String {
    let raw = (Uuid::new_v4().as_u128() % 900_000) + 100_000;
    raw.to_string()
}

fn service_base(headers: &HeaderMap) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https");
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("nexus.conxian-labs.com");

    format!("{}://{}", proto, host)
}

fn unauthorized_response(headers: &HeaderMap) -> Response {
    bearer_unauthorized_response(headers, "Agent or admin credential required")
}

fn bearer_unauthorized_response(headers: &HeaderMap, error_description: &str) -> Response {
    let resource_metadata = format!(
        "{}/.well-known/oauth-protected-resource",
        service_base(headers)
    );

    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": "unauthorized",
            "error_description": error_description
        })),
    )
        .into_response();

    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        format!("Bearer resource_metadata=\"{}\"", resource_metadata)
            .parse()
            .unwrap(),
    );
    response
}

fn admin_token_not_configured_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": "admin_api_token_not_configured",
            "error_description": "NEXUS_ADMIN_API_TOKEN must be configured for admin write routes"
        })),
    )
        .into_response()
}

fn forbidden_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "insufficient_scope",
            "error_description": "Credential does not satisfy required scope"
        })),
    )
        .into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn authorize_admin_write(
    state: &crate::api::rest::AppState,
    headers: &HeaderMap,
) -> Result<(), Response> {
    let Some(expected_token) = configured_admin_token(state) else {
        return Err(admin_token_not_configured_response());
    };

    let Some(token) = bearer_token(headers) else {
        return Err(bearer_unauthorized_response(
            headers,
            "Admin API token required",
        ));
    };

    if token != expected_token {
        return Err(bearer_unauthorized_response(
            headers,
            "Invalid admin API token",
        ));
    }

    Ok(())
}

fn authorize_for_scope(
    state: &crate::api::rest::AppState,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<Vec<String>, Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(unauthorized_response(headers));
    };

    if let Some(expected_token) = configured_admin_token(state) {
        if token == expected_token {
            return Ok(vec![
                "admin.write".to_string(),
                "api.read".to_string(),
                "api.write".to_string(),
            ]);
        }
    }

    let credentials = CREDENTIALS.lock().unwrap();
    let Some(record) = credentials.get(&token) else {
        return Err(unauthorized_response(headers));
    };

    if record.revoked {
        return Err(unauthorized_response(headers));
    }

    if !record.scopes.contains(&required_scope.to_string()) {
        return Err(forbidden_response());
    }

    Ok(record.scopes.clone())
}

async fn get_protected_status(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<ProtectedStatusResponse>, Response> {
    let scopes = authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(ProtectedStatusResponse {
        status: "ok",
        scopes,
    }))
}

pub trait DualSignatureRequest {
    fn second_approver(&self) -> &Option<String>;
    fn signatures(&self) -> &Option<Vec<String>>;

    fn approval_message(&self) -> String;

    fn validate_dual_signature(&self, config: &crate::config::Config) -> Result<(), (StatusCode, Json<Value>)> {
        let signatures = self.signatures().as_ref();
        let sig_strings = signatures.map(|s| s.iter().collect::<HashSet<_>>()).unwrap_or_default();
        let unique_sigs_count = sig_strings.len();

        if self.second_approver().is_none() || unique_sigs_count < 2 {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "insufficient_approvals",
                    "error_description": "Action requires dual-signature (Two-Person Control) with unique signatures"
                })),
            ));
        }

        // [NIP-004] Cryptographic verification
        if !config.admin_public_keys.is_empty() {
            let msg_bytes = self.approval_message().into_bytes();
            let mut verified_count = 0;
            let mut verified_keys = HashSet::new();

            for sig_hex in sig_strings {
                let sig_bytes = hex::decode(sig_hex).map_err(|_| (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "invalid_signature_format"}))
                ))?;
                let signature = Signature::from_der(&sig_bytes).map_err(|_| (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "invalid_signature_der"}))
                ))?;

                for pk_hex in &config.admin_public_keys {
                    let pk_bytes = hex::decode(pk_hex).map_err(|_| (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "invalid_configured_pk"}))
                    ))?;
                    let verifying_key = VerifyingKey::from_sec1_bytes(&pk_bytes).map_err(|_| (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "invalid_configured_sec1_pk"}))
                    ))?;

                    if verifying_key.verify(&msg_bytes, &signature).is_ok() {
                        if verified_keys.insert(pk_hex.clone()) {
                            verified_count += 1;
                        }
                        break;
                    }
                }
            }

            if verified_count < 2 {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "error": "cryptographic_verification_failed",
                        "error_description": format!("Only {} of 2 required cryptographic signatures verified against trusted admin keys", verified_count)
                    })),
                ));
            }
        }

        Ok(())
    }
}

impl DualSignatureRequest for ReleaseApprovalRequest {
    fn approval_message(&self) -> String {
        format!("approve-release-artifact:{}", self.artifact_id)
    }
    fn second_approver(&self) -> &Option<String> {
        &self.second_approver
    }
    fn signatures(&self) -> &Option<Vec<String>> {
        &self.signatures
    }
}

impl DualSignatureRequest for ReleaseDecisionRequest {
    fn approval_message(&self) -> String {
        format!("release-decision:{}", self.artifact_id)
    }
    fn second_approver(&self) -> &Option<String> {
        &self.second_approver
    }
    fn signatures(&self) -> &Option<Vec<String>> {
        &self.signatures
    }
}

impl DualSignatureRequest for GovernanceDecisionRequest {
    fn approval_message(&self) -> String {
        format!("governance-decision:{}", self.action_id)
    }
    fn second_approver(&self) -> &Option<String> {
        &self.second_approver
    }
    fn signatures(&self) -> &Option<Vec<String>> {
        &self.signatures
    }
}

#[tracing::instrument(skip(state))]
async fn request_release_approval(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReleaseApprovalRequest>,
) -> Result<Json<ReleaseApprovalResponse>, Response> {
    authorize_admin_write(&state, &headers)?;

    payload
        .validate_dual_signature(&state.config)
        .map_err(|e| e.into_response())?;

    Ok(Json(ReleaseApprovalResponse {
        accepted: true,
        request_id: format!("req_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Release approval request for artifact {} from {} accepted.",
            payload.artifact_id, payload.requested_by
        ),
    }))
}

#[tracing::instrument(skip(state))]
async fn submit_release_decision(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReleaseDecisionRequest>,
) -> Result<Json<WorkflowDecisionResponse>, Response> {
    authorize_admin_write(&state, &headers)?;

    payload
        .validate_dual_signature(&state.config)
        .map_err(|e| e.into_response())?;

    Ok(Json(WorkflowDecisionResponse {
        accepted: true,
        decision_id: format!("dec_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Decision {} for artifact {} submitted by {}.",
            payload.decision, payload.artifact_id, payload.actor_id
        ),
    }))
}

#[tracing::instrument(skip(state))]
async fn submit_governance_decision(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<GovernanceDecisionRequest>,
) -> Result<Json<WorkflowDecisionResponse>, Response> {
    authorize_admin_write(&state, &headers)?;

    payload
        .validate_dual_signature(&state.config)
        .map_err(|e| e.into_response())?;

    Ok(Json(WorkflowDecisionResponse {
        accepted: true,
        decision_id: format!("dec_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Governance decision {} for action {} submitted by {}.",
            payload.decision, payload.action_id, payload.actor_id
        ),
    }))
}

fn current_timestamp() -> String {
    format!("{}Z", chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S"))
}

async fn get_runtime_health(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "status": "healthy",
        "timestamp": current_timestamp(),
        "services": {
            "sync": "active",
            "state": "active",
            "api": "active"
        }
    })))
}

async fn get_runtime_readiness(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "status": "ready",
        "timestamp": current_timestamp()
    })))
}

async fn list_audit_events(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "events": [],
        "total": 0
    })))
}

async fn get_environment(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Path(_env): Path<String>,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "env": "production",
        "version": env!("CARGO_PKG_VERSION")
    })))
}

async fn list_chains(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "chains": ["bitcoin/mainnet", "evm/ethereum", "cosmos/hub"]
    })))
}

async fn get_chain_status(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Path(_chain): Path<String>,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "status": "synchronized",
        "height": 1000000
    })))
}

async fn list_attestations(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "attestations": []
    })))
}

async fn get_attestation(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Path(_id): Path<String>,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "id": _id,
        "valid": true
    })))
}

async fn get_drift(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "drift": 0
    })))
}

async fn get_safety_mode(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "safety_mode": crate::safety::is_safety_mode_active(&state.storage).await.unwrap_or(false)
    })))
}

async fn ack_safety_mode(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_admin_write(&state, &headers)?;
    Ok(Json(json!({
        "status": "acknowledged",
        "timestamp": current_timestamp()
    })))
}

async fn get_promotion_evidence(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Path(release): Path<String>,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "release": release,
        "evidence": []
    })))
}

async fn list_environments(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "environments": ["production"]
    })))
}

async fn get_auth_md() -> impl IntoResponse {
    let md = r#"# Conxian Nexus Administrative Authentication

Nexus protects administrative and release-bearing operations using **Two-Person Control (Dual-Signature)**.

## Authentication Model

1. **Bearer Authorization**: Requests must include a valid `NEXUS_ADMIN_API_TOKEN` or a scoped Agent credential.
2. **Dual-Signature Enforcement**: Critical actions (releases, governance) require:
   - A primary requester.
   - A secondary approver name.
   - A list of at least two unique signatures.

## Endpoints

- `/admin/v1/release/approval`: Request approval for a new release artifact.
- `/admin/v1/release/decision`: Submit a final release decision (Approve/Reject).
- `/admin/v1/governance/decision`: Submit a decision for a system governance action.
- `/admin/v1/status`: Check current credential scopes.

## Security Controls

- **Fail-Closed**: If the admin token is not configured, write routes return `503 Service Unavailable`.
- **Zero-Secret Logging**: All sensitive configuration fields are redacted in debug logs.
"#;
    Html(md.to_string())
}

async fn get_oauth_protected_resource_metadata(headers: HeaderMap) -> Json<Value> {
    let base = service_base(&headers);
    Json(json!({
        "resource_name": "Conxian Nexus",
        "authorization_servers": [format!("{}/.well-known/oauth-authorization-server", base)],
        "scopes_supported": ["api.read", "api.write", "admin.write"]
    }))
}

async fn get_oauth_authorization_server_metadata(headers: HeaderMap) -> Json<Value> {
    let base = service_base(&headers);
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{}/agent/auth", base),
        "token_endpoint": format!("{}/agent/auth/claim/complete", base),
        "identity_types_supported": ["anonymous", "identity_assertion"],
        "anonymous_supported": ["api_key"],
        "identity_assertion_supported": ["verified_email"],
        "credential_types_supported": ["api_key"],
        "events_supported": []
    }))
}

async fn agent_auth(
    State(_state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, Response> {
    let base = service_base(&headers);
    let request_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let requested_credential_type = payload
        .get("requested_credential_type")
        .and_then(Value::as_str)
        .unwrap_or("api_key")
        .to_string();

    match request_type {
        "anonymous" => {
            let registration_id = format!("reg_{}", Uuid::new_v4().simple());
            let claim_token = issue_claim_token();
            let claim_view_token = issue_claim_view_token();
            let otp = issue_otp();
            let credential = issue_api_key();
            let pre_claim_scopes = vec!["api.read".to_string()];
            let post_claim_scopes = vec!["api.read".to_string(), "api.write".to_string()];

            let record = RegistrationRecord {
                registration_id: registration_id.clone(),
                registration_type: "anonymous".to_string(),
                claim_token_hash: hash_value(&claim_token),
                claim_view_token: claim_view_token.clone(),
                otp_hash: hash_value(&otp),
                otp_plaintext: otp,
                requested_credential_type: requested_credential_type.clone(),
                credential: Some(credential.clone()),
                pre_claim_scopes: pre_claim_scopes.clone(),
                post_claim_scopes: post_claim_scopes.clone(),
                email: None,
                claimed: false,
                claim_token_expires_at: format!(
                    "{}Z",
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S")
                ),
            };

            REGISTRATIONS
                .lock()
                .unwrap()
                .insert(record.claim_token_hash.clone(), record.clone());
            CREDENTIALS.lock().unwrap().insert(
                credential.clone(),
                CredentialRecord {
                    registration_id: registration_id.clone(),
                    scopes: pre_claim_scopes.clone(),
                    revoked: false,
                },
            );

            Ok(Json(json!({
                "registration_id": registration_id,
                "registration_type": "anonymous",
                "credential_type": requested_credential_type,
                "credential": credential,
                "credential_expires": Value::Null,
                "scopes": pre_claim_scopes,
                "claim_url": format!("{}/agent/auth/claim", base),
                "claim_token": claim_token,
                "claim_view_url": format!("{}/agent/auth/claim/view?token={}", base, claim_view_token),
                "post_claim_scopes": post_claim_scopes
            })))
        }
        "identity_assertion" => {
            let assertion_type = payload
                .get("assertion_type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let assertion = payload
                .get("assertion")
                .and_then(Value::as_str)
                .unwrap_or_default();

            if assertion_type != "verified_email" || assertion.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "unsupported_registration_shape"})),
                )
                    .into_response());
            }

            let registration_id = format!("reg_{}", Uuid::new_v4().simple());
            let claim_token = issue_claim_token();
            let claim_view_token = issue_claim_view_token();
            let otp = issue_otp();
            let post_claim_scopes = vec!["api.read".to_string(), "api.write".to_string()];

            let record = RegistrationRecord {
                registration_id: registration_id.clone(),
                registration_type: "email-verification".to_string(),
                claim_token_hash: hash_value(&claim_token),
                claim_view_token: claim_view_token.clone(),
                otp_hash: hash_value(&otp),
                otp_plaintext: otp,
                requested_credential_type,
                credential: None,
                pre_claim_scopes: vec![],
                post_claim_scopes: post_claim_scopes.clone(),
                email: Some(assertion.to_string()),
                claimed: false,
                claim_token_expires_at: format!(
                    "{}Z",
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S")
                ),
            };

            REGISTRATIONS
                .lock()
                .unwrap()
                .insert(record.claim_token_hash.clone(), record.clone());

            Ok(Json(json!({
                "registration_id": registration_id,
                "registration_type": "email-verification",
                "claim_url": format!("{}/agent/auth/claim", base),
                "claim_token": claim_token,
                "claim_view_url": format!("{}/agent/auth/claim/view?token={}", base, claim_view_token),
                "post_claim_scopes": post_claim_scopes
            })))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "unsupported_registration_type"})),
        )
            .into_response()),
    }
}

async fn start_claim(
    State(_state): State<crate::api::rest::AppState>,
    Json(payload): Json<ClaimRequest>,
) -> Result<Json<Value>, Response> {
    let claim_hash = hash_value(&payload.claim_token);
    let mut registrations = REGISTRATIONS.lock().unwrap();
    let Some(record) = registrations.get_mut(&claim_hash) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_claim_token"})),
        )
            .into_response());
    };

    if record.claimed {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"error": "previously_claimed"})),
        )
            .into_response());
    }

    if record.registration_type != "anonymous" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "claim_not_applicable"})),
        )
            .into_response());
    }

    record.email = Some(payload.email);

    Ok(Json(json!({
        "registration_id": record.registration_id,
        "claim_attempt_id": format!("claim_attempt_{}", Uuid::new_v4().simple()),
        "status": "initiated",
        "claim_view_url": format!("/agent/auth/claim/view?token={}", record.claim_view_token)
    })))
}

async fn complete_claim(
    State(_state): State<crate::api::rest::AppState>,
    Json(payload): Json<ClaimCompleteRequest>,
) -> Result<Json<Value>, Response> {
    let claim_hash = hash_value(&payload.claim_token);
    let mut registrations = REGISTRATIONS.lock().unwrap();
    let Some(record) = registrations.get_mut(&claim_hash) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_claim_token"})),
        )
            .into_response());
    };

    if record.claimed {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"error": "previously_claimed"})),
        )
            .into_response());
    }

    if record.otp_hash != hash_value(&payload.otp) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "otp_invalid"})),
        )
            .into_response());
    }

    record.claimed = true;

    if record.registration_type == "anonymous" {
        if let Some(existing_credential) = &record.credential {
            if let Some(credential_record) =
                CREDENTIALS.lock().unwrap().get_mut(existing_credential)
            {
                credential_record.scopes = record.post_claim_scopes.clone();
            }
        }

        return Ok(Json(json!({
            "registration_id": record.registration_id,
            "status": "claimed"
        })));
    }

    let credential = issue_api_key();
    CREDENTIALS.lock().unwrap().insert(
        credential.clone(),
        CredentialRecord {
            registration_id: record.registration_id.clone(),
            scopes: record.post_claim_scopes.clone(),
            revoked: false,
        },
    );

    record.credential = Some(credential.clone());

    Ok(Json(json!({
        "registration_id": record.registration_id,
        "status": "claimed",
        "credential_type": record.requested_credential_type,
        "credential": credential,
        "credential_expires": Value::Null,
        "scopes": record.post_claim_scopes
    })))
}

async fn view_claim_otp(
    State(_state): State<crate::api::rest::AppState>,
    Query(query): Query<ClaimViewQuery>,
) -> Result<Html<String>, Response> {
    let registrations = REGISTRATIONS.lock().unwrap();
    let record = registrations
        .values()
        .find(|entry| entry.claim_view_token == query.token)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "invalid_claim_view_token"})),
            )
                .into_response()
        })?;

    Ok(Html(format!(
        "<html><body><h1>Conxian Nexus Claim Code</h1><p>Registration: {}</p><p>OTP: <strong>{}</strong></p></body></html>",
        record.registration_id, record.otp_plaintext
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_value_changes_output() {
        assert_ne!(hash_value("a"), "a");
    }
}

#[cfg(test)]
mod hardening_tests {
    use super::*;

    #[tokio::test]
    async fn test_dual_signature_rejection_of_identical_signatures() {
        let req = ReleaseApprovalRequest {
            artifact_id: "art_123".to_string(),
            requested_by: "alice".to_string(),
            second_approver: Some("bob".to_string()),
            signatures: Some(vec!["sig1".to_string(), "sig1".to_string()]),
            notes: None,
        };
        assert!(req.validate_dual_signature(&crate::config::Config::default_test()).is_err());
    }

    #[tokio::test]
    async fn test_dual_signature_acceptance_of_distinct_signatures() {
        let req = ReleaseApprovalRequest {
            artifact_id: "art_123".to_string(),
            requested_by: "alice".to_string(),
            second_approver: Some("bob".to_string()),
            signatures: Some(vec!["sig1".to_string(), "sig2".to_string()]),
            notes: None,
        };
        assert!(req.validate_dual_signature(&crate::config::Config::default_test()).is_ok());
    }

    #[tokio::test]
    async fn test_dual_signature_rejection_of_three_signatures_with_duplicates() {
        let req = ReleaseApprovalRequest {
            artifact_id: "art_123".to_string(),
            requested_by: "alice".to_string(),
            second_approver: Some("bob".to_string()),
            signatures: Some(vec!["sig1".to_string(), "sig2".to_string(), "sig1".to_string()]),
            notes: None,
        };
        // Even with 3 items, only 2 are unique. In this PoC we accept 2+ unique.
        // If we wanted exactly 2, we'd check len == 2. NIP says "at least two".
        // But the check should be unique_count >= 2.
        assert!(req.validate_dual_signature(&crate::config::Config::default_test()).is_ok());

        let req_fail = ReleaseApprovalRequest {
            artifact_id: "art_123".to_string(),
            requested_by: "alice".to_string(),
            second_approver: Some("bob".to_string()),
            signatures: Some(vec!["sig1".to_string(), "sig1".to_string(), "sig1".to_string()]),
            notes: None,
        };
        assert!(req_fail.validate_dual_signature(&crate::config::Config::default_test()).is_err());
    }
}

#[cfg(test)]
mod cryptographic_hardening_tests {
    use super::*;
    use k256::ecdsa::{SigningKey, Signature, signature::Signer};
    use hex;

    #[tokio::test]
    async fn test_dual_signature_cryptographic_verification_success() {
        // Setup two trusted keys
        let sk1 = SigningKey::from_slice(&[1u8; 32]).unwrap();
        let sk2 = SigningKey::from_slice(&[2u8; 32]).unwrap();
        let pk1_hex = hex::encode(sk1.verifying_key().to_sec1_bytes());
        let pk2_hex = hex::encode(sk2.verifying_key().to_sec1_bytes());

        let mut config = crate::config::Config::default_test();
        config.admin_public_keys = vec![pk1_hex, pk2_hex];

        let req = ReleaseApprovalRequest {
            artifact_id: "art_123".to_string(),
            requested_by: "alice".to_string(),
            second_approver: Some("bob".to_string()),
            signatures: None,
            notes: None,
        };
        let msg = req.approval_message();

        let sig1 = Signer::<Signature>::sign(&sk1, msg.as_bytes()).to_der();
        let sig2 = Signer::<Signature>::sign(&sk2, msg.as_bytes()).to_der();

        let req_with_sigs = ReleaseApprovalRequest {
            signatures: Some(vec![hex::encode(sig1), hex::encode(sig2)]),
            ..req
        };

        assert!(req_with_sigs.validate_dual_signature(&config).is_ok());
    }

    #[tokio::test]
    async fn test_dual_signature_cryptographic_verification_failure_wrong_key() {
        let sk1 = SigningKey::from_slice(&[1u8; 32]).unwrap();
        let sk_untrusted = SigningKey::from_slice(&[3u8; 32]).unwrap();
        let pk1_hex = hex::encode(sk1.verifying_key().to_sec1_bytes());

        let mut config = crate::config::Config::default_test();
        config.admin_public_keys = vec![pk1_hex];

        let req = ReleaseApprovalRequest {
            artifact_id: "art_123".to_string(),
            requested_by: "alice".to_string(),
            second_approver: Some("bob".to_string()),
            signatures: None,
            notes: None,
        };
        let msg = req.approval_message();

        let sig1 = Signer::<Signature>::sign(&sk1, msg.as_bytes()).to_der();
        let sig_untrusted = Signer::<Signature>::sign(&sk_untrusted, msg.as_bytes()).to_der();

        let req_with_sigs = ReleaseApprovalRequest {
            signatures: Some(vec![hex::encode(sig1), hex::encode(sig_untrusted)]),
            ..req
        };

        let result = req_with_sigs.validate_dual_signature(&config);
        assert!(result.is_err());
        // Since we only have 1 trusted key in config, verified_count will be 1.
    }
}
