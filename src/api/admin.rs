use axum::{
    extract::Json,
    http::{header, HeaderMap, StatusCode},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

pub fn admin_routes() -> Router {
    Router::new()
        .route("/releases/request-approval", post(request_release_approval))
        .route("/releases/decision", post(submit_release_decision))
        .route("/governance/decision", post(submit_governance_decision))
}

fn configured_admin_token() -> Option<String> {
    std::env::var(crate::config::ENV_ADMIN_API_TOKEN)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn authorize_admin_request(headers: &HeaderMap) -> Result<(), StatusCode> {
    let Some(expected_token) = configured_admin_token() else {
        return Ok(());
    };

    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == expected_token => Ok(()),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn request_release_approval(
    headers: HeaderMap,
    Json(payload): Json<ReleaseApprovalRequest>,
) -> Result<Json<ReleaseApprovalResponse>, StatusCode> {
    authorize_admin_request(&headers)?;

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
    headers: HeaderMap,
    Json(payload): Json<ReleaseDecisionRequest>,
) -> Result<Json<WorkflowDecisionResponse>, StatusCode> {
    authorize_admin_request(&headers)?;

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
    headers: HeaderMap,
    Json(payload): Json<GovernanceDecisionRequest>,
) -> Result<Json<WorkflowDecisionResponse>, StatusCode> {
    authorize_admin_request(&headers)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorize_admin_request_without_configured_token_allows_request() {
        std::env::remove_var(crate::config::ENV_ADMIN_API_TOKEN);
        let headers = HeaderMap::new();
        assert!(authorize_admin_request(&headers).is_ok());
    }

    #[test]
    fn test_authorize_admin_request_with_token_requires_bearer_match() {
        std::env::set_var(crate::config::ENV_ADMIN_API_TOKEN, "test-token");

        let mut headers = HeaderMap::new();
        assert_eq!(authorize_admin_request(&headers), Err(StatusCode::UNAUTHORIZED));

        headers.insert(header::AUTHORIZATION, "Bearer wrong-token".parse().unwrap());
        assert_eq!(authorize_admin_request(&headers), Err(StatusCode::UNAUTHORIZED));

        headers.insert(header::AUTHORIZATION, "Bearer test-token".parse().unwrap());
        assert!(authorize_admin_request(&headers).is_ok());

        std::env::remove_var(crate::config::ENV_ADMIN_API_TOKEN);
    }
}
