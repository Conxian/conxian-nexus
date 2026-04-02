//! [CON-330] Sovereign Transactional SQL (Kwil Pilot).
//! Bridges application state to Kwil's decentralized relational database.

use crate::storage::Storage;
use lib_conxian_core::Wallet;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilBlockCommitment {
    pub hash: String,
    pub height: u64,
    pub block_type: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilStateRootCommitment {
    pub block_height: u64,
    pub state_root: String,
}

#[derive(Debug, Clone)]
pub struct KwilConfig {
    pub provider_url: String,
    pub db_id: String,
}

impl KwilConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        use anyhow::Context;

        let provider_url = std::env::var("KWIL_PROVIDER_URL")
            .context("Missing env var: KWIL_PROVIDER_URL")?;
        let db_id = std::env::var("KWIL_DB_ID").context("Missing env var: KWIL_DB_ID")?;

        Ok(Self { provider_url, db_id })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilReceipt {
    pub tx_hash: String,
    pub payload_signature: String,
}

/// [NEXUS-SQL-01] Kwil persistence layer.
pub struct KwilAdapter {
    _storage: Arc<Storage>,
    _provider_url: String,
    _db_id: String,
    wallet: Arc<Wallet>,
}

impl KwilAdapter {
    pub fn new(storage: Arc<Storage>, cfg: KwilConfig, wallet: Arc<Wallet>) -> Self {
        Self {
            _storage: storage,
            _provider_url: cfg.provider_url,
            _db_id: cfg.db_id,
            wallet,
        }
    }

    /// Pilot: Persist block to Kwil with cryptographic signature.
    pub async fn persist_block(
        &self,
        commitment: KwilBlockCommitment,
    ) -> anyhow::Result<KwilReceipt> {
        tracing::info!(
            "Pilot: Committing block to Kwil: {} at height {}",
            commitment.hash,
            commitment.height
        );

        let payload = canonical_block_payload(&commitment);
        let signature = self.wallet.sign(&payload);

        // [STUB] Implement Kwil gRPC/REST call: insert_block action.
        // The signature ensures that the action is authenticated by the Nexus identity.

        let tx_hash = "kwil_tx_stub".to_string();
        tracing::debug!(tx_hash = %tx_hash, "Kwil action 'insert_block' broadcasted");

        Ok(KwilReceipt {
            tx_hash,
            payload_signature: signature,
        })
    }

    /// Pilot: Persist state root to Kwil with cryptographic signature.
    pub async fn persist_state_root(
        &self,
        commitment: KwilStateRootCommitment,
    ) -> anyhow::Result<KwilReceipt> {
        tracing::info!(
            "Pilot: Committing state root to Kwil for height {}",
            commitment.block_height
        );

        let payload = canonical_state_root_payload(&commitment);
        let signature = self.wallet.sign(&payload);

        // [STUB] Implement Kwil gRPC/REST call: upsert_state_root action.

        let tx_hash = "kwil_tx_stub".to_string();
        tracing::debug!(tx_hash = %tx_hash, "Kwil action 'upsert_state_root' broadcasted");

        Ok(KwilReceipt {
            tx_hash,
            payload_signature: signature,
        })
    }
}

fn encode_payload_value(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('|', "%7C")
        .replace('=', "%3D")
}

pub fn canonical_block_payload(commitment: &KwilBlockCommitment) -> String {
    format!(
        "v1|hash={}|height={}|type={}|state={}",
        encode_payload_value(&commitment.hash),
        commitment.height,
        encode_payload_value(&commitment.block_type),
        encode_payload_value(&commitment.state)
    )
}

pub fn canonical_state_root_payload(commitment: &KwilStateRootCommitment) -> String {
    format!(
        "v1|block_height={}|state_root={}",
        commitment.block_height,
        encode_payload_value(&commitment.state_root)
    )
}
