use axum::{routing::post, Json, Router};
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
pub struct WorkflowMutationResponse {
    pub accepted: bool,
    pub message: String,
    #[serde(rename = "auditEventId")]
    pub audit_event_id: String,
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

async fn request_release_approval(
    Json(payload): Json<ReleaseApprovalRequest>,
) -> Json<ReleaseApprovalResponse> {
    Json(ReleaseApprovalResponse {
        accepted: true,
        request_id: format!("req_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Accepted bootstrap release approval request for artifact {} from {}.",
            payload.artifact_id, payload.requested_by
        ),
    })
}

async fn submit_release_decision(
    Json(payload): Json<ReleaseDecisionRequest>,
) -> Json<WorkflowDecisionResponse> {
    Json(WorkflowDecisionResponse {
        accepted: true,
        decision_id: format!("release_decision_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Accepted bootstrap release decision '{}' for artifact {} from actor {}.",
            payload.decision, payload.artifact_id, payload.actor_id
        ),
    })
}

async fn submit_governance_decision(
    Json(payload): Json<GovernanceDecisionRequest>,
) -> Json<WorkflowDecisionResponse> {
    Json(WorkflowDecisionResponse {
        accepted: true,
        decision_id: format!("governance_decision_{}", Uuid::new_v4()),
        audit_event_id: format!("audit_{}", Uuid::new_v4()),
        message: format!(
            "Accepted bootstrap governance decision '{}' for action {} from actor {}.",
            payload.decision, payload.action_id, payload.actor_id
        ),
    })
}
