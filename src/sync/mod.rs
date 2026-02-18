//! nexus-sync module handles the ingestion of Stacks node events.
//!
//! It distinguishes between microblock soft-finality and burn-block hard-finality
//! to maintain an accurate off-chain representation of the Stacks L1 state.

use serde::{Deserialize, Serialize};
use crate::storage::Storage;
use crate::state::NexusState;
use std::sync::Arc;
use chrono::{DateTime, Utc};

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
    pub txs: Vec<String>,
    pub timestamp: DateTime<Utc>,
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
                 txs: vec!["tx1".to_string(), "tx2".to_string()],
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

        for tx_id in &data.txs {
            sqlx::query(
                "INSERT INTO stacks_transactions (tx_id, block_hash, created_at) VALUES ($1, $2, $3) ON CONFLICT (tx_id) DO NOTHING"
            )
            .bind(tx_id)
            .bind(&data.hash)
            .bind(data.timestamp)
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;

        // Update cryptographic state root with Merkle Tree
        for tx_id in &data.txs {
            self.state.update_state(tx_id, 1);
        }

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

use sha2::{Sha256, Digest};
