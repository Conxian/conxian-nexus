use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use conxian_nexus::api::admin::admin_routes;
use serde_json::Value;
use tower::util::ServiceExt;

#[tokio::test]
async fn test_admin_release_request_requires_token_when_configured() {
    std::env::set_var(conxian_nexus::config::ENV_ADMIN_API_TOKEN, "test-admin-token");

    let app = admin_routes();
    let request = Request::builder()
        .method("POST")
        .uri("/releases/request-approval")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"artifactId":"artifact-1","requestedBy":"actor-1","notes":"test"}"#,
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let authorized_request = Request::builder()
        .method("POST")
        .uri("/releases/request-approval")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer test-admin-token")
        .body(Body::from(
            r#"{"artifactId":"artifact-1","requestedBy":"actor-1","notes":"test"}"#,
        ))
        .unwrap();

    let response = app.oneshot(authorized_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.get("accepted").and_then(Value::as_bool), Some(true));
    assert!(json.get("requestId").and_then(Value::as_str).is_some());
    assert!(json.get("auditEventId").and_then(Value::as_str).is_some());

    std::env::remove_var(conxian_nexus::config::ENV_ADMIN_API_TOKEN);
}

#[tokio::test]
async fn test_admin_governance_decision_allows_request_without_token_configuration() {
    std::env::remove_var(conxian_nexus::config::ENV_ADMIN_API_TOKEN);

    let app = admin_routes();
    let request = Request::builder()
        .method("POST")
        .uri("/governance/decision")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"actionId":"gov-1","decision":"approve","actorId":"actor-1","notes":"ok"}"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.get("accepted").and_then(Value::as_bool), Some(true));
    assert!(json.get("decisionId").and_then(Value::as_str).is_some());
    assert!(json.get("auditEventId").and_then(Value::as_str).is_some());
}
