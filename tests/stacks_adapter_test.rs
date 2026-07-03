use conxian_nexus::executor::stacks::{StacksAdapter, StacksTransaction};

#[tokio::test]
async fn test_stacks_adapter_structural_validation() {
    let adapter = StacksAdapter::new();

    // Test valid transaction ID and amount
    let valid_tx = StacksTransaction {
        tx_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
        block_height: 100,
        sender: "ST1234".to_string(),
        amount_sbtc: 1000,
    };
    let result = adapter.verify_transaction(&valid_tx).await.unwrap();
    assert!(result.valid);
    assert_eq!(result.status, "Stacks transaction verified (Pilot)");

    // Test invalid transaction ID format
    let invalid_tx_id = StacksTransaction {
        tx_id: "invalid".to_string(),
        block_height: 100,
        sender: "ST1234".to_string(),
        amount_sbtc: 1000,
    };
    let result = adapter.verify_transaction(&invalid_tx_id).await.unwrap();
    assert!(!result.valid);
    assert_eq!(result.status, "Invalid transaction ID format");

    // Test zero amount sBTC
    let zero_amount_tx = StacksTransaction {
        tx_id: "0x1234".to_string(),
        block_height: 100,
        sender: "ST1234".to_string(),
        amount_sbtc: 0,
    };
    let result = adapter.verify_transaction(&zero_amount_tx).await.unwrap();
    assert!(!result.valid);
    assert_eq!(result.status, "Zero amount sBTC transaction");
}
