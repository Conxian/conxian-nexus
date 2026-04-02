use conxian_nexus::storage::kwil::{KwilBlockCommitment, KwilStateRootCommitment};
use lib_conxian_core::Wallet;
use tokio;

#[tokio::test]
async fn test_kwil_block_persistence_pilot_signed() {
    // Manual setup for logic verification without live Storage/DB
    let adapter_mock = KwilAdapterMock {
        wallet: Wallet::new(),
    };

    let commitment = KwilBlockCommitment {
        hash: "0xabc123".to_string(),
        height: 1000,
        block_type: "microblock".to_string(),
        state: "soft".to_string(),
    };

    let result = adapter_mock.persist_block(commitment).await;
    assert!(result.is_ok());
    let tx_hash = result.unwrap();
    assert!(tx_hash.starts_with("kwil_tx_"));
    // Signature should be 128 hex chars (64 bytes for Secp256k1)
    assert_eq!(tx_hash.len(), 8 + 128);
}

#[tokio::test]
async fn test_kwil_state_root_persistence_pilot_signed() {
    let adapter_mock = KwilAdapterMock {
        wallet: Wallet::new(),
    };

    let commitment = KwilStateRootCommitment {
        block_height: 1000,
        state_root: "0xroot123".to_string(),
    };

    let result = adapter_mock.persist_state_root(commitment).await;
    assert!(result.is_ok());
    let tx_hash = result.unwrap();
    assert!(tx_hash.starts_with("kwil_tx_"));
    assert_eq!(tx_hash.len(), 8 + 128);
}

struct KwilAdapterMock {
    wallet: Wallet,
}

impl KwilAdapterMock {
    async fn persist_block(&self, commitment: KwilBlockCommitment) -> anyhow::Result<String> {
        let payload = serde_json::to_string(&commitment)?;
        let signature = self.wallet.sign(&payload);
        Ok(format!("kwil_tx_{}", signature))
    }

    async fn persist_state_root(&self, commitment: KwilStateRootCommitment) -> anyhow::Result<String> {
        let payload = serde_json::to_string(&commitment)?;
        let signature = self.wallet.sign(&payload);
        Ok(format!("kwil_tx_{}", signature))
    }
}
