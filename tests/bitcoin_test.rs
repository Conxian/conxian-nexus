use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use conxian_nexus::api::rest::app_router;
use conxian_nexus::config::Config;
use conxian_nexus::executor::rgb::{RGBAdapter, RGBRolloutMode};
use conxian_nexus::executor::NexusExecutor;
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tower::Service;

#[tokio::test]
async fn test_rgb_adapter_disabled_rejects_all() {
    let adapter = RGBAdapter::new(RGBRolloutMode::Disabled);

    let result = adapter
        .lookup_contract("rgb:test123456_nia_long_enough_id_for_validation")
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "RGB adapter is disabled");
}

#[tokio::test]
async fn test_rgb_adapter_shadow_returns_mock() {
    let adapter = RGBAdapter::new(RGBRolloutMode::Shadow);

    let result = adapter
        .lookup_contract("rgb:test123456_nia_long_enough_id_for_validation")
        .await;
    assert!(result.is_ok());

    let metadata = result
        .unwrap()
        .expect("Shadow mode must return mock payload");
    let json = serde_json::to_value(&metadata).unwrap();
    assert_eq!(
        json["contract_id"],
        "rgb:test123456_nia_long_enough_id_for_validation"
    );
    assert_eq!(json["mode"], "shadow");
    assert_eq!(json["status"], "verified");
}

#[tokio::test]
async fn test_rgb_adapter_active_known_contract() {
    let mut known = HashSet::new();
    known.insert("rgb:known12345_nia_long_enough_id_for_validation".to_string());
    let adapter = RGBAdapter::with_known_contracts(RGBRolloutMode::Active, known);

    let result = adapter
        .lookup_contract("rgb:known12345_nia_long_enough_id_for_validation")
        .await;
    assert!(result.is_ok());

    let metadata = result.unwrap().expect("Known contract must resolve");
    let json = serde_json::to_value(&metadata).unwrap();
    assert_eq!(
        json["contract_id"],
        "rgb:known12345_nia_long_enough_id_for_validation"
    );
    assert_eq!(json["mode"], "active");
    assert_eq!(json["status"], "active");
}

#[tokio::test]
async fn test_rgb_adapter_active_unknown_contract() {
    let adapter = RGBAdapter::with_known_contracts(RGBRolloutMode::Active, HashSet::new());

    let result = adapter
        .lookup_contract("rgb:unknown1234_nia_long_enough_id_for_validation")
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_rgb_adapter_invalid_contract_id_format() {
    let adapter = RGBAdapter::new(RGBRolloutMode::Shadow);

    // Missing rgb: prefix
    let result = adapter.lookup_contract("notrgb").await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid RGB contract ID prefix"),
        "Should reject IDs without rgb: prefix"
    );

    // Too short
    let result = adapter.lookup_contract("rgb:ab").await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid RGB contract ID length"),
        "Should reject IDs that are too short"
    );
}

#[tokio::test]
async fn test_rgb_adapter_empty_known_contracts_active() {
    let adapter = RGBAdapter::with_known_contracts(RGBRolloutMode::Active, HashSet::new());

    let result = adapter
        .lookup_contract("rgb:nonexistent_nia_long_enough_id_for_validation")
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_rgb_adapter_display_formats() {
    assert_eq!(format!("{}", RGBRolloutMode::Disabled), "disabled");
    assert_eq!(format!("{}", RGBRolloutMode::Shadow), "shadow");
    assert_eq!(format!("{}", RGBRolloutMode::Active), "active");
}

#[tokio::test]
async fn test_rgb_adapter_serde_roundtrip() {
    let mode = RGBRolloutMode::Shadow;
    let serialized = serde_json::to_string(&mode).unwrap();
    assert_eq!(serialized, "\"shadow\"");
    let deserialized: RGBRolloutMode = serde_json::from_str(&serialized).unwrap();
    assert_eq!(mode, deserialized);
}

// ---------------------------------------------------------------------------
// DLC Bond Handlers — Success Path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dlc_bond_creation_success_path() {
    let config = Config::default_test();
    let storage = match Storage::from_config_lazy(&config) {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let mut app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        Arc::new(Config::default_test()),
    );

    let response = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/dlc/bond")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"bond_id":"bond123","principal_sbtc":1000000,"expiry_height":100,"coupon_rate":0.05}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status() == StatusCode::CREATED
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 201 or 500 (if Redis missing), got {}",
        response.status()
    );

    if response.status() == StatusCode::CREATED {
        let body = axum::body::to_bytes(response.into_body(), 2048)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "Initialized");
        assert!(
            json["dlc_contract_id"]
                .as_str()
                .unwrap()
                .starts_with("dlc_"),
            "Contract ID must start with dlc_"
        );
        assert!(
            json["oracle_announcement"].as_str().unwrap().len() > 20,
            "Oracle announcement must be a valid signature string"
        );
        assert_eq!(json["next_coupon_height"], 10); // expiry_height / 10 = 100 / 10
    }
}

// ---------------------------------------------------------------------------
// DLC Bond Handlers — Validation Failure Paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dlc_bond_creation_validation_empty_bond_id() {
    let config = Config::default_test();
    let storage = match Storage::from_config_lazy(&config) {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let mut app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        Arc::new(Config::default_test()),
    );

    let response = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/dlc/bond")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"bond_id":"","principal_sbtc":1000000,"expiry_height":100,"coupon_rate":0.05}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_dlc_bond_creation_validation_zero_principal() {
    let config = Config::default_test();
    let storage = match Storage::from_config_lazy(&config) {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let mut app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        Arc::new(Config::default_test()),
    );

    let response = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/dlc/bond")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"bond_id":"bond_valid","principal_sbtc":0,"expiry_height":100,"coupon_rate":0.05}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_dlc_bond_creation_invalid_json() {
    let config = Config::default_test();
    let storage = match Storage::from_config_lazy(&config) {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let mut app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        Arc::new(Config::default_test()),
    );

    let response = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/dlc/bond")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"invalid_json": true"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_dlc_bond_creation_high_coupon_rate() {
    let config = Config::default_test();
    let storage = match Storage::from_config_lazy(&config) {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };
    let nexus_state = Arc::new(NexusState::new());
    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let mut app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        Arc::new(Config::default_test()),
    );

    // Bond with extreme values to test serialization boundaries
    let response = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/dlc/bond")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"bond_id":"high-rate-bond","principal_sbtc":999999999,"expiry_height":9999999,"coupon_rate":0.99}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status() == StatusCode::CREATED
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 201 or 500, got {}",
        response.status()
    );

    if response.status() == StatusCode::CREATED {
        let body = axum::body::to_bytes(response.into_body(), 2048)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "Initialized");
    }
}

// ---------------------------------------------------------------------------
// BTC Transaction Format Validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_btc_tx_id_format_validation() {
    // BTC transaction IDs must be 64-character hex (0x-prefixed 66-char for Nexus)
    // Valid: 0x + 64 hex chars = 66 chars
    let valid_txid = "0x".to_owned() + &"a".repeat(64);
    assert_eq!(valid_txid.len(), 66);

    let config = Config::default_test();
    let storage = match Storage::from_config_lazy(&config) {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping test: Database not available");
            return;
        }
    };
    let nexus_state = Arc::new(NexusState::new());
    nexus_state.update_state(&valid_txid, 100);

    let executor = Arc::new(NexusExecutor::new(
        storage.clone(),
        RGBRolloutMode::Disabled,
        HashSet::new(),
    ));
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let mut app = app_router(
        storage,
        nexus_state,
        executor,
        None,
        tableland,
        None,
        None,
        Arc::new(Config::default_test()),
    );

    // Valid BTC tx_id should work
    let response = app
        
        .call(
            Request::builder()
                .method("GET")
                .uri(&format!("/v1/mmr-proof?tx_id={}", valid_txid))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Expect either 200 (proof found) or 500 (DB missing siblings)
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 200 or 500 for valid BTC tx_id format, got {}",
        response.status()
    );

    // Invalid: not 0x-prefixed
    let invalid_txid = "a".repeat(64); // no 0x prefix
    let response = app
        
        .call(
            Request::builder()
                .method("GET")
                .uri(&format!("/v1/mmr-proof?tx_id={}", invalid_txid))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Non-0x-prefixed tx_id should be rejected"
    );

    // Invalid: wrong length (not 66)
    let short_txid = "0x".to_owned() + &"a".repeat(32); // only 34 chars
    let response = app
        
        .call(
            Request::builder()
                .method("GET")
                .uri(&format!("/v1/mmr-proof?tx_id={}", short_txid))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Wrong-length tx_id should be rejected"
    );
}

#[tokio::test]
async fn test_btc_tx_state_transition() {
    // Verify that BTC-like transaction IDs can be added to Nexus state
    // and produce valid Merkle proofs
    let state = Arc::new(NexusState::new());

    // Simulate a batch of BTC transactions
    let btc_txns: Vec<String> = (0..10).map(|i| format!("0x{:064x}", i)).collect();

    state.update_state_batch(&btc_txns);

    // Verify each transaction has a valid proof
    for tx in &btc_txns {
        let proof = state.generate_merkle_proof(tx);
        assert!(proof.is_some(), "BTC tx {} must have a Merkle proof", tx);
        let proof = proof.unwrap();
        assert_eq!(proof.leaf, *tx);
        assert!(conxian_nexus::state::verify_merkle_proof(&proof));
    }

    // Verify the root is consistent
    let root = state.get_state_root();
    assert_ne!(
        root,
        "0x0000000000000000000000000000000000000000000000000000000000000000"
    );
}

#[tokio::test]
async fn test_rgb_contract_lookup_with_btc_tx_ids() {
    // Verify RGB adapter handles BTC tx IDs correctly (rejects non-rgb: prefixed)
    let adapter = RGBAdapter::new(RGBRolloutMode::Shadow);

    let btc_tx = "0x".to_owned() + &"a".repeat(64);
    let result = adapter.lookup_contract(&btc_tx).await;
    assert!(result.is_err(), "RGB adapter must reject BTC tx IDs");
}

// ---------------------------------------------------------------------------
// RGB Adapter — Concurrent Access
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rgb_adapter_concurrent_lookups() {
    let adapter = Arc::new(RGBAdapter::with_known_contracts(
        RGBRolloutMode::Active,
        [
            "rgb:alpha12345_nia_long_enough_id_for_validation",
            "rgb:beta123456_nia_long_enough_id_for_validation",
            "rgb:gamma12345_nia_long_enough_id_for_validation",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    ));

    let mut handles = Vec::new();
    for id in &[
        "rgb:alpha12345_nia_long_enough_id_for_validation",
        "rgb:beta123456_nia_long_enough_id_for_validation",
        "rgb:gamma12345_nia_long_enough_id_for_validation",
        "rgb:unknown_nia_long_enough_id_for_validation",
    ] {
        let adapter = adapter.clone();
        let id = id.to_string();
        handles.push(tokio::spawn(
            async move { adapter.lookup_contract(&id).await },
        ));
    }

    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        let is_known = i < 3;
        if is_known {
            assert!(result.is_ok());
            assert!(result.unwrap().is_some(), "Known contract should resolve");
        } else {
            assert!(result.is_ok());
            assert!(
                result.unwrap().is_none(),
                "Unknown contract should return None"
            );
        }
    }
}

#[tokio::test]
async fn test_rgb_validation_edge_cases() {
    let adapter = conxian_nexus::executor::rgb::RGBAdapter::new(
        conxian_nexus::executor::rgb::RGBRolloutMode::Shadow,
    );

    // Prefix check
    assert!(adapter.validate_contract_id("notrgb:123").is_err());

    // Length check
    assert!(adapter.validate_contract_id("rgb:short").is_err());

    // Schema heuristics
    assert_eq!(
        adapter
            .validate_contract_id("rgb:asset_nia_123456789012345678901234567890")
            .unwrap(),
        conxian_nexus::executor::rgb::RGBSchema::NIA
    );
    assert_eq!(
        adapter
            .validate_contract_id("rgb:asset_lnpbp_123456789012345678901234567890")
            .unwrap(),
        conxian_nexus::executor::rgb::RGBSchema::LNPBP
    );
    assert_eq!(
        adapter
            .validate_contract_id("rgb:generic_123456789012345678901234567890")
            .unwrap(),
        conxian_nexus::executor::rgb::RGBSchema::Unknown
    );
}

#[tokio::test]
async fn test_bitvm_validation_edge_cases() {
    let config = conxian_nexus::config::Config::default_test();
    let storage = conxian_nexus::storage::Storage::from_config_lazy(&config).unwrap();
    let adapter = conxian_nexus::executor::bitvm::BitVMAdapter::new(std::sync::Arc::new(storage));

    let mut transition = conxian_nexus::executor::bitvm::BitVMTransition {
        prev_state_root: "short".to_string(),
        next_state_root: "0x0000000000000000000000000000000000000000000000000000000000000002"
            .to_string(),
        proof_bytes: "00".to_string(),
        vk_bytes: "00".to_string(),
        public_inputs: vec![],
        trace_id: "t1".to_string(),
    };

    let res = adapter.verify_transition(&transition).await.unwrap();
    assert!(!res.valid);
    assert!(res.message.contains("prev_state_root"));

    transition.prev_state_root =
        "0x0000000000000000000000000000000000000000000000000000000000000001".to_string();
    transition.next_state_root = "bad".to_string();
    let res = adapter.verify_transition(&transition).await.unwrap();
    assert!(!res.valid);
    assert!(res.message.contains("next_state_root"));
}
