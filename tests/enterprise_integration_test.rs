use conxian_nexus::config::Config;
use conxian_nexus::executor::rgb::RGBRolloutMode;
use conxian_nexus::executor::{ComplianceZkSanitizedEvent, Iso20022FinalityEvent, NexusExecutor};
use conxian_nexus::state::NexusState;
use conxian_nexus::storage::tableland::TablelandAdapter;
use conxian_nexus::storage::Storage;
use conxian_nexus::sync::{
    NexusSync, PosSettlementEvent, SmeEscrowSyncEvent, SupplyProvenanceEvent,
};
use std::collections::HashSet;
use std::sync::Arc;

#[tokio::test]
async fn test_e2e_enterprise_erp_blockchain_integration() {
    let config = Config::default_test();
    let storage = match Storage::from_config(&config).await {
        Ok(s) => Arc::new(s),
        Err(_) => {
            eprintln!("Skipping integration test: PostgreSQL database connection not available");
            return;
        }
    };
    let state_tracker = Arc::new(NexusState::new());
    let tableland = Arc::new(TablelandAdapter::new(
        storage.clone(),
        config.tableland_base_url.clone(),
    ));

    let sync = NexusSync::new(
        storage.clone(),
        state_tracker.clone(),
        tableland.clone(),
        None,
        config.stacks_node_rpc_url.clone(),
        config.stacks_node_ws_url.clone(),
    );

    let executor = NexusExecutor::new(storage.clone(), RGBRolloutMode::Disabled, HashSet::new());

    // 1. Banking & Finance: ISO 20022 Cross-Border Finality
    let iso_event = Iso20022FinalityEvent {
        uetr: "uetr-999-cbpr-final".to_string(),
        msg_type: "pacs.008".to_string(),
        debtor_agent: "DEUTFFMM".to_string(),
        creditor_agent: "CHASUS33".to_string(),
        amount: 2500000.0,
        currency: "USD".to_string(),
        settlement_status: "FINALIZED".to_string(),
        chain_target: "EVM_MAINNET".to_string(),
    };
    let iso_proof = executor
        .process_iso20022_finality(iso_event)
        .await
        .expect("ISO 20022 finality processing should succeed");
    assert!(iso_proof.starts_with("0x"));

    // 2. Retail & POS: JSON Webhook Normalized Event Ingestion into MMR
    let pos_event = PosSettlementEvent {
        commitment_id: "pos_commit_777".to_string(),
        merchant_id: "merch_starbucks_001".to_string(),
        terminal_id: "term_99".to_string(),
        transaction_count: 150,
        total_amount: 1450.75,
        currency: "USD".to_string(),
    };
    let (pos_node_pos, pos_root) = sync
        .ingest_pos_settlement(pos_event)
        .await
        .expect("POS settlement ingestion should succeed");
    assert!(pos_root.starts_with("0x"));
    assert_eq!(pos_node_pos, 0);

    // 3. Logistics & Supply: EDI Purchase Order Provenance Verification & MMR Insertion
    let supply_event = SupplyProvenanceEvent {
        po_number: "PO-2026-8890".to_string(),
        document_hash: "a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890"
            .to_string(),
        supplier_id: "sup_acme_corp".to_string(),
        buyer_id: "buy_global_logistics".to_string(),
    };
    let (supply_pos, supply_root) = sync
        .ingest_supply_provenance(supply_event)
        .await
        .expect("Supply provenance ingestion should succeed");
    assert!(supply_root.starts_with("0x"));
    assert!(supply_pos > 0);

    // 4. SME Invoicing: UBL Invoice Escrow Sync
    let escrow_event = SmeEscrowSyncEvent {
        invoice_id: "INV-2026-0042".to_string(),
        ubl_version: "2.1".to_string(),
        escrow_contract_address: "0x71C7656EC7ab88b098defB751B7401B5f6d8976F".to_string(),
        chain_family: "EVM".to_string(),
        escrow_status: "LOCKED".to_string(),
        amount: 50000.0,
        currency: "EUR".to_string(),
        l1_finality_height: 18450123,
    };
    let escrow_commitment = sync
        .sync_sme_escrow(escrow_event)
        .await
        .expect("SME escrow sync should succeed");
    assert!(escrow_commitment.starts_with("0x"));

    // 5. Compliance & KYC: ZK Field Verification (PostalAddress, TownName, Ctry)
    let zk_event = ComplianceZkSanitizedEvent {
        case_id: "kyc_case_5510".to_string(),
        postal_address: "100 Wall Street".to_string(),
        town_name: "New York".to_string(),
        country_code: "USA".to_string(),
        verifier_contract: "0xZKVerifierContractAddress".to_string(),
        sanitized_fields: serde_json::json!({
            "TownName": "New York",
            "Ctry": "USA"
        }),
    };
    let zk_proof = executor
        .verify_compliance_zk_state(zk_event)
        .await
        .expect("Compliance ZK verification should succeed");
    assert!(zk_proof.starts_with("0x"));
}
