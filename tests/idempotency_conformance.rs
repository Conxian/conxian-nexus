//! Live-DB conformance suite for [`IdempotencyStore`].
//!
//! Mirrors the enclave SDK's backend-neutral `ReplayStore` conformance suite,
//! adapted to the Nexus consume-once contract. Every case requires a live
//! PostgreSQL database and is skipped when `DATABASE_URL` is unset.

use chrono::{Duration, Utc};
use conxian_nexus::storage::idempotency::{ConsumeOutcome, IdempotencyError, IdempotencyStore};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Serializes the suite so each case's `TRUNCATE`-based reset cannot race a
/// concurrently-running case against the same tables.
static SUITE_LOCK: Mutex<()> = Mutex::const_new(());

fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn connect(url: &str) -> IdempotencyStore {
    let pool = PgPoolOptions::new()
        .max_connections(64)
        .connect(url)
        .await
        .expect("failed to connect to DATABASE_URL");
    IdempotencyStore::new(pool)
}

/// Clear both tables so each case runs against an isolated, re-runnable state.
async fn reset(url: &str) {
    let pool = PgPoolOptions::new()
        .connect(url)
        .await
        .expect("failed to connect to DATABASE_URL");
    sqlx::query("TRUNCATE idempotency_records, idempotency_high_water")
        .execute(&pool)
        .await
        .expect("failed to reset conformance tables");
    pool.close().await;
}

/// 1. Single consume-once: accept then duplicate.
#[tokio::test]
async fn conformance_single_consume_once() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SUITE_LOCK.lock().await;
    reset(&url).await;
    let store = connect(&url).await;

    assert_eq!(
        store
            .consume_once("conformance.single.1", "op")
            .await
            .unwrap(),
        ConsumeOutcome::Fresh
    );
    assert_eq!(
        store
            .consume_once("conformance.single.1", "op")
            .await
            .unwrap(),
        ConsumeOutcome::AlreadyConsumed
    );
}

/// 2. All-or-nothing batch: partial conflict rolls back, retry succeeds atomically.
#[tokio::test]
async fn conformance_batch_all_or_nothing() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SUITE_LOCK.lock().await;
    reset(&url).await;
    let store = connect(&url).await;

    let first = store
        .consume_once_batch(&[
            ("conformance.batch.a".to_string(), "op".to_string()),
            ("conformance.batch.b".to_string(), "op".to_string()),
        ])
        .await
        .unwrap();
    assert_eq!(first, vec![ConsumeOutcome::Fresh, ConsumeOutcome::Fresh]);

    // "a" is already consumed, so the whole batch must roll back; "c" is not persisted.
    let conflict = store
        .consume_once_batch(&[
            ("conformance.batch.c".to_string(), "op".to_string()),
            ("conformance.batch.a".to_string(), "op".to_string()),
        ])
        .await
        .unwrap();
    assert_eq!(
        conflict,
        vec![
            ConsumeOutcome::AlreadyConsumed,
            ConsumeOutcome::AlreadyConsumed
        ]
    );

    // Retrying the failed batch succeeds atomically. A Fresh result for "c" proves
    // the rolled-back batch never persisted it (otherwise this retry would conflict).
    let retry = store
        .consume_once_batch(&[
            ("conformance.batch.c".to_string(), "op".to_string()),
            ("conformance.batch.d".to_string(), "op".to_string()),
        ])
        .await
        .unwrap();
    assert_eq!(retry, vec![ConsumeOutcome::Fresh, ConsumeOutcome::Fresh]);
}

/// 3. Restart durability: consumption survives a reconnect.
#[tokio::test]
async fn conformance_restart_durability() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };

    let _guard = SUITE_LOCK.lock().await;
    reset(&url).await;
    {
        let store = connect(&url).await;
        assert_eq!(
            store
                .consume_once("conformance.restart.1", "op")
                .await
                .unwrap(),
            ConsumeOutcome::Fresh
        );
    }

    // Reconnect with a fresh pool.
    let store = connect(&url).await;
    assert_eq!(
        store
            .consume_once("conformance.restart.1", "op")
            .await
            .unwrap(),
        ConsumeOutcome::AlreadyConsumed
    );
}

/// 4. Anti-rollback high-water clock.
#[tokio::test]
async fn conformance_high_water_clock() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SUITE_LOCK.lock().await;
    reset(&url).await;
    let store = connect(&url).await;

    store
        .observe_time("conformance.clock.1", 1000)
        .await
        .unwrap();
    store
        .observe_time("conformance.clock.1", 1001)
        .await
        .unwrap();
    store
        .observe_time("conformance.clock.1", 1001)
        .await
        .unwrap(); // equal is allowed
    assert!(matches!(
        store.observe_time("conformance.clock.1", 1000).await,
        Err(IdempotencyError::ClockRollback)
    ));
}

/// 5. Expiry/retention: expired records are reclaimed and re-consumable.
#[tokio::test]
async fn conformance_expiry_retention() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SUITE_LOCK.lock().await;
    reset(&url).await;
    let store = connect(&url).await;
    let now = Utc::now();
    let retain_until = now + Duration::seconds(1);

    assert_eq!(
        store
            .consume_once_until("conformance.expiry.1", "op", retain_until)
            .await
            .unwrap(),
        ConsumeOutcome::Fresh
    );
    assert_eq!(
        store
            .consume_once("conformance.expiry.1", "op")
            .await
            .unwrap(),
        ConsumeOutcome::AlreadyConsumed
    );

    let reclaimed = store
        .purge_expired(now + Duration::seconds(2))
        .await
        .unwrap();
    assert!(reclaimed >= 1, "expired record should be reclaimed");

    assert_eq!(
        store
            .consume_once("conformance.expiry.1", "op")
            .await
            .unwrap(),
        ConsumeOutcome::Fresh,
        "reclaimed key must be re-consumable"
    );
}

/// 6. Validation precedes time observation (invalid input never advances state).
#[tokio::test]
async fn conformance_validation_before_time_observation() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SUITE_LOCK.lock().await;
    reset(&url).await;
    let store = connect(&url).await;

    assert!(matches!(
        store.observe_time("", 1000).await,
        Err(IdempotencyError::InvalidClock)
    ));
    assert!(matches!(
        store.consume_once("", "op").await,
        Err(IdempotencyError::InvalidKey)
    ));
    assert!(matches!(
        store.consume_once("key", "").await,
        Err(IdempotencyError::InvalidOperation)
    ));
}

/// 7. Contention: 32 concurrent consumers of the same key produce exactly one Fresh.
#[tokio::test(flavor = "multi_thread")]
async fn conformance_contention() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SUITE_LOCK.lock().await;
    reset(&url).await;
    let store = Arc::new(connect(&url).await);

    let mut handles = Vec::new();
    for _ in 0..32 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store
                .consume_once("conformance.contention.1", "op")
                .await
                .unwrap()
        }));
    }

    let mut fresh = 0usize;
    for handle in handles {
        if handle.await.unwrap() == ConsumeOutcome::Fresh {
            fresh += 1;
        }
    }
    assert_eq!(fresh, 1, "exactly one consumer must observe Fresh");

    // Overlapping batches: exactly one of two overlapping batches commits atomically.
    let store2 = Arc::new(connect(&url).await);
    let batch_a = store2.clone();
    let batch_b = store2.clone();
    let (ra, rb) = tokio::join!(
        tokio::spawn(async move {
            batch_a
                .consume_once_batch(&[
                    ("conformance.contention.x".to_string(), "op".to_string()),
                    ("conformance.contention.y".to_string(), "op".to_string()),
                ])
                .await
                .unwrap()
        }),
        tokio::spawn(async move {
            batch_b
                .consume_once_batch(&[
                    ("conformance.contention.y".to_string(), "op".to_string()),
                    ("conformance.contention.z".to_string(), "op".to_string()),
                ])
                .await
                .unwrap()
        }),
    );
    let ra = ra.unwrap();
    let rb = rb.unwrap();
    let committed_a = ra.iter().all(|o| *o == ConsumeOutcome::Fresh);
    let committed_b = rb.iter().all(|o| *o == ConsumeOutcome::Fresh);
    assert_ne!(
        committed_a, committed_b,
        "exactly one overlapping batch commits"
    );
}
