//! Comprehensive test suite for SRL-1 Lightning Network recovery layer.

use chrono::{Duration as ChronoDuration, Utc};
use conxian_nexus::executor::lightning::{
    LightningFailureType, LightningPaymentStatus, LightningResilienceAdapter, PaymentIntent,
};

#[test]
fn test_exhaustive_5x5_state_transition_matrix() {
    let adapter = LightningResilienceAdapter::new();
    use LightningPaymentStatus::*;

    let all_statuses = [Pending, Succeeded, Failed, Recovering, MppSplitting];

    // Allowed transition set per SRL-1 spec
    let allowed_transitions = [
        (Pending, Succeeded),
        (Pending, Failed),
        (Pending, Recovering),
        (Pending, MppSplitting),
        (MppSplitting, Succeeded),
        (MppSplitting, Failed),
        (MppSplitting, Recovering),
        (Recovering, Succeeded),
        (Recovering, Failed),
        (Recovering, MppSplitting),
        (Failed, Recovering),
        (Failed, MppSplitting),
    ];

    for &from in &all_statuses {
        for &to in &all_statuses {
            let is_allowed = allowed_transitions.contains(&(from, to));
            assert_eq!(
                adapter.validate_transition(from, to),
                is_allowed,
                "Transition from {:?} to {:?} expectation mismatch (expected {})",
                from,
                to,
                is_allowed
            );
        }
    }
}

#[test]
fn test_categorize_failure_full_taxonomy() {
    let adapter = LightningResilienceAdapter::new();

    // Permanent failures
    assert_eq!(
        adapter.categorize_failure("no_route to target node"),
        LightningFailureType::Permanent
    );
    assert_eq!(
        adapter.categorize_failure("invalid_invoice format or expired signature"),
        LightningFailureType::Permanent
    );

    // MPP Partial failures
    assert_eq!(
        adapter.categorize_failure("mpp_partial_failure: path 2 rejected"),
        LightningFailureType::MppPartial
    );
    assert_eq!(
        adapter.categorize_failure("split_error: path liquidity insufficient"),
        LightningFailureType::MppPartial
    );

    // Indeterminate / Timeout failures
    assert_eq!(
        adapter.categorize_failure("timeout waiting for HTLC settlement"),
        LightningFailureType::Indeterminate
    );
    assert_eq!(
        adapter.categorize_failure("mpp_timeout on shard 3"),
        LightningFailureType::Indeterminate
    );

    // Default / Transient failures
    assert_eq!(
        adapter.categorize_failure("connection_refused by peer"),
        LightningFailureType::Transient
    );
    assert_eq!(
        adapter.categorize_failure("temporary channel congestion"),
        LightningFailureType::Transient
    );
}

#[test]
fn test_should_recover_transient_retry_counter_bounds() {
    let adapter = LightningResilienceAdapter::new();
    let now = Utc::now();

    for retries in 0..3 {
        let intent = PaymentIntent {
            payment_id: format!("pay_transient_{}", retries),
            payment_hash: "hash".into(),
            amount_msat: 1000,
            status: LightningPaymentStatus::Failed,
            failure_type: Some(LightningFailureType::Transient),
            retry_count: retries,
            created_at: now,
            last_updated_at: now,
        };
        assert!(
            adapter.should_recover(&intent),
            "Transient failure with retry_count {} should recover",
            retries
        );
    }

    for retries in 3..6 {
        let intent = PaymentIntent {
            payment_id: format!("pay_transient_exhausted_{}", retries),
            payment_hash: "hash".into(),
            amount_msat: 1000,
            status: LightningPaymentStatus::Failed,
            failure_type: Some(LightningFailureType::Transient),
            retry_count: retries,
            created_at: now,
            last_updated_at: now,
        };
        assert!(
            !adapter.should_recover(&intent),
            "Transient failure with retry_count {} must NOT recover (limit reached)",
            retries
        );
    }
}

#[test]
fn test_should_recover_stale_pending_payment() {
    let adapter = LightningResilienceAdapter::new();
    let now = Utc::now();

    // Fresh pending payment (200s old <= 300s threshold)
    let fresh_pending = PaymentIntent {
        payment_id: "fresh_pending".into(),
        payment_hash: "hash".into(),
        amount_msat: 2000,
        status: LightningPaymentStatus::Pending,
        failure_type: None,
        retry_count: 0,
        created_at: now - ChronoDuration::seconds(200),
        last_updated_at: now - ChronoDuration::seconds(200),
    };
    assert!(
        !adapter.should_recover(&fresh_pending),
        "Fresh pending payment (< 300s) should not recover"
    );

    // Stale pending payment (350s old > 300s threshold)
    let stale_pending = PaymentIntent {
        payment_id: "stale_pending".into(),
        payment_hash: "hash".into(),
        amount_msat: 2000,
        status: LightningPaymentStatus::Pending,
        failure_type: None,
        retry_count: 0,
        created_at: now - ChronoDuration::seconds(350),
        last_updated_at: now - ChronoDuration::seconds(350),
    };
    assert!(
        adapter.should_recover(&stale_pending),
        "Stale pending payment (> 300s) must trigger recovery"
    );
}

#[test]
fn test_should_recover_stale_mpp_splitting_payment() {
    let adapter = LightningResilienceAdapter::new();
    let now = Utc::now();

    // Fresh MPP splitting (400s old <= 600s threshold)
    let fresh_mpp = PaymentIntent {
        payment_id: "fresh_mpp".into(),
        payment_hash: "hash".into(),
        amount_msat: 5000,
        status: LightningPaymentStatus::MppSplitting,
        failure_type: Some(LightningFailureType::MppPartial),
        retry_count: 0,
        created_at: now - ChronoDuration::seconds(400),
        last_updated_at: now - ChronoDuration::seconds(400),
    };
    assert!(
        !adapter.should_recover(&fresh_mpp),
        "Fresh MPP splitting (< 600s) should not recover"
    );

    // Stale MPP splitting (650s old > 600s threshold)
    let stale_mpp = PaymentIntent {
        payment_id: "stale_mpp".into(),
        payment_hash: "hash".into(),
        amount_msat: 5000,
        status: LightningPaymentStatus::MppSplitting,
        failure_type: Some(LightningFailureType::MppPartial),
        retry_count: 0,
        created_at: now - ChronoDuration::seconds(650),
        last_updated_at: now - ChronoDuration::seconds(650),
    };
    assert!(
        adapter.should_recover(&stale_mpp),
        "Stale MPP splitting (> 600s) must trigger recovery"
    );
}

#[test]
fn test_process_recovery_state_mutations() {
    let adapter = LightningResilienceAdapter::new();
    let now = Utc::now();

    // 1. Transient failure recovery
    let mut transient_intent = PaymentIntent {
        payment_id: "p_transient".into(),
        payment_hash: "hash".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::Transient),
        retry_count: 1,
        created_at: now,
        last_updated_at: now,
    };
    let action = adapter.process_recovery(&mut transient_intent);
    assert_eq!(action, Some("retry_initiated"));
    assert_eq!(transient_intent.status, LightningPaymentStatus::Recovering);
    assert_eq!(transient_intent.retry_count, 2);

    // 2. MPP Partial failure recovery
    let mut mpp_intent = PaymentIntent {
        payment_id: "p_mpp".into(),
        payment_hash: "hash".into(),
        amount_msat: 5000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::MppPartial),
        retry_count: 0,
        created_at: now,
        last_updated_at: now,
    };
    let action = adapter.process_recovery(&mut mpp_intent);
    assert_eq!(action, Some("split_recovery_triggered"));
    assert_eq!(mpp_intent.status, LightningPaymentStatus::MppSplitting);

    // 3. Indeterminate failure recovery
    let mut ind_intent = PaymentIntent {
        payment_id: "p_ind".into(),
        payment_hash: "hash".into(),
        amount_msat: 3000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::Indeterminate),
        retry_count: 0,
        created_at: now,
        last_updated_at: now,
    };
    let action = adapter.process_recovery(&mut ind_intent);
    assert_eq!(action, Some("reconciliation_requested"));
    assert_eq!(ind_intent.status, LightningPaymentStatus::Recovering);

    // 4. Stale pending recovery
    let mut stale_intent = PaymentIntent {
        payment_id: "p_stale".into(),
        payment_hash: "hash".into(),
        amount_msat: 4000,
        status: LightningPaymentStatus::Pending,
        failure_type: None,
        retry_count: 0,
        created_at: now - ChronoDuration::seconds(400),
        last_updated_at: now - ChronoDuration::seconds(400),
    };
    let action = adapter.process_recovery(&mut stale_intent);
    assert_eq!(action, Some("stale_payment_recovery"));
    assert_eq!(stale_intent.status, LightningPaymentStatus::Recovering);

    // 5. Permanent failure (no action)
    let mut perm_intent = PaymentIntent {
        payment_id: "p_perm".into(),
        payment_hash: "hash".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::Permanent),
        retry_count: 0,
        created_at: now,
        last_updated_at: now,
    };
    let action = adapter.process_recovery(&mut perm_intent);
    assert_eq!(action, None);
    assert_eq!(perm_intent.status, LightningPaymentStatus::Failed);
}
