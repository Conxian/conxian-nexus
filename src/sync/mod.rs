//! nexus-sync module handles the ingestion and processing of Stacks L1 events,
//! maintaining a local representation of the Stacks L1 state.

use crate::state::NexusState;
use crate::storage::tableland::{TablelandAdapter, TablelandStateCommitment};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use reqwest::Client;
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

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
    tableland: Arc<TablelandAdapter>,
    rpc_url: String,
    http_client: Client,
    event_tx: mpsc::Sender<StacksEvent>,
    event_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<StacksEvent>>>,
}

impl NexusSync {
    pub fn new(
        storage: Arc<Storage>,
        state: Arc<NexusState>,
        tableland: Arc<TablelandAdapter>,
        rpc_url: String,
    ) -> Self {
        let (tx, rx) = mpsc::channel(1000);
        Self {
            storage,
            state,
            tableland,
            rpc_url,
            http_client: Client::new(),
            event_tx: tx,
            event_rx: Arc::new(tokio::sync::Mutex::new(rx)),
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Starting NexusSync service (RPC: {})...", self.rpc_url);

        // Spawn Poller task
        let poller_tx = self.event_tx.clone();
        let rpc_url = self.rpc_url.clone();
        let storage = self.storage.clone();
        let http_client = self.http_client.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                if let Err(e) =
                    Self::poll_stacks_node(&poller_tx, &rpc_url, &storage, &http_client).await
                {
                    tracing::error!("Sync polling error: {}", e);
                }
            }
        });

        // Event Processing Loop
        let mut rx = self.event_rx.lock().await;
        while let Some(event) = rx.recv().await {
            if let Err(e) = self.handle_event(event).await {
                tracing::error!("Error handling event: {}", e);
            }
        }

        Ok(())
    }

    pub async fn load_initial_state(&self) -> anyhow::Result<()> {
        tracing::info!("Loading initial state from DB...");
        let rows = sqlx::query(
            "SELECT tx_id FROM stacks_transactions t JOIN stacks_blocks b ON t.block_hash = b.hash WHERE b.state != 'orphaned' ORDER BY b.height ASC, t.created_at ASC"
        )
        .fetch_all(&self.storage.pg_pool)
        .await?;

        let mut tx_ids = Vec::new();
        for row in rows {
            tx_ids.push(row.get::<String, _>(0));
        }

        self.state.set_initial_leaves(tx_ids);
        tracing::info!(
            "State loaded. Current root: {}",
            self.state.get_state_root()
        );
        Ok(())
    }

    pub async fn handle_event(&self, event: StacksEvent) -> anyhow::Result<()> {
        match event {
            StacksEvent::Microblock(data) => self.process_microblock(data).await?,
            StacksEvent::BurnBlock(data) => self.process_burn_block(data).await?,
        }
        Ok(())
    }

    async fn process_microblock(&self, data: MicroblockData) -> anyhow::Result<()> {
        tracing::debug!("Processing microblock: {}", data.hash);

        // [NEXUS-03] Microblock Reorg Detection & Rollback
        if let Some(parent) = self.get_latest_block_hash().await? {
            if parent != data.parent_hash {
                tracing::warn!(
                    "Reorg detected! Parent mismatch: {} != {}",
                    parent,
                    data.parent_hash
                );
                self.handle_microblock_reorg(&data).await?;
            }
        }

        let mut tx = self.storage.pg_pool.begin().await?;
        sqlx::query("INSERT INTO stacks_blocks (hash, height, type, state, created_at) VALUES ($1, $2, 'microblock', 'soft', $3) ON CONFLICT (hash) DO NOTHING")
            .bind(&data.hash).bind(data.height as i64).bind(data.timestamp).execute(&mut *tx).await?;

        for tx_data in &data.txs {
            sqlx::query("INSERT INTO stacks_transactions (tx_id, block_hash, sender, payload, created_at) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (tx_id) DO NOTHING")
                .bind(&tx_data.tx_id).bind(&data.hash).bind(&tx_data.sender).bind(&tx_data.payload).bind(data.timestamp).execute(&mut *tx).await?;
        }
        tx.commit().await?;

        let tx_ids: Vec<String> = data.txs.iter().map(|t| t.tx_id.clone()).collect();
        let added_nodes = self.state.update_state_batch(&tx_ids);

        // Persist MMR nodes [CON-396]
        self.persist_mmr_state(data.height, &added_nodes).await?;

        let root = self.state.get_state_root();

        // [CON-69] Commit state root to Tableland
        let _ = self
            .tableland
            .commit_state(TablelandStateCommitment {
                table_id: "nexus_state_roots".to_string(),
                query: format!(
                    "INSERT INTO nexus_state_roots (block_height, state_root) VALUES ({}, '{}')",
                    data.height, root
                ),
                timestamp: Utc::now().timestamp(),
            })
            .await
            .ok();

        Ok(())
    }

    async fn process_burn_block(&self, data: BurnBlockData) -> anyhow::Result<()> {
        tracing::info!("Processing burn block: {}", data.hash);

        sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state, created_at)
             VALUES ($1, $2, 'burn_block', 'hard', $3)
             ON CONFLICT (hash) DO NOTHING",
        )
        .bind(&data.hash)
        .bind(data.height as i64)
        .bind(data.timestamp)
        .execute(&self.storage.pg_pool)
        .await?;

        sqlx::query(
            "UPDATE stacks_blocks
             SET state = 'hard'
             WHERE type = 'microblock' AND height <= $1 AND state = 'soft'",
        )
        .bind(data.height as i64)
        .execute(&self.storage.pg_pool)
        .await?;
        Ok(())
    }

    async fn poll_stacks_node(
        tx: &mpsc::Sender<StacksEvent>,
        rpc_url: &str,
        storage: &Storage,
        http_client: &Client,
    ) -> anyhow::Result<()> {
        // [NEXUS-02] Real-time Sync Ingestion via Hiro RPC
        let info_url = format!("{}/v2/info", rpc_url);
        let info: serde_json::Value = http_client.get(info_url).send().await?.json().await?;

        let burn_block_height = info["burn_block_height"].as_u64().unwrap_or(0);
        let burn_block_hash = info["burn_block_hash"].as_str().unwrap_or("");

        if burn_block_hash.is_empty() || burn_block_height == 0 {
            return Ok(());
        }

        let inserted = sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state, created_at)
             VALUES ($1, $2, 'burn_block', 'hard', $3)
             ON CONFLICT (hash) DO NOTHING",
        )
        .bind(burn_block_hash)
        .bind(burn_block_height as i64)
        .bind(Utc::now())
        .execute(&storage.pg_pool)
        .await?
        .rows_affected()
            > 0;

        if inserted {
            tracing::info!(
                "Found new burn block tip: height={}, hash={}",
                burn_block_height,
                burn_block_hash
            );

            tx.send(StacksEvent::BurnBlock(BurnBlockData {
                hash: burn_block_hash.to_string(),
                height: burn_block_height,
                timestamp: Utc::now(),
            }))
            .await?;
        }

        Ok(())
    }

    async fn get_latest_block_hash(&self) -> anyhow::Result<Option<String>> {
        let row = sqlx::query(
            "SELECT hash FROM stacks_blocks
             WHERE type = 'microblock' AND state != 'orphaned'
             ORDER BY height DESC
             LIMIT 1",
        )
        .fetch_optional(&self.storage.pg_pool)
        .await?;
        Ok(row.map(|r| r.get(0)))
    }

    async fn handle_microblock_reorg(&self, data: &MicroblockData) -> anyhow::Result<()> {
        tracing::info!("Rolling back to last valid burn block height...");
        // 1. Mark orphaned blocks
        sqlx::query(
            "UPDATE stacks_blocks
             SET state = 'orphaned'
             WHERE type = 'microblock' AND height >= $1 AND state = 'soft'",
        )
        .bind(data.height as i64)
        .execute(&self.storage.pg_pool)
        .await?;

        // 2. Clear state and rebuild from DB
        self.load_initial_state().await?;
        Ok(())
    }

    async fn persist_mmr_state(
        &self,
        height: u64,
        nodes: &[(u64, [u8; 32])],
    ) -> anyhow::Result<()> {
        for (pos, hash) in nodes {
            sqlx::query("INSERT INTO mmr_nodes (pos, hash, block_height) VALUES ($1, $2, $3) ON CONFLICT (pos) DO UPDATE SET hash = $2, block_height = $3")
                .bind(*pos as i64)
                .bind(hex::encode(hash))
                .bind(height as i64)
                .execute(&self.storage.pg_pool).await?;
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
            .query_async(&mut conn)
            .await?;
        Ok(())
    }
}
