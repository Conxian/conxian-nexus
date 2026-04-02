use conxian_nexus::storage::kwil::{
    KwilAdapter, KwilBlockCommitment, KwilConfig, KwilStateRootCommitment,
};
use conxian_nexus::storage::Storage;
use lib_conxian_core::Wallet;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn make_test_storage() -> Arc<Storage> {
    let pg_pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost/nexus")
        .expect("connect_lazy should not require a live DB");

    let redis_client = redis::Client::open("redis://127.0.0.1/")
        .expect("redis client construction should not require a live server");

    Arc::new(Storage {
        pg_pool,
        redis_client,
    })
}

fn make_test_cfg() -> KwilConfig {
    KwilConfig {
        provider_url: "http://127.0.0.1:0".to_string(),
        db_id: "nexus_pilot".to_string(),
    }
}

#[tokio::test]
async fn test_kwil_block_persistence_pilot_signed() -> anyhow::Result<()> {
    let storage = make_test_storage();
    let adapter = KwilAdapter::new(storage, make_test_cfg(), Arc::new(Wallet::new()));

    let commitment = KwilBlockCommitment {
        hash: "0xabc123".to_string(),
        height: 1000,
        block_type: "microblock".to_string(),
        state: "soft".to_string(),
    };

    let receipt = adapter.persist_block(commitment).await?;
    assert!(receipt.tx_hash.starts_with("kwil_tx_stub_"));

    let stub_digest = receipt
        .tx_hash
        .strip_prefix("kwil_tx_stub_")
        .expect("stub prefix");
    assert_eq!(stub_digest.len(), 64);
    assert!(is_hex(stub_digest));
    assert!(is_hex(&receipt.payload_signature));

    Ok(())
}

#[tokio::test]
async fn test_kwil_state_root_persistence_pilot_signed() -> anyhow::Result<()> {
    let storage = make_test_storage();
    let adapter = KwilAdapter::new(storage, make_test_cfg(), Arc::new(Wallet::new()));

    let commitment = KwilStateRootCommitment {
        block_height: 1000,
        state_root: "0xroot123".to_string(),
    };

    let receipt = adapter.persist_state_root(commitment).await?;
    assert!(receipt.tx_hash.starts_with("kwil_tx_stub_"));

    let stub_digest = receipt
        .tx_hash
        .strip_prefix("kwil_tx_stub_")
        .expect("stub prefix");
    assert_eq!(stub_digest.len(), 64);
    assert!(is_hex(stub_digest));
    assert!(is_hex(&receipt.payload_signature));

    Ok(())
}
