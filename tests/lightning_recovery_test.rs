use chrono::Utc;
use conxian_nexus::executor::lightning::{
    LightningFailureType, LightningPaymentStatus, PaymentIntent,
};

#[test]
fn test_lightning_recovery_logic_transient() {
    let adapter = conxian_nexus::executor::lightning::LightningResilienceAdapter::new();
    let mut intent = PaymentIntent {
        payment_id: "p1".to_string(),
        payment_hash: "h1".to_string(),
        amount_msat: 1000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::Transient),
        retry_count: 0,
        created_at: Utc::now(),
        last_updated_at: Utc::now(),
    };

    let action = adapter.process_recovery(&mut intent);
    assert_eq!(action, Some("retry_initiated"));
    assert_eq!(intent.status, LightningPaymentStatus::Recovering);
    assert_eq!(intent.retry_count, 1);
}

#[test]
fn test_lightning_recovery_logic_mpp() {
    let adapter = conxian_nexus::executor::lightning::LightningResilienceAdapter::new();
    let mut intent = PaymentIntent {
        payment_id: "p2".to_string(),
        payment_hash: "h2".to_string(),
        amount_msat: 5000,
        status: LightningPaymentStatus::Failed,
        failure_type: Some(LightningFailureType::MppPartial),
        retry_count: 0,
        created_at: Utc::now(),
        last_updated_at: Utc::now(),
    };

    let action = adapter.process_recovery(&mut intent);
    assert_eq!(action, Some("split_recovery_triggered"));
    assert_eq!(intent.status, LightningPaymentStatus::MppSplitting);
}
