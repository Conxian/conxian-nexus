//! nexus-sync module handles the ingestion of Stacks node events.
//!
//! It distinguishes between microblock soft-finality and burn-block hard-finality
//! to maintain an accurate off-chain representation of the Stacks L1 state.

use crate::state::NexusState;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
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
        let rows = sqlx::query("SELECT tx_id FROM stacks_transactions ORDER BY created_at ASC")
            .fetch_all(&self.storage.pg_pool)
            .await?;

        let leaves: Vec<String> = rows.into_iter().map(|r| r.get("tx_id")).collect();
        let count = leaves.len();
        self.state.set_initial_leaves(leaves);

        let root = self.state.get_state_root();
        tracing::info!("State rebuilt: {} leaves, root: {}", count, root);

        // Persist the verified root to Redis for fast health checks
        self.persist_root_to_redis(&root).await.ok();

        Ok(())
    }

    async fn persist_root_to_redis(&self, root: &str) -> anyhow::Result<()> {
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;
        redis::cmd("SET")
            .arg("nexus:state_root")
            .arg(root)
            .query_async::<_, ()>(&mut conn)
            .await?;
        Ok(())
    }

    /// Starts the sync service, listening for Stacks node events.
    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Starting NexusSync service...");

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        // Spawn simulation task
        let simulator_tx = tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                if let Err(e) = Self::generate_simulated_event(&simulator_tx).await {
                    tracing::error!("Event simulation error: {}", e);
                }
            }
        });

        while let Some(event) = rx.recv().await {
            if let Err(e) = self.handle_event(event).await {
                tracing::error!("Error handling event: {}", e);
            }
        }

        Ok(())
    }

    async fn generate_simulated_event(tx: &tokio::sync::mpsc::Sender<StacksEvent>) -> anyhow::Result<()> {
        let height = Utc::now().timestamp() as u64 / 600; // Mock height
        let hash = format!(
            "0x{:x}",
            Sha256::digest(format!("block-{}", Utc::now()).as_bytes())
        );

        let event = if rand::random::<u8>() % 10 == 0 {
            StacksEvent::BurnBlock(BurnBlockData {
                hash,
                height,
                timestamp: Utc::now(),
            })
        } else {
            StacksEvent::Microblock(MicroblockData {
                hash,
                height,
                parent_hash: "0x...".to_string(),
                txs: vec![
                    TransactionData {
                        tx_id: format!("tx-{}", rand::random::<u32>()),
                        sender: "SP123".to_string(),
                        payload: Some("payload".to_string()),
                    },
                ],
                timestamp: Utc::now(),
            })
        };

        tx.send(event).await?;
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
        let tx_ids: Vec<String> = data.txs.iter().map(|t| t.tx_id.clone()).collect();
        self.state.update_state_batch(&tx_ids);

        // Persist new root
        let root = self.state.get_state_root();
        self.persist_root_to_redis(&root).await.ok();

        // Invalidate cache on new microblock
        if let Ok(mut conn) = self.storage.redis_client.get_multiplexed_async_connection().await {
            redis::cmd("DEL")
                .arg("cache:vaults:all")
                .query_async::<_, ()>(&mut conn)
                .await.ok();
        }

        Ok(())
    }

    async fn process_burn_block(&self, data: BurnBlockData) -> anyhow::Result<()> {
        tracing::info!("Processing burn block: {} (hard-finality)", data.hash);

        let mut tx = self.storage.pg_pool.begin().await?;

        // Update all previous soft blocks to hard state up to this height
        sqlx::query(
            "UPDATE stacks_blocks SET state = 'hard' WHERE height <= $1 AND state = 'soft'",
        )
        .bind(data.height as i64)
        .execute(&mut *tx)
        .await?;

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

        let root = self.state.get_state_root();
        self.persist_root_to_redis(&root).await.ok();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::NexusState;
    use crate::storage::Storage;
    use std::sync::Arc;

    // Note: These tests are disabled by default as they require a running Postgres/Redis.
    #[tokio::test]
    #[ignore]
    async fn test_handle_microblock_event() {
        let storage = Arc::new(Storage::new().await.unwrap());
        let state = Arc::new(NexusState::new());
        let sync = NexusSync::new(storage, state);

        let event = StacksEvent::Microblock(MicroblockData {
            hash: "test-hash".to_string(),
            height: 100,
            parent_hash: "parent".to_string(),
            txs: vec![TransactionData {
                tx_id: "tx-test".to_string(),
                sender: "SP123".to_string(),
                payload: Some("data".to_string()),
            }],
            timestamp: Utc::now(),
        });

        sync.handle_event(event).await.unwrap();
        // Should verify the root changed
        assert_ne!(sync.state.get_state_root(), "0x0000000000000000000000000000000000000000000000000000000000000000");
    }
}
