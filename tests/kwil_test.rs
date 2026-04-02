use conxian_nexus::storage::kwil::{
    canonical_block_payload, canonical_state_root_payload, KwilBlockCommitment, KwilReceipt,
    KwilStateRootCommitment,
};
use lib_conxian_core::Wallet;
use std::sync::Arc;

#[tokio::test]
async fn test_kwil_block_persistence_pilot_signed() {
    // Manual setup for logic verification without live Storage/DB
    let adapter_mock = KwilAdapterMock {
        wallet: Arc::new(Wallet::new()),
    };

    let commitment = KwilBlockCommitment {
        hash: "0xabc123".to_string(),
        height: 1000,
        block_type: "microblock".to_string(),
        state: "soft".to_string(),
    };

    let receipt = adapter_mock.persist_block(commitment).await?;
    assert_eq!(receipt.tx_hash, "kwil_tx_stub");
    assert!(!receipt.payload_signature.is_empty());
    assert!(receipt
        .payload_signature
        .chars()
        .all(|c| c.is_ascii_hexdigit()));

    Ok(())
}

#[tokio::test]
async fn test_kwil_state_root_persistence_pilot_signed() {
    let adapter_mock = KwilAdapterMock {
        wallet: Arc::new(Wallet::new()),
    };

    let commitment = KwilStateRootCommitment {
        block_height: 1000,
        state_root: "0xroot123".to_string(),
    };

    let receipt = adapter_mock.persist_state_root(commitment).await?;
    assert_eq!(receipt.tx_hash, "kwil_tx_stub");
    assert!(!receipt.payload_signature.is_empty());
    assert!(receipt
        .payload_signature
        .chars()
        .all(|c| c.is_ascii_hexdigit()));

    Ok(())
}

struct KwilAdapterMock {
    wallet: Arc<Wallet>,
}

impl KwilAdapterMock {
    async fn persist_block(&self, commitment: KwilBlockCommitment) -> anyhow::Result<KwilReceipt> {
        let payload = canonical_block_payload(&commitment);
        let signature = self.wallet.sign(&payload);
        Ok(KwilReceipt {
            tx_hash: "kwil_tx_stub".to_string(),
            payload_signature: signature,
        })
    }

    async fn persist_state_root(
        &self,
        commitment: KwilStateRootCommitment,
    ) -> anyhow::Result<KwilReceipt> {
        let payload = canonical_state_root_payload(&commitment);
        let signature = self.wallet.sign(&payload);
        Ok(KwilReceipt {
            tx_hash: "kwil_tx_stub".to_string(),
            payload_signature: signature,
        })
    }
}
