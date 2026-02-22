//! nexus-sync module handles the ingestion and processing of Stacks L1 events,
//! maintaining a local representation of the Stacks L1 state.

use crate::state::NexusState;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use sqlx::Row;
use std::sync::Arc;
use tokio::time::{self, Duration};
use reqwest::Client;

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
    rpc_url: String,
    http_client: Client,
}

impl NexusSync {
    pub fn new(storage: Arc<Storage>, state: Arc<NexusState>, rpc_url: String) -> Self {
        Self {
            storage,
            state,
            rpc_url,
            http_client: Client::new(),
        }
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
        tracing::info!("Starting NexusSync service (RPC: {})...", self.rpc_url);

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        // Spawn Poller task
        let poller_tx = tx.clone();
        let rpc_url = self.rpc_url.clone();
        let storage = self.storage.clone();
        let http_client = self.http_client.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(20));
            loop {
                interval.tick().await;
                if let Err(e) = Self::poll_stacks_node(&poller_tx, &rpc_url, &storage, &http_client).await {
                    tracing::error!("Sync polling error: {}", e);
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

    async fn poll_stacks_node(
        tx: &tokio::sync::mpsc::Sender<StacksEvent>,
        rpc_url: &str,
        storage: &Storage,
        http_client: &Client,
    ) -> anyhow::Result<()> {
        // 1. Get current height from L1
        let url = format!("{}/extended/v1/block?limit=1", rpc_url);
        let resp = http_client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Stacks RPC returned error: {}", resp.status()));
        }
        let json: serde_json::Value = resp.json().await?;
        let latest_l1_height = json["results"][0]["height"].as_u64().unwrap_or(0);

        // 2. Get processed height from DB
        let row = sqlx::query("SELECT MAX(height) as max_height FROM stacks_blocks")
            .fetch_one(&storage.pg_pool)
            .await?;
        let processed_height: i64 = row.get::<Option<i64>, _>("max_height").unwrap_or(0);

        if latest_l1_height > processed_height as u64 {
            for height in (processed_height as u64 + 1)..=latest_l1_height {
                tracing::info!("Fetching data for height: {}", height);

                let block_url = format!("{}/extended/v1/block/by_height/{}", rpc_url, height);
                let block_resp = http_client.get(&block_url).send().await?;
                if !block_resp.status().is_success() {
                    tracing::warn!("Failed to fetch block at height {}: {}", height, block_resp.status());
                    continue;
                }
                let block_json: serde_json::Value = block_resp.json().await?;

                let hash = block_json["hash"].as_str().unwrap_or("").to_string();
                let timestamp = block_json["burn_block_time_iso"].as_str()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);

                // Fetch transactions
                let txs_url = format!("{}/extended/v1/tx/block_height/{}", rpc_url, height);
                let txs_resp = http_client.get(&txs_url).send().await?;
                let mut txs = Vec::new();

                if txs_resp.status().is_success() {
                    let txs_json: serde_json::Value = txs_resp.json().await?;
                    if let Some(results) = txs_json["results"].as_array() {
                        for tx_val in results {
                            txs.push(TransactionData {
                                tx_id: tx_val["tx_id"].as_str().unwrap_or("").to_string(),
                                sender: tx_val["sender_address"].as_str().unwrap_or("").to_string(),
                                payload: Some(tx_val["tx_type"].as_str().unwrap_or("").to_string()),
                            });
                        }
                    }
                }

                let event = StacksEvent::BurnBlock(BurnBlockData {
                    hash: hash.clone(),
                    height,
                    timestamp,
                });
                tx.send(event).await?;

                // Also send as microblock if it has transactions, for state update
                if !txs.is_empty() {
                    let micro_event = StacksEvent::Microblock(MicroblockData {
                        hash,
                        height,
                        parent_hash: "".to_string(),
                        txs,
                        timestamp,
                    });
                    tx.send(micro_event).await?;
                }
            }
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
            "INSERT INTO stacks_blocks (hash, height, type, state, created_at) VALUES (, , 'microblock', 'soft', ) ON CONFLICT (hash) DO NOTHING"
        )
        .bind(&data.hash)
        .bind(data.height as i64)
        .bind(data.timestamp)
        .execute(&mut *tx).await?;

        for tx_data in &data.txs {
            sqlx::query(
                "INSERT INTO stacks_transactions (tx_id, block_hash, sender, payload, created_at) VALUES (, , , , ) ON CONFLICT (tx_id) DO NOTHING"
            )
            .bind(&tx_data.tx_id)
            .bind(&data.hash)
            .bind(&tx_data.sender)
            .bind(&tx_data.payload)
            .bind(data.timestamp)
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;

        let tx_ids: Vec<String> = data.txs.iter().map(|t| t.tx_id.clone()).collect();
        self.state.update_state_batch(&tx_ids);

        let root = self.state.get_state_root();
        self.persist_root_to_redis(&root).await.ok();

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

        sqlx::query(
            "UPDATE stacks_blocks SET state = 'hard' WHERE height <=  AND state = 'soft'",
        )
        .bind(data.height as i64)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state, created_at) VALUES (, , 'burn_block', 'hard', ) ON CONFLICT (hash) DO NOTHING"
        )
        .bind(&data.hash)
        .bind(data.height as i64)
        .bind(data.timestamp)
        .execute(&mut *tx).await?;

        tx.commit().await?;

        // Note: Burn blocks confirm transactions, they don't necessarily add new state to the transaction tree
        // unless we want to include block hashes in the root.
        // For now, we only update the root in Redis to ensure it's current.
        let root = self.state.get_state_root();
        self.persist_root_to_redis(&root).await.ok();

        Ok(())
    }
}
