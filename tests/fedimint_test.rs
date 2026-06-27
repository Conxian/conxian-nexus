use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::storage::Storage;
use conxian_nexus::config::Config;
use std::sync::Arc;
use std::collections::HashSet;

#[tokio::test]
async fn test_fedimint_adapter_structural_validation() {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).unwrap());
    let executor = NexusExecutor::new(storage, RGBRolloutMode::Disabled, HashSet::new());

    let result = executor.fedimint_adapter.verify_mint_proof("mock_proof").await;
    assert!(result.is_ok());
    assert!(result.unwrap());
}
