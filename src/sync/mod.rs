use sha2::Digest;
pub mod bip110;

use crate::state::NexusState;
use crate::storage::kwil::{KwilAdapter, KwilMmrNodeCommitment};
use crate::storage::tableland::TablelandAdapter;
use crate::storage::Storage;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_tungstenite::connect_async;

#[derive(Debug, Serialize, Deserialize)]
pub struct BurnBlockData {
    pub hash: String,
    pub height: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MicroblockData {
    pub hash: String,
    pub height: u64,
    pub parent_hash: String,
    pub tx_ids: Vec<String>,
}

pub struct NexusSync {
    pub storage: Arc<Storage>,
    pub state_tracker: Arc<NexusState>,
    pub tableland: Arc<TablelandAdapter>,
    pub kwil: Option<Arc<KwilAdapter>>,
    pub rpc_url: String,
    pub ws_url: String,
}

impl NexusSync {
    pub fn new(
        storage: Arc<Storage>,
        state_tracker: Arc<NexusState>,
        tableland: Arc<TablelandAdapter>,
        kwil: Option<Arc<KwilAdapter>>,
        rpc_url: String,
        ws_url: String,
    ) -> Self {
        Self {
            storage,
            state_tracker,
            tableland,
            kwil,
            rpc_url,
            ws_url,
        }
    }

    pub async fn load_initial_state(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let url_str = self.ws_url.clone();
        let (ws_stream, _) = connect_async(&url_str).await?;
        let (mut _write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            let msg = msg?;
            if msg.is_text() {
                // Handle message
            }
        }
        Ok(())
    }

    pub async fn process_microblock(&self, data: MicroblockData) -> anyhow::Result<()> {
        let added_nodes = self.state_tracker.update_state_batch(&data.tx_ids);
        let root = self.state_tracker.get_state_root();

        self.persist_root_to_redis(&root).await?;

        if let Some(kwil) = &self.kwil {
            let mmr_commitments: Vec<KwilMmrNodeCommitment> = added_nodes
                .iter()
                .map(|(pos, hash)| KwilMmrNodeCommitment {
                    pos: *pos,
                    hash: hex::encode(hash),
                    block_height: data.height,
                })
                .collect();

            for node in mmr_commitments {
                let _ = kwil.persist_mmr_node(node).await;
            }
        }
        Ok(())
    }

    pub async fn persist_root_to_redis(&self, root: &str) -> anyhow::Result<()> {
        let mut conn = self
            .storage
            .redis_client
            .get_multiplexed_async_connection()
            .await?;
        let _: () = redis::cmd("SET")
            .arg("nexus:state_root")
            .arg(root)
            .query_async::<()>(&mut conn)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosSettlementEvent {
    pub commitment_id: String,
    pub merchant_id: String,
    pub terminal_id: String,
    pub transaction_count: u64,
    pub total_amount: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyProvenanceEvent {
    pub po_number: String,
    pub document_hash: String,
    pub supplier_id: String,
    pub buyer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmeEscrowSyncEvent {
    pub invoice_id: String,
    pub ubl_version: String,
    pub escrow_contract_address: String,
    pub chain_family: String,
    pub escrow_status: String,
    pub amount: f64,
    pub currency: String,
    pub l1_finality_height: u64,
}

impl NexusSync {
    pub async fn ingest_pos_settlement(
        &self,
        event: PosSettlementEvent,
    ) -> anyhow::Result<(u64, String)> {
        let payload = format!(
            "pos:{}:{}:{}:{}:{}",
            event.commitment_id,
            event.merchant_id,
            event.terminal_id,
            event.total_amount,
            event.currency
        );
        let added_nodes = self.state_tracker.update_state_batch(&[payload]);
        let root = self.state_tracker.get_state_root();
        self.persist_root_to_redis(&root).await?;

        let pos = added_nodes.first().map(|(p, _)| *p).unwrap_or(0);

        sqlx::query(
            "INSERT INTO enterprise_pos_settlement_commitments
             (commitment_id, merchant_id, terminal_id, transaction_count, total_amount, currency, mmr_pos, mmr_root)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (commitment_id) DO UPDATE SET mmr_root = EXCLUDED.mmr_root"
        )
        .bind(&event.commitment_id)
        .bind(&event.merchant_id)
        .bind(&event.terminal_id)
        .bind(event.transaction_count as i64)
        .bind(event.total_amount)
        .bind(&event.currency)
        .bind(pos as i64)
        .bind(&root)
        .execute(&self.storage.pg_pool)
        .await?;

        Ok((pos, root))
    }

    pub async fn ingest_supply_provenance(
        &self,
        event: SupplyProvenanceEvent,
    ) -> anyhow::Result<(u64, String)> {
        let payload = format!(
            "supply:{}:{}:{}",
            event.po_number, event.document_hash, event.supplier_id
        );
        let added_nodes = self.state_tracker.update_state_batch(&[payload]);
        let root = self.state_tracker.get_state_root();
        self.persist_root_to_redis(&root).await?;

        let pos = added_nodes.first().map(|(p, _)| *p).unwrap_or(0);

        sqlx::query(
            "INSERT INTO enterprise_supply_provenance_hashes
             (po_number, document_hash, supplier_id, buyer_id, mmr_pos, mmr_root)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (po_number) DO UPDATE SET mmr_root = EXCLUDED.mmr_root",
        )
        .bind(&event.po_number)
        .bind(&event.document_hash)
        .bind(&event.supplier_id)
        .bind(&event.buyer_id)
        .bind(pos as i64)
        .bind(&root)
        .execute(&self.storage.pg_pool)
        .await?;

        Ok((pos, root))
    }

    pub async fn sync_sme_escrow(&self, event: SmeEscrowSyncEvent) -> anyhow::Result<String> {
        let state_commitment = format!(
            "0x{}",
            hex::encode(sha2::Sha256::digest(
                format!(
                    "{}:{}:{}",
                    event.invoice_id, event.escrow_status, event.l1_finality_height
                )
                .as_bytes()
            ))
        );

        sqlx::query(
            "INSERT INTO enterprise_sme_escrow_sync
             (invoice_id, ubl_version, escrow_contract_address, chain_family, escrow_status, amount, currency, l1_finality_height, offchain_state_commitment)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (invoice_id) DO UPDATE SET
                 escrow_status = EXCLUDED.escrow_status,
                 l1_finality_height = EXCLUDED.l1_finality_height,
                 offchain_state_commitment = EXCLUDED.offchain_state_commitment"
        )
        .bind(&event.invoice_id)
        .bind(&event.ubl_version)
        .bind(&event.escrow_contract_address)
        .bind(&event.chain_family)
        .bind(&event.escrow_status)
        .bind(event.amount)
        .bind(&event.currency)
        .bind(event.l1_finality_height as i64)
        .bind(&state_commitment)
        .execute(&self.storage.pg_pool)
        .await?;

        Ok(state_commitment)
    }
}
