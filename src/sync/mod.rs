//! nexus-sync module handles the ingestion of Stacks node events.
//!
//! It distinguishes between microblock soft-finality and burn-block hard-finality
//! to maintain an accurate off-chain representation of the Stacks L1 state.

use serde::{Deserialize, Serialize};
use crate::storage::Storage;
use crate::state::NexusState;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};
use sqlx::Row;

/// Represents the types of events received from a Stacks node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StacksEvent {
    /// Soft-finality block (microblock).
    Microblock(MicroblockData),
    /// Hard-finality block (burn block).
    BurnBlock(BurnBlockData),
}

/// Data payload for a Stacks microblock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroblockData {
    pub hash: String,
    pub height: u64,
    pub parent_hash: String,
    pub txs: Vec<TransactionData>,
    pub timestamp: DateTime<Utc>,
}

/// Data payload for a transaction within a microblock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionData {
    pub tx_id: String,
    pub sender: String,
    pub payload: Option<String>,
}

/// Data payload for a Stacks burn block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnBlockData {
    pub hash: String,
    pub height: u64,
    pub timestamp: DateTime<Utc>,
}

/// The sync service responsible for processing on-chain events.
pub struct NexusSync {
    storage: Arc<Storage>,
    state: Arc<NexusState>,
}

impl NexusSync {
    pub fn new(storage: Arc<Storage>, state: Arc<NexusState>) -> Self {
        Self { storage, state }
    }

    /// Loads initial state from the database.
    pub async fn load_initial_state(&self) -> anyhow::Result<()> {
        tracing::info!("Loading initial state from database...");
        let rows = sqlx::query(
            "SELECT tx_id FROM stacks_transactions ORDER BY created_at ASC"
        )
        .fetch_all(&self.storage.pg_pool)
        .await?;

        let leaves: Vec<String> = rows.into_iter().map(|r| r.get("tx_id")).collect();
        self.state.set_initial_leaves(leaves);
        Ok(())
    }

    /// Starts the sync service, listening for Stacks node events.
    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Starting NexusSync service...");

        // In a real implementation, we'd use tokio_tungstenite to connect to a Stacks node WS.
        // For now, we simulate an event stream.
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

        loop {
            interval.tick().await;
            self.simulate_event().await?;
        }
    }

    async fn simulate_event(&self) -> anyhow::Result<()> {
        let height = Utc::now().timestamp() as u64 / 600; // Mock height
        let hash = format!("0x{:x}", Sha256::digest(format!("block-{}", Utc::now()).as_bytes()));

        if rand::random::<u8>() % 10 == 0 {
             let event = StacksEvent::BurnBlock(BurnBlockData {
                 hash,
                 height,
                 timestamp: Utc::now(),
             });
             self.handle_event(event).await?;
        } else {
             let event = StacksEvent::Microblock(MicroblockData {
                 hash,
                 height,
                 parent_hash: "0x...".to_string(),
                 txs: vec![
                     TransactionData {
                         tx_id: "tx1".to_string(),
                         sender: "SP123".to_string(),
                         payload: Some("payload1".to_string())
                     },
                     TransactionData {
                         tx_id: "tx2".to_string(),
                         sender: "SP456".to_string(),
                         payload: Some("payload2".to_string())
                     },
                 ],
                 timestamp: Utc::now(),
             });
             self.handle_event(event).await?;
        }
        Ok(())
    }

    /// Handles incoming Stacks node events and updates the local state.
    pub async fn handle_event(&self, event: StacksEvent) -> anyhow::Result<()> {
        match event {
            StacksEvent::Microblock(data) => {
                self.process_microblock(data).await?;
            }
            StacksEvent::BurnBlock(data) => {
                self.process_burn_block(data).await?;
            }
        }
        Ok(())
    }

    async fn process_microblock(&self, data: MicroblockData) -> anyhow::Result<()> {
        tracing::debug!("Processing microblock: {} (soft-finality)", data.hash);

        let mut tx = self.storage.pg_pool.begin().await?;

        sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state, created_at) VALUES ($1, $2, 'microblock', 'soft', $3) ON CONFLICT (hash) DO NOTHING"
        )
        .bind(&data.hash)
        .bind(data.height as i64)
        .bind(data.timestamp)
        .execute(&mut *tx).await?;

        for tx_data in &data.txs {
            sqlx::query(
                "INSERT INTO stacks_transactions (tx_id, block_hash, sender, payload, created_at) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (tx_id) DO NOTHING"
            )
            .bind(&tx_data.tx_id)
            .bind(&data.hash)
            .bind(&tx_data.sender)
            .bind(&tx_data.payload)
            .bind(data.timestamp)
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;

        // Update cryptographic state root with Merkle Tree
        let tx_ids: Vec<String> = data.txs.into_iter().map(|t| t.tx_id).collect();
        self.state.update_state_batch(&tx_ids);

        // Invalidate cache on new microblock
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;
        redis::cmd("DEL").arg("cache:vaults:all").query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    async fn process_burn_block(&self, data: BurnBlockData) -> anyhow::Result<()> {
        tracing::info!("Processing burn block: {} (hard-finality)", data.hash);

        let mut tx = self.storage.pg_pool.begin().await?;

        // Update all previous soft blocks to hard state up to this height
        sqlx::query(
            "UPDATE stacks_blocks SET state = 'hard' WHERE height <= $1 AND state = 'soft'"
        )
        .bind(data.height as i64)
        .execute(&mut *tx).await?;

        sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state, created_at) VALUES ($1, $2, 'burn_block', 'hard', $3) ON CONFLICT (hash) DO NOTHING"
        )
        .bind(&data.hash)
        .bind(data.height as i64)
        .bind(data.timestamp)
        .execute(&mut *tx).await?;

        tx.commit().await?;

        // Update state root for burn block
        self.state.update_state(&data.hash, 0);

        Ok(())
    }
}
