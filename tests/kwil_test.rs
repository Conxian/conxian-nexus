use conxian_nexus::storage::kwil::{
    KwilBlockCommitment, KwilMmrNodeCommitment, KwilStateRootCommitment, KwilAdapter, KwilConfig,
    KwilSettlementProposalCommitment, KwilSettlementLogCommitment
};
use conxian_nexus::config::Config;
use conxian_nexus::storage::Storage;
use lib_conxian_core::Wallet;
use std::sync::Arc;

#[tokio::test]
async fn test_kwil_block_persistence_pilot_signed() {
    let config = Config::default_test();
    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => return,
    };
    let wallet = Arc::new(Wallet::new().unwrap());
    let adapter = KwilAdapter::new(
        storage,
        KwilConfig {
            provider_url: "http://localhost:8080".to_string(),
            db_id: "nexus_test".to_string(),
        },
        wallet.clone(),
    ).unwrap();

    let commitment = KwilBlockCommitment {
        hash: "0x123".to_string(),
        height: 100,
        block_type: "burn".to_string(),
        state: "hard".to_string(),
    };

    // Should fail with connection error as no provider exists
    let _ = adapter.persist_block(commitment).await;
}

#[tokio::test]
async fn test_kwil_state_root_persistence_pilot_signed() {
    let config = Config::default_test();
    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => return,
    };
    let wallet = Arc::new(Wallet::new().unwrap());
    let adapter = KwilAdapter::new(
        storage,
        KwilConfig {
            provider_url: "http://localhost:8080".to_string(),
            db_id: "nexus_test".to_string(),
        },
        wallet,
    ).unwrap();

    let commitment = KwilStateRootCommitment {
        block_height: 100,
        state_root: "0xroot123".to_string(),
    };

    let _ = adapter.persist_state_root(commitment).await;
}

#[tokio::test]
async fn test_kwil_mmr_node_persistence_pilot_signed() {
    let config = Config::default_test();
    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => return,
    };
    let wallet = Arc::new(Wallet::new().unwrap());
    let adapter = KwilAdapter::new(
        storage,
        KwilConfig {
            provider_url: "http://localhost:8080".to_string(),
            db_id: "nexus_test".to_string(),
        },
        wallet.clone(),
    ).unwrap();

    let commitment = KwilMmrNodeCommitment {
        pos: 1,
        hash: "0xmmr123".to_string(),
        block_height: 1000,
    };

    let _ = adapter.persist_mmr_node(commitment).await;
}

#[tokio::test]
async fn test_kwil_settlement_proposal_persistence_pilot_signed() {
    let config = Config::default_test();
    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => return,
    };
    let wallet = Arc::new(Wallet::new().unwrap());
    let adapter = KwilAdapter::new(
        storage,
        KwilConfig {
            provider_url: "http://localhost:8080".to_string(),
            db_id: "nexus_test".to_string(),
        },
        wallet,
    ).unwrap();

    let commitment = KwilSettlementProposalCommitment {
        proposal_id: "prop1".to_string(),
        external_id: "ext1".to_string(),
        source: "ISO20022".to_string(),
        payload: serde_json::json!({"test": "data"}),
        status: "pending".to_string(),
        init_height: 1000,
        unlock_height: 1144,
    };

    let _ = adapter.persist_settlement_proposal(commitment).await;
}

#[tokio::test]
async fn test_kwil_settlement_log_persistence_pilot_signed() {
    let config = Config::default_test();
    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => return,
    };
    let wallet = Arc::new(Wallet::new().unwrap());
    let adapter = KwilAdapter::new(
        storage,
        KwilConfig {
            provider_url: "http://localhost:8080".to_string(),
            db_id: "nexus_test".to_string(),
        },
        wallet,
    ).unwrap();

    let commitment = KwilSettlementLogCommitment {
        external_tx_reference: "ref1".to_string(),
        settlement_network_origin: "ISO20022".to_string(),
        fiat_value_pegged: Some(100.50),
        raw_payload: serde_json::json!({"test": "log"}),
    };

    let _ = adapter.persist_settlement_log(commitment).await;
}
