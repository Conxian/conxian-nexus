//! nexus-sync module handles the ingestion of Stacks node events.
//!
//! It distinguishes between microblock soft-finality and burn-block hard-finality
//! to maintain an accurate off-chain representation of the Stacks L1 state.

use serde::{Deserialize, Serialize};
use crate::storage::Storage;
use std::sync::Arc;

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
}

/// Data payload for a Stacks burn block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnBlockData {
    pub hash: String,
    pub height: u64,
}

/// The sync service responsible for processing on-chain events.
pub struct NexusSync {
    storage: Arc<Storage>,
}

impl NexusSync {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
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

        sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state) VALUES ($1, $2, 'microblock', 'soft') ON CONFLICT (hash) DO NOTHING"
        )
        .bind(&data.hash)
        .bind(data.height as i64)
        .execute(&self.storage.pg_pool).await?;

        let mut conn = self.storage.redis_client.get_async_connection().await?;
        redis::cmd("DEL").arg("cache:vaults:all").query_async::<_, ()>(&mut conn).await?;

        Ok(())
    }

    async fn process_burn_block(&self, data: BurnBlockData) -> anyhow::Result<()> {
        tracing::info!("Processing burn block: {} (hard-finality)", data.hash);

        sqlx::query(
            "UPDATE stacks_blocks SET state = 'hard' WHERE height <= $1 AND state = 'soft'"
        )
        .bind(data.height as i64)
        .execute(&self.storage.pg_pool).await?;

        sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state) VALUES ($1, $2, 'burn_block', 'hard') ON CONFLICT (hash) DO NOTHING"
        )
        .bind(&data.hash)
        .bind(data.height as i64)
        .execute(&self.storage.pg_pool).await?;

        Ok(())
    }
}
