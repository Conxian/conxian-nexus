//! Live-DB conformance suite for [`IdempotencyStore`].
//!
//! Mirrors the enclave SDK's backend-neutral `ReplayStore` conformance suite,
//! adapted to the Nexus consume-once contract and transactional idempotency locks (#251).
//! Every case requires an explicit disposable CONFORMANCE_DATABASE_URL; missing configuration fails.

use chrono::{Duration, Utc};
use conxian_nexus::storage::idempotency::{ConsumeOutcome, IdempotencyError, IdempotencyStore};
#[path = "support/conformance_database.rs"]
mod conformance_database;
use conformance_database::ConformanceDatabase;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Bound concurrent connection usage; every case has an independent schema.
static SUITE_LOCK: Mutex<()> = Mutex::const_new(());

async fn connect(database: &ConformanceDatabase) -> IdempotencyStore {
    IdempotencyStore::new(database.pool().await)
}

/// 1. Single consume-once: accept then duplicate.
#[tokio::test]
async fn conformance_single_consume_once() {
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
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
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
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

/// 3. Reconnect durability; this does not simulate a server restart or failover.
#[tokio::test]
async fn conformance_reconnect_durability() {
    let url = ConformanceDatabase::from_environment();

    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
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
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
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
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
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
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
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
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
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

/// 8. Idempotency lock lifecycle and owner-based release/extension (#251).
#[tokio::test]
async fn conformance_lock_lifecycle() {
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
    let store = connect(&url).await;

    let payload = serde_json::json!({"node_id": "nexus-worker-01", "step": "settlement"});

    // 1. Acquire lock
    assert!(
        store
            .acquire_lock("lock.lifecycle.1", "worker-1", 60, Some(payload.clone()))
            .await
            .unwrap(),
        "first acquisition should succeed"
    );

    // 2. Read lock status
    let lock_info = store
        .get_lock("lock.lifecycle.1")
        .await
        .unwrap()
        .expect("lock record should exist");
    assert_eq!(lock_info.owner, "worker-1");
    assert_eq!(lock_info.payload.as_ref(), Some(&payload));

    // 3. Second worker attempts acquisition while lock is active -> rejected
    assert!(
        !store
            .acquire_lock("lock.lifecycle.1", "worker-2", 60, None)
            .await
            .unwrap(),
        "second worker should be rejected"
    );

    // 4. Owner extends lock
    assert!(
        store
            .extend_lock("lock.lifecycle.1", "worker-1", 120)
            .await
            .unwrap(),
        "owner extension should succeed"
    );

    // 5. Non-owner extension attempt -> rejected
    assert!(
        !store
            .extend_lock("lock.lifecycle.1", "worker-2", 120)
            .await
            .unwrap(),
        "non-owner extension should be rejected"
    );

    // 6. Non-owner release attempt -> rejected
    assert!(
        !store
            .release_lock("lock.lifecycle.1", "worker-2")
            .await
            .unwrap(),
        "non-owner release should be rejected"
    );

    // 7. Owner release
    assert!(
        store
            .release_lock("lock.lifecycle.1", "worker-1")
            .await
            .unwrap(),
        "owner release should succeed"
    );

    // 8. Re-acquisition by worker-2 after release
    assert!(
        store
            .acquire_lock("lock.lifecycle.1", "worker-2", 60, None)
            .await
            .unwrap(),
        "re-acquisition after release should succeed"
    );
}

/// 9. Lock contention: 32 concurrent workers competing for the same lock key (#251).
#[tokio::test(flavor = "multi_thread")]
async fn conformance_lock_contention() {
    let url = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    url.prepare().await;
    let store = Arc::new(connect(&url).await);

    let mut handles = Vec::new();
    for i in 0..32 {
        let store = store.clone();
        let owner = format!("worker-{i}");
        handles.push(tokio::spawn(async move {
            store
                .acquire_lock("lock.contention.1", &owner, 30, None)
                .await
                .unwrap()
        }));
    }

    let mut acquired_count = 0usize;
    for handle in handles {
        if handle.await.unwrap() {
            acquired_count += 1;
        }
    }
    assert_eq!(
        acquired_count, 1,
        "exactly one worker must acquire the active lock"
    );
}

#[tokio::test]
async fn conformance_private_schemas_do_not_reset_other_test_records() {
    let first = ConformanceDatabase::from_environment();
    let second = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    first.prepare().await;
    let first_store = connect(&first).await;
    assert_eq!(
        first_store
            .consume_once("isolation.key", "op")
            .await
            .unwrap(),
        ConsumeOutcome::Fresh
    );
    second.prepare().await;
    let second_store = connect(&second).await;
    assert_eq!(
        second_store
            .consume_once("isolation.key", "op")
            .await
            .unwrap(),
        ConsumeOutcome::Fresh
    );
    assert_eq!(
        first_store
            .consume_once("isolation.key", "op")
            .await
            .unwrap(),
        ConsumeOutcome::AlreadyConsumed
    );
}
