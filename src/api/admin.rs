use crate::api::rest::AppState;
use crate::config::Config;
use axum::{
    extract::{Json, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

lazy_static::lazy_static! {
    static ref REGISTRATIONS: Mutex<HashMap<String, RegistrationRecord>> = Mutex::new(HashMap::new());
    static ref CREDENTIALS: Mutex<HashMap<String, CredentialRecord>> = Mutex::new(HashMap::new());
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
struct CredentialRecord {
    registration_id: String,
    scopes: Vec<String>,
    revoked: bool,
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

#[derive(Serialize)]
pub struct WorkflowDecisionResponse {
    pub accepted: bool,
    #[serde(rename = "decisionId")]
    pub decision_id: String,
    #[serde(rename = "auditEventId")]
    pub audit_event_id: String,
    pub message: String,
}

#[derive(Serialize)]
struct ProtectedStatusResponse {
    status: &'static str,
    scopes: Vec<String>,
}

#[derive(Deserialize)]
pub struct ReleaseApprovalRequest {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    #[serde(rename = "requestedBy")]
    pub requested_by: String,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct ReleaseDecisionRequest {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    pub decision: String,
    #[serde(rename = "actorId")]
    pub actor_id: String,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct GovernanceDecisionRequest {
    #[serde(rename = "actionId")]
    pub action_id: String,
    pub decision: String,
    #[serde(rename = "actorId")]
    pub actor_id: String,
    pub notes: Option<String>,
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

#[derive(Deserialize)]
struct ClaimViewQuery {
    token: String,
}

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_protected_status))
        .route("/releases/request-approval", post(request_release_approval))
        .route("/releases/decision", post(submit_release_decision))
        .route("/governance/decision", post(submit_governance_decision))
}

pub fn public_auth_md_routes() -> Router<AppState> {
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

fn authorize_admin_write(headers: &HeaderMap, config: &Config) -> Result<(), Response> {
    let Some(expected_token) = &config.admin_api_token else {
        return Err(admin_token_not_configured_response());
    };

    let Some(token) = bearer_token(headers) else {
        return Err(bearer_unauthorized_response(
            headers,
            "Admin API token required",
        ));
    };

    if token != **expected_token {
        return Err(bearer_unauthorized_response(
            headers,
            "Invalid admin API token",
        ));
    }

    Ok(())
}

fn authorize_for_scope(
    headers: &HeaderMap,
    config: &Config,
    required_scope: &str,
) -> Result<Vec<String>, Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(unauthorized_response(headers));
    };

    if let Some(expected_token) = &config.admin_api_token {
        if token == **expected_token {
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

    if !record.scopes.iter().any(|scope| scope == required_scope) {
        return Err(forbidden_response());
    }

    Ok(record.scopes.clone())
}

async fn get_protected_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProtectedStatusResponse>, Response> {
    let scopes = authorize_for_scope(&headers, &state.config, "api.read")?;
    Ok(Json(ProtectedStatusResponse {
        status: "ok",
        scopes,
    }))
}

async fn request_release_approval(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReleaseApprovalRequest>,
) -> Result<Json<ReleaseApprovalResponse>, Response> {
    authorize_admin_write(&headers, &state.config)?;

    Ok(Json(ReleaseApprovalResponse {
        accepted: true,
        request_id: format!("req_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Accepted bootstrap release approval request for artifact {} from {}.",
            payload.artifact_id, payload.requested_by
        ),
    }))
}

async fn submit_release_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ReleaseDecisionRequest>,
) -> Result<Json<WorkflowDecisionResponse>, Response> {
    authorize_admin_write(&headers, &state.config)?;

    Ok(Json(WorkflowDecisionResponse {
        accepted: true,
        decision_id: format!("release_decision_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Accepted bootstrap release decision '{}' for artifact {} from actor {}.",
            payload.decision, payload.artifact_id, payload.actor_id
        ),
    }))
}

async fn submit_governance_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GovernanceDecisionRequest>,
) -> Result<Json<WorkflowDecisionResponse>, Response> {
    authorize_admin_write(&headers, &state.config)?;

    Ok(Json(WorkflowDecisionResponse {
        accepted: true,
        decision_id: format!("governance_decision_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Accepted bootstrap governance decision '{}' for action {} from actor {}.",
            payload.decision, payload.action_id, payload.actor_id
        ),
    }))
}

async fn get_auth_md(State(_state): State<AppState>, headers: HeaderMap) -> Html<String> {
    let base = service_base(&headers);
    Html(format!(
        r#"# auth.md

Conxian Nexus supports agent-to-product registration for protected API access.

## Discover
- Protected Resource Metadata: `{base}/.well-known/oauth-protected-resource`
- Authorization Server Metadata: `{base}/.well-known/oauth-authorization-server`
- Registration endpoint: `{base}/agent/auth`

## Supported registration flows
- `anonymous`
- `identity_assertion` with `assertion_type = verified_email`

## Anonymous registration example
```json
{{
  "type": "anonymous",
  "requested_credential_type": "api_key"
}}
```

## Verified email registration example
```json
{{
  "type": "identity_assertion",
  "assertion_type": "verified_email",
  "assertion": "user@example.com",
  "requested_credential_type": "api_key"
}}
```

## Claim completion
- Start claim: `POST {base}/agent/auth/claim`
- Complete claim: `POST {base}/agent/auth/claim/complete`
- View OTP: `GET {base}/agent/auth/claim/view?token=...`

## Credential use
Pass the credential as `Authorization: Bearer <credential>`.

Pre-claim credentials receive `api.read`.
Post-claim credentials receive `api.read` and `api.write`.

## Protected route example
- `GET {base}/admin/v1/status`
"#
    ))
}

async fn get_oauth_protected_resource_metadata(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Json<Value> {
    let base = service_base(&headers);
    Json(json!({
        "resource": format!("{}/", base),
        "resource_name": "Conxian Nexus",
        "authorization_servers": [base],
        "scopes_supported": ["api.read", "api.write"],
        "bearer_methods_supported": ["header"]
    }))
}

async fn get_oauth_authorization_server_metadata(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Json<Value> {
    let base = service_base(&headers);
    Json(json!({
        "issuer": base,
        "agent_auth": {
            "skill": "https://workos.com/auth.md",
            "register_uri": format!("{}/agent/auth", base),
            "claim_uri": format!("{}/agent/auth/claim", base),
            "claim_complete_uri": format!("{}/agent/auth/claim/complete", base),
            "identity_types_supported": ["anonymous", "identity_assertion"],
            "anonymous_supported": ["api_key"],
            "identity_assertion_supported": ["verified_email"],
            "credential_types_supported": ["api_key"],
            "events_supported": []
        }
    }))
}

async fn agent_auth(
    State(_state): State<AppState>,
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
                requested_credential_type: requested_credential_type,
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
    State(_state): State<AppState>,
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
    State(_state): State<AppState>,
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
    State(_state): State<AppState>,
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
