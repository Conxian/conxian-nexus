use conxian_nexus::storage::kwil::{
    canonical_block_payload, canonical_state_root_payload, KwilAdapter, KwilBlockCommitment,
    KwilConfig, KwilStateRootCommitment,
};
use conxian_nexus::storage::Storage;
use lib_conxian_core::Wallet;
use std::sync::Arc;

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn make_test_cfg() -> KwilConfig {
    KwilConfig {
        provider_url: "http://127.0.0.1:0".to_string(), // Invalid port to trigger error or we can use a mock
        db_id: "nexus_pilot".to_string(),
    }
}

fn make_test_storage() -> anyhow::Result<Arc<Storage>> {
    Ok(Arc::new(Storage::new_lazy(
        "postgres://localhost/nexus",
        "redis://127.0.0.1/",
    )?))
}

#[tokio::test]
async fn test_kwil_block_persistence_pilot_signed() -> anyhow::Result<()> {
    let storage = make_test_storage()?;
    let wallet = Arc::new(Wallet::new()?);
    let adapter = KwilAdapter::new(storage, make_test_cfg(), wallet.clone());

    let commitment = KwilBlockCommitment {
        hash: "0xabc123".to_string(),
        height: 1000,
        block_type: "microblock".to_string(),
        state: "soft".to_string(),
    };

    let payload = canonical_block_payload(&commitment);
    let signature = wallet.sign(&payload);
    assert!(is_hex(&signature));
    assert_eq!(signature.len(), 128);

    // Since we don't have a live Kwil node, we expect a connection error
    let err = adapter
        .persist_block(commitment)
        .await
        .expect_err("expected failure due to missing Kwil node");

    let err_msg = err.to_string().to_ascii_lowercase();
    assert!(err_msg.contains("failed to send request") || err_msg.contains("kwil execution error") || err_msg.contains("connect"));

    Ok(())
}

#[tokio::test]
async fn test_kwil_state_root_persistence_pilot_signed() -> anyhow::Result<()> {
    let storage = make_test_storage()?;
    let wallet = Arc::new(Wallet::new()?);
    let adapter = KwilAdapter::new(storage, make_test_cfg(), wallet.clone());

    let commitment = KwilStateRootCommitment {
        block_height: 1000,
        state_root: "0xroot123".to_string(),
    };

    let payload = canonical_state_root_payload(&commitment);
    let signature = wallet.sign(&payload);
    assert!(is_hex(&signature));
    assert_eq!(signature.len(), 128);

    // Since we don't have a live Kwil node, we expect a connection error
    let err = adapter
        .persist_state_root(commitment)
        .await
        .expect_err("expected failure due to missing Kwil node");

    let err_msg = err.to_string().to_ascii_lowercase();
    assert!(err_msg.contains("failed to send request") || err_msg.contains("kwil execution error") || err_msg.contains("connect"));

    Ok(())
}

#[test]
fn canonical_block_payload_escapes_reserved_chars() {
    let commitment = KwilBlockCommitment {
        hash: "0x|=%".into(),
        height: 1,
        block_type: "micro|block".into(),
        state: "so=ft".into(),
    };

    let payload = canonical_block_payload(&commitment);
    assert_eq!(
        payload,
        "nexus:kwil:block:v1|hash=0x%7C%3D%25|height=1|type=micro%7Cblock|state=so%3Dft"
    );
}

#[test]
fn canonical_state_root_payload_escapes_reserved_chars() {
    let commitment = KwilStateRootCommitment {
        block_height: 42,
        state_root: "0xroot|=%".into(),
    };

    let payload = canonical_state_root_payload(&commitment);
    assert_eq!(
        payload,
        "nexus:kwil:state_root:v1|block_height=42|state_root=0xroot%7C%3D%25"
    );
}
