//! Real disposable PostgreSQL only; no production URL or simulated store.
#[path = "support/conformance_database.rs"]
mod conformance_database;
use conformance_database::ConformanceDatabase;
use conxian_nexus::storage::admin_workflows::{
    Command, Decision, Operation, WorkflowError, WorkflowStore,
};
use sqlx::PgPool;

async fn fixture() -> (ConformanceDatabase, PgPool, WorkflowStore) {
    let db = ConformanceDatabase::from_environment();
    db.prepare().await;
    let pool = db.pool().await;
    sqlx::raw_sql(include_str!(
        "../migrations/20261024000000_admin_workflows.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let store = WorkflowStore::new(pool.clone());
    (db, pool, store)
}

fn command(operation: Operation, decision: Option<Decision>) -> Command {
    Command {
        operation,
        decision,
        entity_id: "artifact-1".into(),
        actor: "operator-1".into(),
        notes: Some("test-only fixture note".into()),
    }
}

#[tokio::test]
async fn workflow_persists_receipt_state_notes_and_audit_across_pool_recreation() {
    let (db, pool, store) = fixture().await;
    let request = command(Operation::RequestRelease, None);
    let first = store.apply("request-1", &request).await.unwrap();
    assert!(first.request_id.is_some());
    assert!(first.decision_id.is_none());
    pool.close().await;
    let next_pool = db.pool().await;
    let recovered = WorkflowStore::new(next_pool.clone());
    assert_eq!(first, recovered.apply("request-1", &request).await.unwrap());
    let decision = recovered
        .apply(
            "decision-1",
            &command(Operation::DecideRelease, Some(Decision::Approve)),
        )
        .await
        .unwrap();
    assert!(decision.decision_id.is_some());
    assert!(decision.request_id.is_none());
    assert_eq!(
        recovered.entities("release", 10, 0).await.unwrap()[0].status,
        "approved"
    );
    let events = recovered.audit(10, 0).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().any(|event| event.id == first.audit_event_id));
    assert!(events
        .iter()
        .any(|event| event.id == decision.audit_event_id));
    let note: String = sqlx::query_scalar(
        "SELECT notes FROM admin_workflow_events WHERE command_id = 'request-1'",
    )
    .fetch_one(&next_pool)
    .await
    .unwrap();
    assert_eq!(note, "test-only fixture note");
}

#[tokio::test]
async fn concurrent_retries_commit_exactly_one_event_and_receipt() {
    let (_, _, store) = fixture().await;
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let store = store.clone();
        tasks.push(tokio::spawn(async move {
            store
                .apply("shared-key", &command(Operation::RequestRelease, None))
                .await
                .unwrap()
        }));
    }
    let expected = tasks.remove(0).await.unwrap();
    for task in tasks {
        assert_eq!(expected, task.await.unwrap());
    }
    assert_eq!(store.audit(100, 0).await.unwrap().len(), 1);
}

#[tokio::test]
async fn reusing_a_key_for_different_input_rejects_without_new_effects() {
    let (_, _, store) = fixture().await;
    let first = command(Operation::RequestRelease, None);
    store.apply("key", &first).await.unwrap();
    let mut altered = first;
    altered.actor = "different-actor".into();
    assert!(matches!(
        store.apply("key", &altered).await,
        Err(WorkflowError::CommandConflict)
    ));
    assert_eq!(store.audit(100, 0).await.unwrap().len(), 1);
}

#[tokio::test]
async fn incompatible_concurrent_decisions_have_one_winner() {
    let (_, _, store) = fixture().await;
    store
        .apply("request", &command(Operation::RequestRelease, None))
        .await
        .unwrap();
    let approve = command(Operation::DecideRelease, Some(Decision::Approve));
    let reject = command(Operation::DecideRelease, Some(Decision::Reject));
    let (a, b) = tokio::join!(
        store.apply("approve", &approve),
        store.apply("reject", &reject)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let error = match (a, b) {
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => error,
        _ => panic!("one conflicting decision must fail"),
    };
    assert!(matches!(error, WorkflowError::InvalidTransition));
    assert_eq!(store.audit(100, 0).await.unwrap().len(), 2);
}

#[tokio::test]
async fn failed_audit_insert_rolls_back_entity_write() {
    let (_, pool, store) = fixture().await;
    // Inject an actual database failure after the entity statement.
    sqlx::query("ALTER TABLE admin_workflow_events ADD CONSTRAINT reject_fixture_actor CHECK(actor <> 'operator-1')")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        store
            .apply("request", &command(Operation::RequestRelease, None))
            .await,
        Err(WorkflowError::Backend(_))
    ));
    assert!(store.entities("release", 100, 0).await.unwrap().is_empty());
    assert!(store.audit(100, 0).await.unwrap().is_empty());
    sqlx::query("ALTER TABLE admin_workflow_events DROP CONSTRAINT reject_fixture_actor")
        .execute(&pool)
        .await
        .unwrap();
    store
        .apply("request", &command(Operation::RequestRelease, None))
        .await
        .unwrap();
    assert_eq!(store.audit(100, 0).await.unwrap().len(), 1);
}

#[tokio::test]
async fn missing_entities_and_invalid_transitions_never_create_receipts() {
    let (_, _, store) = fixture().await;
    assert!(matches!(
        store
            .apply(
                "missing",
                &command(Operation::DecideRelease, Some(Decision::Approve))
            )
            .await,
        Err(WorkflowError::NotFound)
    ));
    assert!(store.audit(100, 0).await.unwrap().is_empty());
    store
        .apply("request", &command(Operation::RequestRelease, None))
        .await
        .unwrap();
    assert!(matches!(
        store
            .apply(
                "duplicate-proposal",
                &command(Operation::RequestRelease, None)
            )
            .await,
        Err(WorkflowError::InvalidTransition)
    ));
    assert_eq!(store.audit(100, 0).await.unwrap().len(), 1);
}

#[tokio::test]
async fn governance_requires_registered_pending_action_and_uses_explicit_lifecycle() {
    let (_, _, store) = fixture().await;
    let decision = command(Operation::DecideGovernance, Some(Decision::Approve));
    assert!(matches!(
        store.apply("missing-action", &decision).await,
        Err(WorkflowError::NotFound)
    ));
    // Trusted owner-side intake; no public proposal/registration endpoint added.
    store
        .apply("proposal", &command(Operation::ProposeGovernance, None))
        .await
        .unwrap();
    store
        .apply(
            "changes",
            &command(Operation::DecideGovernance, Some(Decision::RequestChanges)),
        )
        .await
        .unwrap();
    store
        .apply("reproposal", &command(Operation::ProposeGovernance, None))
        .await
        .unwrap();
    store.apply("approve", &decision).await.unwrap();
    assert_eq!(
        store.entities("governance", 100, 0).await.unwrap()[0].status,
        "approved"
    );
    assert_eq!(store.audit(100, 0).await.unwrap().len(), 4);
}

#[tokio::test]
async fn missing_schema_is_unavailable_not_an_empty_success() {
    let db = ConformanceDatabase::from_environment();
    db.prepare().await;
    let store = WorkflowStore::new(db.pool().await);
    assert!(matches!(
        store.entities("release", 100, 0).await,
        Err(WorkflowError::Backend(_))
    ));
    assert!(matches!(
        store.audit(100, 0).await,
        Err(WorkflowError::Backend(_))
    ));
}

#[tokio::test]
async fn paginated_reads_are_bounded_and_validation_is_explicit() {
    let (_, _, store) = fixture().await;
    for i in 0..5 {
        let mut c = command(Operation::RequestRelease, None);
        c.entity_id = format!("artifact-{i}");
        store.apply(&format!("request-{i}"), &c).await.unwrap();
    }
    let first = store.entities("release", 2, 0).await.unwrap();
    let second = store.entities("release", 2, 2).await.unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);
    assert!(!second
        .iter()
        .any(|row| first.iter().any(|previous| previous.id == row.id)));
    assert!(matches!(
        store.entities("release", 0, 0).await,
        Err(WorkflowError::InvalidInput)
    ));
    assert!(matches!(
        store.audit(1001, 0).await,
        Err(WorkflowError::InvalidInput)
    ));
}

#[tokio::test]
async fn authenticated_http_workflow_commits_then_reads_back_canonical_receipts() {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        Router,
    };
    use conxian_nexus::{
        api::{admin::admin_routes, rest::AppState},
        config::Config,
        executor::{rgb::RGBRolloutMode, NexusExecutor},
        state::NexusState,
        storage::{tableland::TablelandAdapter, Storage},
    };
    use std::{collections::HashSet, sync::Arc};
    use tower::util::ServiceExt;
    let (_, pool, store) = fixture().await;
    store
        .apply(
            "owner-proposal",
            &command(Operation::ProposeGovernance, None),
        )
        .await
        .unwrap();
    let mut config = Config::default_test();
    config.admin_api_token = Some("workflow-conformance-token".into());
    let config = Arc::new(config);
    let storage = Arc::new(Storage {
        pg_pool: pool,
        redis_client: redis::Client::open("redis://127.0.0.1:1/").unwrap(),
    });
    let state = AppState {
        storage: storage.clone(),
        nexus_state: Arc::new(NexusState::new()),
        executor: Arc::new(NexusExecutor::new(
            storage.clone(),
            RGBRolloutMode::Disabled,
            HashSet::new(),
        )),
        oracle: None,
        tableland: Arc::new(TablelandAdapter::new(
            storage,
            config.tableland_base_url.clone(),
        )),
        kwil: None,
        nostr: None,
        gateway_url: None,
        http_client: reqwest::Client::new(),
        config,
    };
    let app = Router::new()
        .nest("/admin/v1", admin_routes(state.clone()))
        .with_state(state);
    async fn call(
        app: &Router,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        key: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", "Bearer workflow-conformance-token")
            .header("content-type", "application/json");
        if let Some(key) = key {
            request = request.header("idempotency-key", key);
        }
        let body = body.map_or(Body::empty(), |value| Body::from(value.to_string()));
        let response = app
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }
    let payload = serde_json::json!({"artifactId":"http-artifact", "requestedBy":"fixture-operator", "signatures":["fixture-signature-1", "fixture-signature-2"]});
    let (status, receipt) = call(
        &app,
        "POST",
        "/admin/v1/releases/request-approval",
        Some(payload.clone()),
        Some("http-request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["accepted"], true);
    assert!(receipt["requestId"].is_string());
    let (_, retried) = call(
        &app,
        "POST",
        "/admin/v1/releases/request-approval",
        Some(payload),
        Some("http-request"),
    )
    .await;
    assert_eq!(receipt, retried);
    let (status, releases) = call(&app, "GET", "/admin/v1/releases", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(releases["releases"][0]["name"], "http-artifact");
    assert_eq!(releases["releases"][0]["status"], "in_review");
    let decision = serde_json::json!({"artifactId":"http-artifact", "actorId":"fixture-operator", "decision":"approve", "signatures":["fixture-signature-1", "fixture-signature-2"]});
    let (status, decided) = call(
        &app,
        "POST",
        "/admin/v1/releases/decision",
        Some(decision),
        Some("http-decision"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(decided["decisionId"].is_string());
    assert!(decided.get("requestId").is_none());
    let (_, events) = call(&app, "GET", "/admin/v1/audit-events", None, None).await;
    let events = events["events"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert!(events
        .iter()
        .any(|event| event["id"] == receipt["auditEventId"]));
    assert!(events
        .iter()
        .any(|event| event["id"] == decided["auditEventId"]));
    let (status, actions) = call(&app, "GET", "/admin/v1/governance-actions", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(actions["governanceActions"][0]["title"], "artifact-1");
    assert_eq!(actions["governanceActions"][0]["status"], "pending");
}
