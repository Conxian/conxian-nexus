//! [CON-330] Sovereign Transactional SQL (Kwil Pilot).
//! Bridges application state to Kwil's decentralized relational database.

use crate::storage::Storage;
use anyhow::{anyhow, Context};
use chrono::Utc;
use lib_conxian_core::Wallet;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilMmrNodeCommitment {
    pub pos: u64,
    pub hash: String,
    pub block_height: u64,
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

#[derive(Debug, Serialize, Deserialize)]
struct KwilExecuteRequest {
    pub db_id: String,
    pub action: String,
    pub params: serde_json::Value,
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct KwilExecuteResponse {
    pub tx_hash: Option<String>,
    pub error: Option<String>,
}

/// [NEXUS-SQL-01] Kwil persistence layer.
pub struct KwilAdapter {
    _storage: Arc<Storage>,
    provider_url: String,
    db_id: String,
    wallet: Arc<Wallet>,
    client: Client,
}

const KWIL_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const KWIL_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

impl KwilAdapter {
    pub fn new(
        storage: Arc<Storage>,
        cfg: KwilConfig,
        wallet: Arc<Wallet>,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .connect_timeout(KWIL_CONNECT_TIMEOUT)
            .timeout(KWIL_REQUEST_TIMEOUT)
            .build()
            .context("Failed to build Kwil HTTP client")?;

        Ok(Self {
            _storage: storage,
            provider_url: cfg.provider_url,
            db_id: cfg.db_id,
            wallet,
            client,
        })
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

        let created_at = Utc::now().to_rfc3339();
        let payload = canonical_block_payload(&commitment, &created_at);
        let signature = self.wallet.sign(&payload);

        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

        let params = serde_json::json!({
            "hash": commitment.hash,
            "height": commitment.height,
            "type": commitment.block_type,
            "state": commitment.state,
            "created_at": created_at,
        });

        let response = self
            .client
            .post(&url)
            .json(&KwilExecuteRequest {
                db_id: self.db_id.clone(),
                action: "insert_block".to_string(),
                params,
                payload: payload.clone(),
                signature: signature.clone(),
            })
            .send()
            .await
            .context("Failed to send request to Kwil")?;

        self.handle_response(response, signature).await
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

        let created_at = Utc::now().to_rfc3339();
        let payload = canonical_state_root_payload(&commitment, &created_at);
        let signature = self.wallet.sign(&payload);

        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

        let params = serde_json::json!({
            "block_height": commitment.block_height,
            "state_root": commitment.state_root,
            "created_at": created_at,
        });

        let response = self
            .client
            .post(&url)
            .json(&KwilExecuteRequest {
                db_id: self.db_id.clone(),
                action: "upsert_state_root".to_string(),
                params,
                payload: payload.clone(),
                signature: signature.clone(),
            })
            .send()
            .await
            .context("Failed to send request to Kwil")?;

        self.handle_response(response, signature).await
    }

    /// [CON-396] Pilot: Persist MMR nodes to Kwil.
    pub async fn persist_mmr_nodes(
        &self,
        nodes: Vec<KwilMmrNodeCommitment>,
    ) -> anyhow::Result<Vec<KwilReceipt>> {
        let mut receipts = Vec::with_capacity(nodes.len());
        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

        for node in nodes {
            let created_at = Utc::now().to_rfc3339();
            let payload = canonical_mmr_node_payload(&node, &created_at);
            let signature = self.wallet.sign(&payload);

            let params = serde_json::json!({
                "pos": node.pos,
                "hash": node.hash,
                "block_height": node.block_height,
                "created_at": created_at,
            });

            let response = self
                .client
                .post(&url)
                .json(&KwilExecuteRequest {
                    db_id: self.db_id.clone(),
                    action: "insert_mmr_node".to_string(),
                    params,
                    payload: payload.clone(),
                    signature: signature.clone(),
                })
                .send()
                .await
                .context("Failed to send MMR node request to Kwil")?;

            receipts.push(self.handle_response(response, signature).await?);
        }

        Ok(receipts)
    }

    async fn handle_response(
        &self,
        response: reqwest::Response,
        signature: String,
    ) -> anyhow::Result<KwilReceipt> {
        let status = response.status();
        let text = response
            .text()
            .await
            .context("Failed to read Kwil response")?;

        if !status.is_success() {
            return Err(anyhow!("Kwil HTTP {}: {}", status, text));
        }

        let result: KwilExecuteResponse =
            serde_json::from_str(&text).context("Failed to parse Kwil response")?;

        if let Some(err) = result.error {
            return Err(anyhow!("Kwil execution error: {}", err));
        }

        let tx_hash = result
            .tx_hash
            .ok_or_else(|| anyhow!("No transaction hash returned from Kwil"))?;

        Ok(KwilReceipt {
            tx_hash,
            payload_signature: signature,
        })
    }
}

/// Percent-encodes payload values so canonical payloads are delimiter-safe.
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

pub fn canonical_block_payload(commitment: &KwilBlockCommitment, created_at: &str) -> String {
    format!(
        "{}|hash={}|height={}|type={}|state={}|created_at={}",
        "nexus:kwil:block:v1",
        encode_payload_value(&commitment.hash),
        commitment.height,
        encode_payload_value(&commitment.block_type),
        encode_payload_value(&commitment.state),
        encode_payload_value(created_at)
    )
}

pub fn canonical_state_root_payload(
    commitment: &KwilStateRootCommitment,
    created_at: &str,
) -> String {
    format!(
        "{}|block_height={}|state_root={}|created_at={}",
        "nexus:kwil:state_root:v1",
        commitment.block_height,
        encode_payload_value(&commitment.state_root),
        encode_payload_value(created_at)
    )
}

pub fn canonical_mmr_node_payload(node: &KwilMmrNodeCommitment, created_at: &str) -> String {
    format!(
        "{}|pos={}|hash={}|block_height={}|created_at={}",
        "nexus:kwil:mmr_node:v1",
        node.pos,
        encode_payload_value(&node.hash),
        node.block_height,
        encode_payload_value(created_at)
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
        let created_at = "2024-05-28T12:00:00Z";
        let block = KwilBlockCommitment {
            hash: "a%b|c=d".to_string(),
            height: 1,
            block_type: "micro|block".to_string(),
            state: "soft=maybe".to_string(),
        };

        let payload = canonical_block_payload(&block, created_at);
        assert!(payload.starts_with("nexus:kwil:block:v1|"));
        assert!(payload.contains("hash=a%25b%7Cc%3Dd"));
        assert!(payload.contains("type=micro%7Cblock"));
        assert!(payload.contains("state=soft%3Dmaybe"));
        assert!(payload.contains("created_at=2024-05-28T12:00:00Z"));

        let state_root = KwilStateRootCommitment {
            block_height: 2,
            state_root: "root|v1".to_string(),
        };

        let payload = canonical_state_root_payload(&state_root, created_at);
        assert!(payload.starts_with("nexus:kwil:state_root:v1|"));
        assert!(payload.contains("state_root=root%7Cv1"));
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilSettlementProposalCommitment {
    pub proposal_id: String,
    pub external_id: String,
    pub source: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub init_height: i64,
    pub unlock_height: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilSettlementLogCommitment {
    pub external_tx_reference: String,
    pub settlement_network_origin: String,
    pub fiat_value_pegged: Option<f64>,
    pub raw_payload: serde_json::Value,
}

impl KwilAdapter {
    /// [CON-162] Pilot: Persist settlement proposal to Kwil.
    pub async fn persist_settlement_proposal(
        &self,
        proposal: KwilSettlementProposalCommitment,
    ) -> anyhow::Result<KwilReceipt> {
        let created_at = Utc::now().to_rfc3339();
        let payload = format!(
            "nexus:kwil:settlement_proposal:v1|proposal_id={}|external_id={}|source={}|status={}|init_height={}|unlock_height={}|created_at={}",
            encode_payload_value(&proposal.proposal_id),
            encode_payload_value(&proposal.external_id),
            encode_payload_value(&proposal.source),
            encode_payload_value(&proposal.status),
            proposal.init_height,
            proposal.unlock_height,
            encode_payload_value(&created_at)
        );
        let signature = self.wallet.sign(&payload);

        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

        let params = serde_json::json!({
            "proposal_id": proposal.proposal_id,
            "external_id": proposal.external_id,
            "source": proposal.source,
            "payload": proposal.payload,
            "status": proposal.status,
            "init_height": proposal.init_height,
            "unlock_height": proposal.unlock_height,
            "created_at": created_at,
        });

        let response = self
            .client
            .post(&url)
            .json(&KwilExecuteRequest {
                db_id: self.db_id.clone(),
                action: "insert_settlement_proposal".to_string(),
                params,
                payload: payload.clone(),
                signature: signature.clone(),
            })
            .send()
            .await
            .context("Failed to send settlement proposal request to Kwil")?;

        self.handle_response(response, signature).await
    }

    /// [CON-164] Pilot: Persist settlement log to Kwil.
    pub async fn persist_settlement_log(
        &self,
        log: KwilSettlementLogCommitment,
    ) -> anyhow::Result<KwilReceipt> {
        let created_at = Utc::now().to_rfc3339();
        let payload = format!(
            "nexus:kwil:settlement_log:v1|external_tx_reference={}|source={}|fiat_value={:?}|created_at={}",
            encode_payload_value(&log.external_tx_reference),
            encode_payload_value(&log.settlement_network_origin),
            log.fiat_value_pegged,
            encode_payload_value(&created_at)
        );
        let signature = self.wallet.sign(&payload);

        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

        let params = serde_json::json!({
            "external_tx_reference": log.external_tx_reference,
            "settlement_network_origin": log.settlement_network_origin,
            "fiat_value_pegged": log.fiat_value_pegged,
            "raw_payload": log.raw_payload,
            "created_at": created_at,
        });

        let response = self
            .client
            .post(&url)
            .json(&KwilExecuteRequest {
                db_id: self.db_id.clone(),
                action: "insert_settlement_log".to_string(),
                params,
                payload: payload.clone(),
                signature: signature.clone(),
            })
            .send()
            .await
            .context("Failed to send settlement log request to Kwil")?;

        self.handle_response(response, signature).await
    }
}
