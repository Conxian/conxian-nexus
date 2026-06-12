use conxian_nexus::executor::lightning::{LightningResilienceAdapter, LightningPaymentStatus, LightningFailureType};

#[test]
fn test_lightning_resilience_logic() {
    let adapter = LightningResilienceAdapter::new();

    // Test Failure Categorization
    assert_eq!(adapter.categorize_failure("permanent: no_route"), LightningFailureType::Permanent);
    assert_eq!(adapter.categorize_failure("transient: temporary_node_failure"), LightningFailureType::Transient);
    assert_eq!(adapter.categorize_failure("indeterminate: mpp_timeout"), LightningFailureType::Indeterminate);

    // Test State Transitions
    assert!(adapter.validate_transition(LightningPaymentStatus::Pending, LightningPaymentStatus::Succeeded));
    assert!(adapter.validate_transition(LightningPaymentStatus::Pending, LightningPaymentStatus::Failed));
    assert!(adapter.validate_transition(LightningPaymentStatus::Pending, LightningPaymentStatus::Recovering));
    assert!(adapter.validate_transition(LightningPaymentStatus::Recovering, LightningPaymentStatus::Succeeded));
    assert!(adapter.validate_transition(LightningPaymentStatus::Recovering, LightningPaymentStatus::Failed));
    assert!(adapter.validate_transition(LightningPaymentStatus::Failed, LightningPaymentStatus::Recovering));

    // Test Invalid Transitions
    assert!(!adapter.validate_transition(LightningPaymentStatus::Succeeded, LightningPaymentStatus::Failed));
    assert!(!adapter.validate_transition(LightningPaymentStatus::Failed, LightningPaymentStatus::Succeeded));
}
