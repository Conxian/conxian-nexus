//! [CON-330] Sovereign Transactional SQL (Kwil Pilot).
//! Bridges application state to Kwil's decentralized relational database.

use crate::storage::Storage;
use anyhow::anyhow;
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

        let provider_url =
            std::env::var("KWIL_PROVIDER_URL").context("Missing env var: KWIL_PROVIDER_URL")?;
        let db_id = std::env::var("KWIL_DB_ID").context("Missing env var: KWIL_DB_ID")?;

        Ok(Self {
            provider_url,
            db_id,
        })
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
        let _signature = self.wallet.sign(&payload);

        tracing::warn!(
            db_id = %self._db_id,
            provider = %self._provider_url,
            "Kwil persistence is not implemented"
        );

        Err(anyhow!("Kwil persistence is not implemented"))
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
        let _signature = self.wallet.sign(&payload);

        tracing::warn!(
            db_id = %self._db_id,
            provider = %self._provider_url,
            "Kwil persistence is not implemented"
        );

        Err(anyhow!("Kwil persistence is not implemented"))
    }
}

/// Percent-encodes payload values so canonical payloads are delimiter-safe.
///
/// Encoding rules:
/// - `%` => `%25`
/// - `|` => `%7C`
/// - `=` => `%3D`
fn encode_payload_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '%' => out.push_str("%25"),
            '|' => out.push_str("%7C"),
            '=' => out.push_str("%3D"),
            _ => out.push(ch),
        }
    }

    out
}

pub fn canonical_block_payload(commitment: &KwilBlockCommitment) -> String {
    format!(
        "{}|hash={}|height={}|type={}|state={}",
        "nexus:kwil:block:v1",
        encode_payload_value(&commitment.hash),
        commitment.height,
        encode_payload_value(&commitment.block_type),
        encode_payload_value(&commitment.state)
    )
}

pub fn canonical_state_root_payload(commitment: &KwilStateRootCommitment) -> String {
    format!(
        "{}|block_height={}|state_root={}",
        "nexus:kwil:state_root:v1",
        commitment.block_height,
        encode_payload_value(&commitment.state_root)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_payload_value_escapes_reserved_chars() {
        let encoded = encode_payload_value("a%b|c=d");
        assert_eq!(encoded, "a%25b%7Cc%3Dd");
    }

    #[test]
    fn canonical_payloads_include_domain_and_are_delimiter_safe() {
        let block = KwilBlockCommitment {
            hash: "a%b|c=d".to_string(),
            height: 1,
            block_type: "micro|block".to_string(),
            state: "soft=maybe".to_string(),
        };

        let payload = canonical_block_payload(&block);
        assert!(payload.starts_with("nexus:kwil:block:v1|"));
        assert!(payload.contains("hash=a%25b%7Cc%3Dd"));
        assert!(payload.contains("type=micro%7Cblock"));
        assert!(payload.contains("state=soft%3Dmaybe"));

        let state_root = KwilStateRootCommitment {
            block_height: 2,
            state_root: "root|v1".to_string(),
        };

        let payload = canonical_state_root_payload(&state_root);
        assert!(payload.starts_with("nexus:kwil:state_root:v1|"));
        assert!(payload.contains("state_root=root%7Cv1"));
    }
}
