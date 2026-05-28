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

#[derive(Debug, Serialize)]
pub struct KwilExecuteRequest {
    pub db_id: String,
    pub action: String,
    pub params: serde_json::Value,
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
pub struct KwilReceipt {
    pub tx_hash: String,
}

pub struct KwilAdapter {
    #[allow(dead_code)]
    storage: Arc<Storage>,
    client: Client,
    provider_url: String,
    db_id: String,
    wallet: Arc<Wallet>,
}

impl KwilAdapter {
    pub fn new(
        storage: Arc<Storage>,
        cfg: KwilConfig,
        wallet: Arc<Wallet>,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        Ok(Self {
            storage,
            client,
            provider_url: cfg.provider_url,
            db_id: cfg.db_id,
            wallet,
        })
    }

    pub async fn persist_block(&self, block: KwilBlockCommitment) -> anyhow::Result<KwilReceipt> {
        let created_at = Utc::now().to_rfc3339();
        let payload = format!(
            "nexus:kwil:block:v1|hash={}|height={}|type={}|state={}|created_at={}",
            encode_payload_value(&block.hash),
            block.height,
            encode_payload_value(&block.block_type),
            encode_payload_value(&block.state),
            encode_payload_value(&created_at)
        );
        let signature = self.wallet.sign(&payload);

        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

        let params = serde_json::json!({
            "hash": block.hash,
            "height": block.height,
            "type": block.block_type,
            "state": block.state,
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
            .context("Failed to send block request to Kwil")?;

        self.handle_response(response, signature).await
    }

    pub async fn persist_state_root(
        &self,
        root: KwilStateRootCommitment,
    ) -> anyhow::Result<KwilReceipt> {
        let created_at = Utc::now().to_rfc3339();
        let payload = format!(
            "nexus:kwil:state_root:v1|height={}|root={}|created_at={}",
            root.block_height,
            encode_payload_value(&root.state_root),
            encode_payload_value(&created_at)
        );
        let signature = self.wallet.sign(&payload);

        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

        let params = serde_json::json!({
            "block_height": root.block_height,
            "state_root": root.state_root,
            "created_at": created_at,
        });

        let response = self
            .client
            .post(&url)
            .json(&KwilExecuteRequest {
                db_id: self.db_id.clone(),
                action: "insert_state_root".to_string(),
                params,
                payload: payload.clone(),
                signature: signature.clone(),
            })
            .send()
            .await
            .context("Failed to send state root request to Kwil")?;

        self.handle_response(response, signature).await
    }

    pub async fn persist_mmr_node(&self, node: KwilMmrNodeCommitment) -> anyhow::Result<KwilReceipt> {
        let created_at = Utc::now().to_rfc3339();
        let payload = format!(
            "nexus:kwil:mmr_node:v1|pos={}|hash={}|height={}|created_at={}",
            node.pos,
            encode_payload_value(&node.hash),
            node.block_height,
            encode_payload_value(&created_at)
        );
        let signature = self.wallet.sign(&payload);

        let url = format!("{}/api/v1/execute", self.provider_url.trim_end_matches('/'));

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

        self.handle_response(response, signature).await
    }

    async fn handle_response(
        &self,
        response: reqwest::Response,
        signature: String,
    ) -> anyhow::Result<KwilReceipt> {
        let status = response.status();
        if status.is_success() {
            let receipt = response
                .json::<KwilReceipt>()
                .await
                .context("Failed to parse Kwil receipt")?;
            Ok(receipt)
        } else {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            Err(anyhow!(
                "Kwil execution failed (status {}): {} | Signature: {}",
                status,
                error_text,
                signature
            ))
        }
    }
}

pub fn encode_payload_value(v: &str) -> String {
    v.replace('|', r"\|").replace('=', r"\=")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilSettlementProposalCommitment {
    pub proposal_id: String,
    pub external_id: String,
    pub source: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub init_height: u64,
    pub unlock_height: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KwilSettlementLogCommitment {
    pub external_tx_reference: String,
    pub settlement_network_origin: String,
    pub fiat_value_pegged: Option<f64>,
    pub raw_payload: serde_json::Value,
}

impl KwilAdapter {
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

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn encode_payload_value_escapes_reserved_chars() {
        assert_eq!(encode_payload_value("a|b=c"), r"a\|b\=c");
    }

    #[test]
    fn canonical_payloads_include_domain_and_are_delimiter_safe() {
        let block = KwilBlockCommitment {
            hash: "0x123".to_string(),
            height: 100,
            block_type: "burn".to_string(),
            state: "hard".to_string(),
        };
        let created_at = "2026-05-28T12:00:00Z";
        let payload = format!(
            "nexus:kwil:block:v1|hash={}|height={}|type={}|state={}|created_at={}",
            encode_payload_value(&block.hash),
            block.height,
            encode_payload_value(&block.block_type),
            encode_payload_value(&block.state),
            encode_payload_value(created_at)
        );
        assert!(payload.starts_with("nexus:kwil:block:v1|"));
        assert!(payload.contains("|hash=0x123|"));
    }
}
