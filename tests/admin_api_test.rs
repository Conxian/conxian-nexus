use conxian_nexus::api::rest::AppState;
use conxian_nexus::config::Config;
use conxian_nexus::state::NexusState;
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use std::sync::Arc;
use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use conxian_nexus::api::admin::{admin_routes, public_auth_md_routes};
use conxian_nexus::config::ENV_ADMIN_API_TOKEN;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::OnceLock;
use tokio::sync::Mutex as AsyncMutex;
use tower::util::ServiceExt;

const RELEASE_APPROVAL_PAYLOAD: &str = r#"{"artifactId":"artifact-1","requestedBy":"actor-1"}"#;
const RELEASE_DECISION_PAYLOAD: &str =
    r#"{"artifactId":"artifact-1","decision":"approve","actorId":"actor-1"}"#;
const GOVERNANCE_DECISION_PAYLOAD: &str =
    r#"{"actionId":"action-1","decision":"approve","actorId":"actor-1"}"#;
const SAFETY_MODE_ACK_PAYLOAD: &str = r#"{"ackBy":"operator-1","reason":"acknowledged"}"#;

#[derive(Clone)]
struct CanonicalEndpoint {
    method: Method,
    request_path: &'static str,
    contract_path: &'static str,
    body: Option<&'static str>,
}

fn canonical_admin_v1_endpoints() -> Vec<CanonicalEndpoint> {
    vec![
        CanonicalEndpoint {
            method: Method::POST,
            request_path: "/admin/v1/releases/request-approval",
            contract_path: "/admin/v1/releases/request-approval",
            body: Some(RELEASE_APPROVAL_PAYLOAD),
        },
        CanonicalEndpoint {
            method: Method::POST,
            request_path: "/admin/v1/releases/decision",
            contract_path: "/admin/v1/releases/decision",
            body: Some(RELEASE_DECISION_PAYLOAD),
        },
        CanonicalEndpoint {
            method: Method::POST,
            request_path: "/admin/v1/governance/decision",
            contract_path: "/admin/v1/governance/decision",
            body: Some(GOVERNANCE_DECISION_PAYLOAD),
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/runtime/health",
            contract_path: "/admin/v1/runtime/health",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/runtime/readiness",
            contract_path: "/admin/v1/runtime/readiness",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/audit-events",
            contract_path: "/admin/v1/audit-events",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/chains",
            contract_path: "/admin/v1/chains",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/chains/bitcoin%2Fmainnet/status",
            contract_path: "/admin/v1/chains/{chain}/status",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/attestations",
            contract_path: "/admin/v1/attestations",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/attestations/attestation-1",
            contract_path: "/admin/v1/attestations/{id}",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/drift",
            contract_path: "/admin/v1/drift",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/safety-mode",
            contract_path: "/admin/v1/safety-mode",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::POST,
            request_path: "/admin/v1/safety-mode/ack",
            contract_path: "/admin/v1/safety-mode/ack",
            body: Some(SAFETY_MODE_ACK_PAYLOAD),
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/promotion-evidence/release%2F2026.06",
            contract_path: "/admin/v1/promotion-evidence/{release}",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/environments",
            contract_path: "/admin/v1/environments",
            body: None,
        },
        CanonicalEndpoint {
            method: Method::GET,
            request_path: "/admin/v1/environments/production",
            contract_path: "/admin/v1/environments/{env}",
            body: None,
        },
    ]
}

fn parse_openapi_admin_v1_methods() -> BTreeSet<String> {
    let openapi = std::fs::read_to_string("docs/openapi.yaml")
        .expect("docs/openapi.yaml should be readable from repo root");
    let mut in_paths = false;
    let mut current_path: Option<String> = None;
    let mut methods = BTreeSet::new();

    for raw_line in openapi.lines() {
        if raw_line.trim() == "paths:" {
            in_paths = true;
            continue;
        }

        if !in_paths {
            continue;
        }

        if !raw_line.starts_with("  ") {
            break;
        }

        if raw_line.starts_with("  /") && raw_line.trim_end().ends_with(':') {
            current_path = Some(raw_line.trim().trim_end_matches(':').to_string());
            continue;
        }

        let Some(path) = current_path.as_ref() else {
            continue;
        };

        if !path.starts_with("/admin/v1") {
            continue;
        }

        let method_name = raw_line.trim().strip_suffix(':').filter(|name| {
            matches!(
                *name,
                "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
            )
        });

        if let Some(method_name) = method_name {
            methods.insert(format!("{} {}", method_name.to_uppercase(), path));
        }
    }

    methods
}
fn test_router() -> Router {
    let config = Arc::new(Config::from_env().unwrap_or_else(|_| Config::default_test()));
    let storage = Arc::new(Storage::new_lazy("postgres://localhost/nexus", "redis://127.0.0.1/").unwrap());
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        std::collections::HashSet::new(),
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
        config: config.clone(),
    };

    Router::new()
        .merge(public_auth_md_routes(state.clone()))
        .nest("/admin/v1", admin_routes(state.clone()))
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

#[tokio::test]
async fn test_canonical_admin_v1_routes_are_reachable_with_expected_methods() {
    let _env_lock = admin_api_token_lock().lock().await;
    let _token = ScopedEnvVar::set(ENV_ADMIN_API_TOKEN, Some("expected-admin-token"));
    let app = test_router();

    for endpoint in canonical_admin_v1_endpoints() {
        let mut builder = Request::builder()
            .method(endpoint.method.clone())
            .uri(endpoint.request_path)
            .header(header::HOST, "nexus.test")
            .header(header::AUTHORIZATION, "Bearer expected-admin-token");

        let request = if let Some(payload) = endpoint.body {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            builder.body(Body::from(payload)).unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        };

        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();

        assert_ne!(
            status,
            StatusCode::NOT_FOUND,
            "{} {} should not 404",
            endpoint.method,
            endpoint.request_path
        );
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{} {} should accept canonical method",
            endpoint.method,
            endpoint.request_path
        );
    }
}

#[test]
fn test_openapi_admin_v1_surface_matches_router_contract() {
    let expected: BTreeSet<String> = canonical_admin_v1_endpoints()
        .into_iter()
        .map(|endpoint| format!("{} {}", endpoint.method, endpoint.contract_path))
        .chain(std::iter::once("GET /admin/v1/status".to_string()))
        .collect();

    let openapi_paths = parse_openapi_admin_v1_methods();
    assert_eq!(openapi_paths, expected);
}
