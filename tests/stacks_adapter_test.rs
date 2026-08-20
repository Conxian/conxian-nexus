use conxian_nexus::config::Config;
use conxian_nexus::executor::stacks::{StacksAdapter, StacksTransaction};
use conxian_nexus::storage::Storage;
use std::sync::Arc;

#[tokio::test]
async fn test_stacks_adapter_structural_validation() {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let adapter = StacksAdapter::new(storage);

    // Test valid transaction ID and amount
    let valid_tx = StacksTransaction {
        tx_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
        block_height: 100,
        sender: "SP2J6ZY48GV1EZ5V2V5RB9MP66SW86PYKKQ9H6DPR".to_string(),
        amount_sbtc: 1000,
    };
    let result = adapter.verify_transaction(&valid_tx).await.unwrap();
    assert!(result.valid);
    assert!(result.status.contains("CON-1200 Phase 2"));

    // Test invalid transaction ID format
    let invalid_tx_id = StacksTransaction {
        tx_id: "invalid".to_string(),
        block_height: 100,
        sender: "SP2J6ZY48GV1EZ5V2V5RB9MP66SW86PYKKQ9H6DPR".to_string(),
        amount_sbtc: 1000,
    };
    let result = adapter.verify_transaction(&invalid_tx_id).await.unwrap();
    assert!(!result.valid);
    assert!(result.status.contains("Invalid transaction ID format"));

    // Test zero amount sBTC
    let zero_amount_tx = StacksTransaction {
        tx_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
        block_height: 100,
        sender: "SP2J6ZY48GV1EZ5V2V5RB9MP66SW86PYKKQ9H6DPR".to_string(),
        amount_sbtc: 0,
    };
    let result = adapter.verify_transaction(&zero_amount_tx).await.unwrap();
    assert!(!result.valid);
    assert!(result.status.contains("Zero amount sBTC transaction"));
}
