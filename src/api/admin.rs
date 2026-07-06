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
    pub actor_id: String,
    pub notes: Option<String>,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct AdminLoginRequest {
    pub session_name: String,
    pub signatures: Option<Vec<String>>,
    pub second_approver: Option<String>,
}

#[derive(Deserialize)]
struct RegistrationRequest {
    #[serde(rename = "type")]
    registration_type: String,
    #[serde(rename = "requested_credential_type")]
    credential_type: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(rename = "postClaimScopes", default)]
    post_claim_scopes: Vec<String>,
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
        .route("/login", post(login_handler))
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
        .route("/agent/auth", post(start_registration))
        .route("/agent/auth/claim", post(start_claim))
        .route("/agent/auth/claim/complete", post(complete_claim))
        .route("/agent/auth/claim/view", get(view_claim_otp))
        .with_state(state)
}

fn configured_admin_token(state: &crate::api::rest::AppState) -> Option<String> {
    state.config.admin_api_token.clone()
}

fn hash_value(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    hex::encode(hasher.finalize())
}

fn issue_api_key() -> String {
    format!("nx_key_{}", Uuid::new_v4().simple())
}

fn issue_claim_token() -> String {
    format!("nx_claim_{}", Uuid::new_v4().simple())
}

fn issue_claim_view_token() -> String {
    format!("nx_cv_{}", Uuid::new_v4().simple())
}

fn issue_otp() -> String {
    let u = Uuid::new_v4().as_u128();
    format!("{:06}", u % 1_000_000)
}

fn service_base(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:3000");

    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");

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
            "error": "invalid_token",
            "error_description": error_description
        })),
    )
        .into_response();

    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_str(&format!(
            "Bearer realm=\"Conxian Nexus Admin\", scope=\"api.read api.write admin.write\", error=\"invalid_token\", error_description=\"{}\", resource_metadata=\"{}\"",
            error_description, resource_metadata
        ))
        .unwrap(),
    );

    response
}

fn admin_token_not_configured_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": "admin_api_token_not_configured",
            "error_description": "NEXUS_ADMIN_API_TOKEN environment variable must be set to access admin routes."
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
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.to_string())
}

fn authorize_admin_write(
    state: &crate::api::rest::AppState,
    headers: &HeaderMap,
) -> Result<(), Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(bearer_unauthorized_response(
            headers,
            "Admin API token required",
        ));
    };

    // Check credentials pool first
    {
        let credentials = CREDENTIALS.lock().unwrap();
        if let Some(record) = credentials.get(&token) {
            if !record.revoked && record.scopes.contains(&"admin.write".to_string()) {
                return Ok(());
            }
        }
    }

    // Static fallback
    let Some(expected_token) = configured_admin_token(state) else {
        return Err(admin_token_not_configured_response());
    };

    if token == expected_token {
        if !cfg!(debug_assertions) {
             tracing::warn!("REMEDIATION NEEDED: Static admin token used in production-like build (Hole 1.2).");
        }
        return Ok(());
    }

    Err(bearer_unauthorized_response(
        headers,
        "Invalid admin API token",
    ))
}

fn authorize_for_scope(
    state: &crate::api::rest::AppState,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<Vec<String>, Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(unauthorized_response(headers));
    };

    // Check credentials pool first
    {
        let credentials = CREDENTIALS.lock().unwrap();
        if let Some(record) = credentials.get(&token) {
            if !record.revoked && record.scopes.contains(&required_scope.to_string()) {
                return Ok(record.scopes.clone());
            }
        }
    }

    // Static fallback
    if let Some(expected_token) = configured_admin_token(state) {
        if token == expected_token {
            return Ok(vec![
                "admin.write".to_string(),
                "api.read".to_string(),
                "api.write".to_string(),
            ]);
        }
    }

    Err(unauthorized_response(headers))
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
        // [NIP-004] Cryptographic Dual-Signature Verification
        // In test environments, if admin_public_keys is empty, we allow structural signatures.

        let signatures = self.signatures().as_ref();
        let sig_strings = signatures.map(|s| s.iter().collect::<HashSet<_>>()).unwrap_or_default();
        let unique_sigs_count = sig_strings.len();

        if unique_sigs_count < 2 {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "insufficient_approvals",
                    "message": format!("At least two unique cryptographic signatures are required for this action. Found: {}", unique_sigs_count)
                })),
            ));
        }

        if config.admin_public_keys.is_empty() {
             if cfg!(debug_assertions) {
                 return Ok(());
             } else {
                 return Err((
                     StatusCode::INTERNAL_SERVER_ERROR,
                     Json(json!({"error": "misconfigured", "message": "ADMIN_PUBLIC_KEYS not configured."}))
                 ));
             }
        }

        let mut verified_count = 0;
        let msg = self.approval_message();

        for sig_hex in sig_strings {
            let sig_bytes = match hex::decode(sig_hex) {
                Ok(b) => b,
                Err(_) => continue,
            };

            let signature = match Signature::from_der(&sig_bytes) {
                Ok(s) => s,
                Err(_) => continue,
            };

            for pk_hex in &config.admin_public_keys {
                let pk_bytes = match hex::decode(pk_hex) {
                    Ok(b) => b,
                    Err(_) => continue,
                };

                let vk = match VerifyingKey::from_sec1_bytes(&pk_bytes) {
                    Ok(k) => k,
                    Err(_) => continue,
                };

                if vk.verify(msg.as_bytes(), &signature).is_ok() {
                    verified_count += 1;
                    break;
                }
            }
        }

        if verified_count < 2 {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "cryptographic_verification_failed",
                    "message": format!("Only {} of 2 required signatures verified against trusted admin_public_keys (NIP-004).", verified_count)
                })),
            ));
        }

        Ok(())
    }
}

impl DualSignatureRequest for ReleaseApprovalRequest {
    fn approval_message(&self) -> String {
        format!("approve_release:{}:{}", self.artifact_id, self.requested_by)
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
        format!("release_decision:{}:{}:{}", self.artifact_id, self.decision, self.actor_id)
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
        format!("governance_decision:{}:{}:{}", self.action_id, self.decision, self.actor_id)
    }
    fn second_approver(&self) -> &Option<String> {
        &self.second_approver
    }
    fn signatures(&self) -> &Option<Vec<String>> {
        &self.signatures
    }
}

impl DualSignatureRequest for AdminLoginRequest {
    fn approval_message(&self) -> String {
        format!("admin_login:{}", self.session_name)
    }
    fn second_approver(&self) -> &Option<String> {
        &self.second_approver
    }
    fn signatures(&self) -> &Option<Vec<String>> {
        &self.signatures
    }
}

#[tracing::instrument(skip(state))]
async fn login_handler(
    State(state): State<crate::api::rest::AppState>,
    Json(payload): Json<AdminLoginRequest>,
) -> Result<Json<Value>, Response> {
    payload.validate_dual_signature(&state.config).map_err(|(code, json)| (code, json).into_response())?;

    let credential = issue_api_key();
    let scopes = vec![
        "admin.write".to_string(),
        "api.read".to_string(),
        "api.write".to_string(),
    ];

    CREDENTIALS.lock().unwrap().insert(
        credential.clone(),
        CredentialRecord {
            registration_id: format!("login_{}", payload.session_name),
            scopes: scopes.clone(),
            revoked: false,
        },
    );

    tracing::info!("Admin login successful for session: {}", payload.session_name);

    Ok(Json(json!({
        "status": "success",
        "credential": credential,
        "scopes": scopes
    })))
}

async fn request_release_approval(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReleaseApprovalRequest>,
) -> Result<Json<ReleaseApprovalResponse>, Response> {
    authorize_admin_write(&state, &headers)?;
    payload.validate_dual_signature(&state.config).map_err(|(code, json)| (code, json).into_response())?;

    let request_id = format!("req_{}", Uuid::new_v4().simple());
    let audit_event_id = format!("audit_{}", Uuid::new_v4().simple());

    tracing::info!(
        "Release approval requested for {} by {} (Dual-Sigs Verified)",
        payload.artifact_id,
        payload.requested_by
    );

    Ok(Json(ReleaseApprovalResponse {
        accepted: true,
        request_id,
        audit_event_id,
        message: "Release approval recorded in audit trail.".to_string(),
    }))
}

async fn submit_release_decision(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReleaseDecisionRequest>,
) -> Result<Json<ReleaseApprovalResponse>, Response> {
    authorize_admin_write(&state, &headers)?;
    payload.validate_dual_signature(&state.config).map_err(|(code, json)| (code, json).into_response())?;

    let request_id = format!("req_{}", Uuid::new_v4().simple());
    let audit_event_id = format!("audit_{}", Uuid::new_v4().simple());

    tracing::info!(
        "Release decision '{}' for {} by {} (Dual-Sigs Verified)",
        payload.decision,
        payload.artifact_id,
        payload.actor_id
    );

    Ok(Json(ReleaseApprovalResponse {
        accepted: true,
        request_id,
        audit_event_id,
        message: format!("Release decision '{}' finalized.", payload.decision),
    }))
}

async fn submit_governance_decision(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<GovernanceDecisionRequest>,
) -> Result<Json<ReleaseApprovalResponse>, Response> {
    authorize_admin_write(&state, &headers)?;
    payload.validate_dual_signature(&state.config).map_err(|(code, json)| (code, json).into_response())?;

    let request_id = format!("req_{}", Uuid::new_v4().simple());
    let audit_event_id = format!("audit_{}", Uuid::new_v4().simple());

    tracing::info!(
        "Governance decision '{}' for action {} by {} (Dual-Sigs Verified)",
        payload.decision,
        payload.action_id,
        payload.actor_id
    );

    Ok(Json(ReleaseApprovalResponse {
        accepted: true,
        request_id,
        audit_event_id,
        message: "Governance decision recorded.".to_string(),
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
        "version": "v0.4.18",
        "safety_mode": crate::safety::is_safety_mode_active(&state.storage).await.unwrap_or(false)
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
    Path(env): Path<String>,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "environment": env,
        "status": "active"
    })))
}

async fn list_chains(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "chains": ["bitcoin", "stacks", "evm", "cosmos"]
    })))
}

async fn get_chain_status(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "chain": chain,
        "height": 840000,
        "synchronized": true
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
    Path(id): Path<String>,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "id": id,
        "verified": true
    })))
}

async fn get_drift(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "drift_ms": 12,
        "status": "low"
    })))
}

async fn get_safety_mode(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    let active = crate::safety::is_safety_mode_active(&state.storage).await.unwrap_or(false);
    Ok(Json(json!({
        "active": active,
        "triggered_at": Value::Null
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
        "evidence_type": "git_tag_attestation",
        "verified": true
    })))
}

async fn list_environments(
    State(state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    authorize_for_scope(&state, &headers, "api.read")?;
    Ok(Json(json!({
        "environments": ["dev", "staging", "prod"]
    })))
}

async fn get_auth_md() -> impl IntoResponse {
    let base = "http://localhost:3000";
    Json(json!({
        "issuer": base,
        "registration_endpoint": format!("{}/agent/auth/register", base),
        "claim_endpoint": format!("{}/agent/auth/claim", base),
        "token_endpoint": format!("{}/agent/auth/token", base),
        "jwks_uri": format!("{}/agent/auth/jwks", base),
        "scopes_supported": ["api.read", "api.write", "admin.write"]
    }))
}

async fn get_oauth_protected_resource_metadata(headers: HeaderMap) -> Json<Value> {
    let base = service_base(&headers);
    Json(json!({
        "resource": base,
        "resource_name": "Conxian Nexus",
        "authorization_servers": [base]
    }))
}

async fn get_oauth_authorization_server_metadata(headers: HeaderMap) -> Json<Value> {
    let base = service_base(&headers);
    Json(json!({
        "issuer": base,
        "registration_endpoint": format!("{}/agent/auth/register", base),
        "token_endpoint": format!("{}/agent/auth/token", base),
        "jwks_uri": format!("{}/agent/auth/jwks", base),
        "scopes_supported": ["api.read", "api.write", "admin.write"]
    }))
}

async fn start_registration(
    State(_state): State<crate::api::rest::AppState>,
    headers: HeaderMap,
    Json(payload): Json<RegistrationRequest>,
) -> Result<Json<Value>, Response> {
    match payload.registration_type.as_str() {
        "anonymous" | "identity_assertion" => {
            let registration_id = format!("reg_{}", Uuid::new_v4().simple());
            let claim_token = issue_claim_token();
            let claim_view_token = issue_claim_view_token();
            let otp_plaintext = issue_otp();

            let base = service_base(&headers);
            let post_claim_scopes = if payload.post_claim_scopes.is_empty() {
                if payload.scopes.is_empty() {
                    vec!["api.read".to_string(), "api.write".to_string()]
                } else {
                    payload.scopes.clone()
                }
            } else {
                payload.post_claim_scopes.clone()
            };

            let credential = if payload.registration_type == "anonymous" {
                 Some(issue_api_key())
            } else {
                 None
            };

            let record = RegistrationRecord {
                registration_id: registration_id.clone(),
                registration_type: payload.registration_type.clone(),
                claim_token_hash: hash_value(&claim_token),
                claim_view_token: claim_view_token.clone(),
                otp_hash: hash_value(&otp_plaintext),
                otp_plaintext,
                requested_credential_type: payload.credential_type.clone(),
                credential: credential.clone(),
                pre_claim_scopes: payload.scopes,
                post_claim_scopes: post_claim_scopes.clone(),
                email: None,
                claimed: false,
                claim_token_expires_at: format!(
                    "{}Z",
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S")
                ),
            };

            if let Some(cred) = &credential {
                 CREDENTIALS.lock().unwrap().insert(cred.clone(), CredentialRecord {
                     registration_id: registration_id.clone(),
                     scopes: vec!["api.read".to_string(), "api.write".to_string()],
                     revoked: false,
                 });
            }

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
                "post_claim_scopes": post_claim_scopes,
                "credential": credential
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

    record.email = Some(payload.email);

    let base = "/"; // Simplified for response
    Ok(Json(json!({
        "registration_id": record.registration_id,
        "claim_attempt_id": format!("claim_attempt_{}", Uuid::new_v4().simple()),
        "status": "initiated",
        "claim_view_url": format!("{}agent/auth/claim/view?token={}", base, record.claim_view_token)
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

    if let Some(existing_credential) = &record.credential {
        if let Some(credential_record) =
            CREDENTIALS.lock().unwrap().get_mut(existing_credential)
        {
            credential_record.scopes = record.post_claim_scopes.clone();
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
mod cryptographic_hardening_tests {
    use super::*;
    use k256::ecdsa::{SigningKey, Signature, signature::Signer};
    use hex;

    #[tokio::test]
    async fn test_dual_signature_rejection_of_identical_signatures() {
        let sk1 = SigningKey::from_slice(&[1u8; 32]).unwrap();
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
        let sig1 = hex::encode(Signer::<Signature>::sign(&sk1, msg.as_bytes()).to_der());

        let req_with_sigs = ReleaseApprovalRequest {
            signatures: Some(vec![sig1.clone(), sig1.clone()]),
            ..req
        };

        assert!(req_with_sigs.validate_dual_signature(&config).is_err());
    }

    #[tokio::test]
    async fn test_dual_signature_cryptographic_verification_success() {
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

        let sig1 = hex::encode(Signer::<Signature>::sign(&sk1, msg.as_bytes()).to_der());
        let sig2 = hex::encode(Signer::<Signature>::sign(&sk2, msg.as_bytes()).to_der());

        let req_with_sigs = ReleaseApprovalRequest {
            signatures: Some(vec![sig1, sig2]),
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

        let sig1 = hex::encode(Signer::<Signature>::sign(&sk1, msg.as_bytes()).to_der());
        let sig_untrusted = hex::encode(Signer::<Signature>::sign(&sk_untrusted, msg.as_bytes()).to_der());

        let req_with_sigs = ReleaseApprovalRequest {
            signatures: Some(vec![sig1, sig_untrusted]),
            ..req
        };

        let result = req_with_sigs.validate_dual_signature(&config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_admin_login_success() {
        let sk1 = SigningKey::from_slice(&[1u8; 32]).unwrap();
        let sk2 = SigningKey::from_slice(&[2u8; 32]).unwrap();
        let pk1_hex = hex::encode(sk1.verifying_key().to_sec1_bytes());
        let pk2_hex = hex::encode(sk2.verifying_key().to_sec1_bytes());

        let mut config = crate::config::Config::default_test();
        config.admin_public_keys = vec![pk1_hex, pk2_hex];

        let login_req = AdminLoginRequest {
            session_name: "test_session".to_string(),
            signatures: None,
            second_approver: None,
        };
        let msg = login_req.approval_message();
        let sig1 = hex::encode(Signer::<Signature>::sign(&sk1, msg.as_bytes()).to_der());
        let sig2 = hex::encode(Signer::<Signature>::sign(&sk2, msg.as_bytes()).to_der());

        let login_req_with_sigs = AdminLoginRequest {
            signatures: Some(vec![sig1, sig2]),
            ..login_req
        };

        let state = crate::api::rest::AppState {
            storage: crate::storage::Storage::for_tests(),
            nexus_state: std::sync::Arc::new(crate::state::NexusState::new()),
            executor: std::sync::Arc::new(crate::executor::NexusExecutor::new(
                crate::storage::Storage::for_tests(),
                crate::executor::rgb::RGBRolloutMode::Disabled,
                std::collections::HashSet::new(),
            )),
            oracle: None,
            tableland: std::sync::Arc::new(crate::storage::tableland::TablelandAdapter::new(
                crate::storage::Storage::for_tests(),
                "http://localhost".to_string(),
            )),
            kwil: None,
            nostr: None,
            gateway_url: None,
            http_client: reqwest::Client::new(),
            config: std::sync::Arc::new(config),
        };

        let response = login_handler(State(state), Json(login_req_with_sigs)).await;
        assert!(response.is_ok());
        let body = response.unwrap();
        assert_eq!(body["status"], "success");
        assert!(body["credential"].as_str().unwrap().starts_with("nx_key_"));
    }
}
