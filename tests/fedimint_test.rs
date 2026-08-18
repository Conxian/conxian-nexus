use std::sync::Arc;
use conxian_nexus::config::Config;
use conxian_nexus::executor::fedimint::FedimintAdapter;
use conxian_nexus::storage::Storage;

#[tokio::test]
async fn test_fedimint_adapter_proof_verification_flow() {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).expect("Failed to initialize lazy storage"));
    let adapter = FedimintAdapter::new(storage);

    let valid_proof = "fed:ecash-blinded-mint-note-sample-payload-for-integration-testing";

    // First verification should succeed
    let result = adapter
        .verify_mint_proof_detailed(valid_proof, "fed:fedimint_integration_fed_1", 10000)
        .await
        .expect("Verification execution failed");

    assert!(result.valid, "Expected valid proof verification result");
    assert_eq!(result.federation_id, "fed:fedimint_integration_fed_1");
    assert_eq!(result.amount_sats, 10000);
    assert!(!result.proof_hash.is_empty());
    assert!(!result.nonce_hash.is_empty());

    // Basic helper function call check
    let simple_valid = adapter.verify_mint_proof("fed1:another-valid-proof-string-with-sufficient-length").await;
    assert!(simple_valid.is_ok());
}

#[tokio::test]
async fn test_fedimint_adapter_rejects_malformed_proofs() {
    let config = Config::default_test();
    let storage = Arc::new(Storage::from_config_lazy(&config).expect("Failed to initialize lazy storage"));
    let adapter = FedimintAdapter::new(storage);

    let invalid_prefix = adapter.verify_mint_proof("badprefix:123456789012345678901234567890123").await;
    assert!(invalid_prefix.is_err(), "Expected error for invalid prefix");

    let short_proof = adapter.verify_mint_proof("fed:too-short").await;
    assert!(short_proof.is_err(), "Expected error for short proof");
}
