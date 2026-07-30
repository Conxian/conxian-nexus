use chrono::Utc;
use conxian_nexus::executor::lightning::{
    LightningFailureType, LightningPaymentStatus, LightningResilienceAdapter, PaymentEvent,
    PaymentIntent,
};

#[test]
fn test_failure_type_display() {
    assert_eq!(LightningFailureType::Permanent.to_string(), "permanent");
    assert_eq!(LightningFailureType::Transient.to_string(), "transient");
    assert_eq!(
        LightningFailureType::Indeterminate.to_string(),
        "indeterminate"
    );
    assert_eq!(
        LightningFailureType::MppPartial.to_string(),
        "mpp_partial"
    );
}

#[test]
fn test_payment_status_display() {
    assert_eq!(LightningPaymentStatus::Pending.to_string(), "pending");
    assert_eq!(LightningPaymentStatus::Succeeded.to_string(), "succeeded");
    assert_eq!(LightningPaymentStatus::Failed.to_string(), "failed");
    assert_eq!(LightningPaymentStatus::Recovering.to_string(), "recovering");
    assert_eq!(
        LightningPaymentStatus::MppSplitting.to_string(),
        "mpp_splitting"
    );
}

#[test]
fn test_payment_intent_construction() {
    let now = Utc::now();
    let intent = PaymentIntent {
        payment_id: "pay_1".into(),
        payment_hash: "hash_1".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Pending,
        failure_type: None,
        retry_count: 0,
        created_at: now,
        last_updated_at: now,
    };
    assert_eq!(intent.payment_id, "pay_1");
    assert_eq!(intent.amount_msat, 1000);
}

#[test]
fn test_payment_event_construction() {
    let now = Utc::now();
    let event = PaymentEvent {
        event_id: "evt_1".into(),
        payment_id: "pay_1".into(),
        status: LightningPaymentStatus::Succeeded,
        failure_type: None,
        timestamp: now,
        metadata: Some(r#"{"source":"test"}"#.into()),
    };
    assert_eq!(event.event_id, "evt_1");
    assert_eq!(event.status, LightningPaymentStatus::Succeeded);
    assert!(event.metadata.is_some());
}

#[test]
fn test_default_adapter() {
    let adapter = LightningResilienceAdapter::default();
    // Default must construct without panicking
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Pending,
        LightningPaymentStatus::Succeeded
    ));
}

#[test]
fn test_categorize_all_failure_types() {
    let adapter = LightningResilienceAdapter::new();
    assert_eq!(
        adapter.categorize_failure("no_route"),
        LightningFailureType::Permanent
    );
    assert_eq!(
        adapter.categorize_failure("invalid_invoice"),
        LightningFailureType::Permanent
    );
    assert_eq!(
        adapter.categorize_failure("mpp_partial_failure"),
        LightningFailureType::MppPartial
    );
    assert_eq!(
        adapter.categorize_failure("split_error"),
        LightningFailureType::MppPartial
    );
    assert_eq!(
        adapter.categorize_failure("timeout"),
        LightningFailureType::Indeterminate
    );
    assert_eq!(
        adapter.categorize_failure("mpp_timeout"),
        LightningFailureType::Indeterminate
    );
    assert_eq!(
        adapter.categorize_failure("temporary_node_failure"),
        LightningFailureType::Transient
    );
}

#[test]
fn test_validate_all_transitions() {
    let adapter = LightningResilienceAdapter::new();
    // Pending transitions
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Pending,
        LightningPaymentStatus::Succeeded
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Pending,
        LightningPaymentStatus::Failed
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Pending,
        LightningPaymentStatus::Recovering
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Pending,
        LightningPaymentStatus::MppSplitting
    ));
    // MppSplitting transitions
    assert!(adapter.validate_transition(
        LightningPaymentStatus::MppSplitting,
        LightningPaymentStatus::Succeeded
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::MppSplitting,
        LightningPaymentStatus::Failed
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::MppSplitting,
        LightningPaymentStatus::Recovering
    ));
    // Recovering transitions
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Recovering,
        LightningPaymentStatus::Succeeded
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Recovering,
        LightningPaymentStatus::Failed
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Recovering,
        LightningPaymentStatus::MppSplitting
    ));
    // Failed transitions
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Failed,
        LightningPaymentStatus::Recovering
    ));
    assert!(adapter.validate_transition(
        LightningPaymentStatus::Failed,
        LightningPaymentStatus::MppSplitting
    ));
    // Invalid
    assert!(!adapter.validate_transition(
        LightningPaymentStatus::Succeeded,
        LightningPaymentStatus::Failed
    ));
    assert!(!adapter.validate_transition(
        LightningPaymentStatus::Failed,
        LightningPaymentStatus::Succeeded
    ));
}

#[test]
fn test_should_recover_all_cases() {
    let adapter = LightningResilienceAdapter::new();
    let now = Utc::now();

    // Recovering always triggers recovery
    let recovering = PaymentIntent {
        payment_id: "p1".into(),
        payment_hash: "h1".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Recovering,
        failure_type: None,
        retry_count: 0,
        created_at: now,
        last_updated_at: now,
    };
    assert!(adapter.should_recover(&recovering));

    // Failed + Transient with retries < 3
    let transient = PaymentIntent {
        payment_id: "p2".into(),
        payment_hash: "h2".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::Transient),
        retry_count: 2,
        created_at: now,
        last_updated_at: now,
    };
    assert!(adapter.should_recover(&transient));

    // Failed + Transient with retries >= 3 should NOT recover
    let exhausted = PaymentIntent {
        payment_id: "p3".into(),
        payment_hash: "h3".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::Transient),
        retry_count: 3,
        created_at: now,
        last_updated_at: now,
    };
    assert!(!adapter.should_recover(&exhausted));

    // Succeeded should NOT recover
    let success = PaymentIntent {
        payment_id: "p4".into(),
        payment_hash: "h4".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Succeeded,
        failure_type: None,
        retry_count: 0,
        created_at: now,
        last_updated_at: now,
    };
    assert!(!adapter.should_recover(&success));

    // Pending should NOT recover
    let pending = PaymentIntent {
        payment_id: "p5".into(),
        payment_hash: "h5".into(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Pending,
        failure_type: None,
        retry_count: 0,
        created_at: now,
        last_updated_at: now,
    };
    assert!(!adapter.should_recover(&pending));
}
