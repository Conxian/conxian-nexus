use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    Router,
};
use conxian_nexus::api::admin::{admin_routes, public_auth_md_routes};
use serde_json::Value;
use tower::util::ServiceExt;

fn test_router() -> Router {
    Router::new()
        .merge(public_auth_md_routes())
        .nest("/admin/v1", admin_routes())
}

#[tokio::test]
async fn test_auth_md_and_metadata_endpoints_exist() {
    let app = test_router();

    let auth_md_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth.md")
                .header(header::HOST, "nexus.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auth_md_response.status(), StatusCode::OK);

    let prm_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-protected-resource")
                .header(header::HOST, "nexus.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(prm_response.status(), StatusCode::OK);

    let body = to_bytes(prm_response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.get("resource_name").and_then(Value::as_str), Some("Conxian Nexus"));
}

#[tokio::test]
async fn test_anonymous_registration_claim_and_protected_access() {
    let app = test_router();

    let registration_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agent/auth")
                .header(header::HOST, "nexus.test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"type":"anonymous","requested_credential_type":"api_key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(registration_response.status(), StatusCode::OK);

    let body = to_bytes(registration_response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let claim_token = json.get("claim_token").and_then(Value::as_str).unwrap().to_string();
    let claim_view_url = json.get("claim_view_url").and_then(Value::as_str).unwrap().to_string();
    let preclaim_credential = json.get("credential").and_then(Value::as_str).unwrap().to_string();

    let status_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/v1/status")
                .header(header::HOST, "nexus.test")
                .header(header::AUTHORIZATION, format!("Bearer {}", preclaim_credential))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);

    let write_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/v1/releases/request-approval")
                .header(header::HOST, "nexus.test")
                .header(header::AUTHORIZATION, format!("Bearer {}", preclaim_credential))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"artifactId":"artifact-1","requestedBy":"actor-1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(write_response.status(), StatusCode::FORBIDDEN);

    let claim_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agent/auth/claim")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"claim_token":"{}","email":"user@example.com"}}"#, claim_token)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(claim_response.status(), StatusCode::OK);

    let otp_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(claim_view_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(otp_page.status(), StatusCode::OK);
    let otp_html = String::from_utf8(to_bytes(otp_page.into_body(), 1024 * 1024).await.unwrap().to_vec()).unwrap();
    let otp = otp_html
        .split("OTP: <strong>")
        .nth(1)
        .unwrap()
        .split("</strong>")
        .next()
        .unwrap()
        .to_string();

    let complete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agent/auth/claim/complete")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"claim_token":"{}","otp":"{}"}}"#, claim_token, otp)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(complete_response.status(), StatusCode::OK);

    let upgraded_write_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/v1/releases/request-approval")
                .header(header::HOST, "nexus.test")
                .header(header::AUTHORIZATION, format!("Bearer {}", preclaim_credential))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"artifactId":"artifact-1","requestedBy":"actor-1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upgraded_write_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_verified_email_registration_issues_credential_after_claim() {
    let app = test_router();

    let registration_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agent/auth")
                .header(header::HOST, "nexus.test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"type":"identity_assertion","assertion_type":"verified_email","assertion":"user@example.com","requested_credential_type":"api_key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(registration_response.status(), StatusCode::OK);

    let body = to_bytes(registration_response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let claim_token = json.get("claim_token").and_then(Value::as_str).unwrap().to_string();
    let claim_view_url = json.get("claim_view_url").and_then(Value::as_str).unwrap().to_string();

    let otp_page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(claim_view_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let otp_html = String::from_utf8(to_bytes(otp_page.into_body(), 1024 * 1024).await.unwrap().to_vec()).unwrap();
    let otp = otp_html
        .split("OTP: <strong>")
        .nth(1)
        .unwrap()
        .split("</strong>")
        .next()
        .unwrap()
        .to_string();

    let complete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agent/auth/claim/complete")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"claim_token":"{}","otp":"{}"}}"#, claim_token, otp)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(complete_response.status(), StatusCode::OK);

    let body = to_bytes(complete_response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let credential = json.get("credential").and_then(Value::as_str).unwrap();

    let status_response = app
        .oneshot(
            Request::builder()
                .uri("/admin/v1/status")
                .header(header::HOST, "nexus.test")
                .header(header::AUTHORIZATION, format!("Bearer {}", credential))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_unauthorized_status_includes_www_authenticate_metadata() {
    let app = test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/v1/status")
                .header(header::HOST, "nexus.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let header_value = response.headers().get(header::WWW_AUTHENTICATE).unwrap().to_str().unwrap();
    assert!(header_value.contains("resource_metadata"));
}
