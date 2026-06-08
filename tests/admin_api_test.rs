use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use axum::{
    body::{to_bytes, Body},
    extract::DefaultBodyLimit,
    http::{header, Request, StatusCode},
    Router,
};
use conxian_nexus::api::admin::{admin_routes, public_auth_md_routes};
use conxian_nexus::api::rest::AppState;
use conxian_nexus::config::{Config, ENV_ADMIN_API_TOKEN};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::Mutex as AsyncMutex;
use tower::util::ServiceExt;

const RELEASE_APPROVAL_PAYLOAD: &str = r#"{"artifactId":"artifact-1","requestedBy":"actor-1"}"#;

fn test_router() -> Router {
    let mut config_val = Config::from_env().unwrap_or_else(|_| Config::default_test());
    if let Ok(token) = std::env::var(ENV_ADMIN_API_TOKEN) {
        config_val.admin_api_token = Some(token);
    }
    let config = Arc::new(config_val);
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let state = AppState {
        storage,
        nexus_state,
        executor,
        oracle: None,
        tableland,
        kwil: None,
        nostr: None,
        gateway_url: None,
        http_client: reqwest::Client::new(),
        config,
    };

    Router::new()
        .merge(public_auth_md_routes())
        .nest("/admin/v1", admin_routes())
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}

fn admin_api_token_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

struct ScopedEnvVar {
    key: &'static str,
    original: Option<String>,
}

impl ScopedEnvVar {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let original = std::env::var(key).ok();
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }

        Self { key, original }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
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

    let body = to_bytes(prm_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json.get("resource_name").and_then(Value::as_str),
        Some("Conxian Nexus")
    );
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
                .body(Body::from(
                    r#"{"type":"anonymous","requested_credential_type":"api_key"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(registration_response.status(), StatusCode::OK);

    let body = to_bytes(registration_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let claim_token = json
        .get("claim_token")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let claim_view_url = json
        .get("claim_view_url")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let preclaim_credential = json
        .get("credential")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let status_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/v1/status")
                .header(header::HOST, "nexus.test")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", preclaim_credential),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);

    let claim_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agent/auth/claim")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"claim_token":"{}","email":"user@example.com"}}"#,
                    claim_token
                )))
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
    let otp_html = String::from_utf8(
        to_bytes(otp_page.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
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
                .body(Body::from(format!(
                    r#"{{"claim_token":"{}","otp":"{}"}}"#,
                    claim_token, otp
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(complete_response.status(), StatusCode::OK);

    let upgraded_status_response = app
        .oneshot(
            Request::builder()
                .uri("/admin/v1/status")
                .header(header::HOST, "nexus.test")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", preclaim_credential),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upgraded_status_response.status(), StatusCode::OK);

    let body = to_bytes(upgraded_status_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let scopes = json
        .get("scopes")
        .and_then(Value::as_array)
        .expect("status response should include scopes array");
    assert!(
        scopes
            .iter()
            .any(|scope| scope.as_str() == Some("api.write")),
        "post-claim credential should include api.write in status scopes"
    );
}

#[tokio::test]
async fn test_admin_write_fails_closed_when_admin_token_unconfigured() {
    let _env_lock = admin_api_token_lock().lock().await;
    let _token = ScopedEnvVar::set(ENV_ADMIN_API_TOKEN, None);

    let app = test_router();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/v1/releases/request-approval")
                .header(header::HOST, "nexus.test")
                .header(header::AUTHORIZATION, "Bearer any-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(RELEASE_APPROVAL_PAYLOAD))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json.get("error").and_then(Value::as_str),
        Some("admin_api_token_not_configured")
    );
}

#[tokio::test]
async fn test_admin_write_requires_matching_bearer_when_admin_token_configured() {
    let _env_lock = admin_api_token_lock().lock().await;
    let _token = ScopedEnvVar::set(ENV_ADMIN_API_TOKEN, Some("expected-admin-token"));

    let app = test_router();

    let missing_token_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/v1/releases/request-approval")
                .header(header::HOST, "nexus.test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(RELEASE_APPROVAL_PAYLOAD))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_token_response.status(), StatusCode::UNAUTHORIZED);

    let body = to_bytes(missing_token_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json.get("error_description").and_then(Value::as_str),
        Some("Admin API token required")
    );

    let wrong_token_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/v1/releases/request-approval")
                .header(header::HOST, "nexus.test")
                .header(header::AUTHORIZATION, "Bearer wrong-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(RELEASE_APPROVAL_PAYLOAD))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_token_response.status(), StatusCode::UNAUTHORIZED);

    let body = to_bytes(wrong_token_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json.get("error_description").and_then(Value::as_str),
        Some("Invalid admin API token")
    );

    let ok_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/v1/releases/request-approval")
                .header(header::HOST, "nexus.test")
                .header(header::AUTHORIZATION, "Bearer expected-admin-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(RELEASE_APPROVAL_PAYLOAD))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_response.status(), StatusCode::OK);
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

    let body = to_bytes(registration_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let claim_token = json
        .get("claim_token")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let claim_view_url = json
        .get("claim_view_url")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

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
    let otp_html = String::from_utf8(
        to_bytes(otp_page.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
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
                .body(Body::from(format!(
                    r#"{{"claim_token":"{}","otp":"{}"}}"#,
                    claim_token, otp
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(complete_response.status(), StatusCode::OK);

    let body = to_bytes(complete_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
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
    let header_value = response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(header_value.contains("resource_metadata"));
}
