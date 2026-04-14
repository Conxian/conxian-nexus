//! nexus-sync module handles the ingestion and processing of Stacks L1 events,
//! maintaining a local representation of the Stacks L1 state.

use crate::state::NexusState;
use crate::storage::Storage;
use crate::storage::kwil::{KwilAdapter, KwilBlockCommitment, KwilStateRootCommitment};
use crate::storage::tableland::{TablelandAdapter, TablelandStateCommitment};
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
    kwil: Option<Arc<KwilAdapter>>,
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
        kwil: Option<Arc<KwilAdapter>>,
        rpc_url: String,
    ) -> Self {
        let (tx, rx) = mpsc::channel(1000);
        Self {
            storage,
            state,
            tableland,
            kwil,
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
        tracing::info!("State loaded. Current root: {}", self.state.get_state_root());
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
                tracing::warn!("Reorg detected! Parent mismatch: {} != {}", parent, data.parent_hash);
                self.handle_microblock_reorg(&data).await?;
            }
        }

        let mut tx = self.storage.pg_pool.begin().await?;
        sqlx::query("INSERT INTO stacks_blocks (hash, height, type, state, created_at) VALUES (, , 'microblock', 'soft', ) ON CONFLICT (hash) DO NOTHING")
            .bind(&data.hash).bind(data.height as i64).bind(data.timestamp).execute(&mut *tx).await?;

        for tx_data in &data.txs {
            sqlx::query("INSERT INTO stacks_transactions (tx_id, block_hash, sender, payload, created_at) VALUES (, , , , ) ON CONFLICT (tx_id) DO NOTHING")
                .bind(&tx_data.tx_id).bind(&data.hash).bind(&tx_data.sender).bind(&tx_data.payload).bind(data.timestamp).execute(&mut *tx).await?;
        }
        tx.commit().await?;

        let tx_ids: Vec<String> = data.txs.iter().map(|t| t.tx_id.clone()).collect();
        let added_nodes = self.state.update_state_batch(&tx_ids);

        // Persist MMR nodes [CON-396]
        self.persist_mmr_state(data.height, &added_nodes).await?;

        let root = self.state.get_state_root();

        // [CON-69] Commit state root to Tableland
        let _ = self.tableland.commit_state(TablelandStateCommitment {
            table_id: "nexus_state_roots".to_string(),
            query: format!("INSERT INTO nexus_state_roots (block_height, state_root) VALUES ({}, '{}')", data.height, root),
            timestamp: Utc::now().timestamp(),
        }).await.ok();

        // [CON-330] Commit block and state root to Kwil (Sovereign SQL Pilot)
        if let Some(kwil) = &self.kwil {
            let _ = kwil.persist_block(KwilBlockCommitment {
                hash: data.hash.clone(),
                height: data.height,
                block_type: "microblock".to_string(),
                state: "soft".to_string(),
            }).await.ok();

            let _ = kwil.persist_state_root(KwilStateRootCommitment {
                block_height: data.height,
                state_root: root.clone(),
            }).await.ok();
        }

        Ok(())
    }

    async fn process_burn_block(&self, data: BurnBlockData) -> anyhow::Result<()> {
        tracing::info!("Processing burn block: {}", data.hash);
        sqlx::query("UPDATE stacks_blocks SET state = 'hard' WHERE height <=  AND state = 'soft'")
            .bind(data.height as i64).execute(&self.storage.pg_pool).await?;

        // Finalize in Kwil if enabled
        if let Some(kwil) = &self.kwil {
            // Note: In a production KwilAdapter, we would add an action for multi-row updates
            // For the pilot, we assume block state management is handled.
            tracing::info!("Pilot: Burn block {} processed in Kwil context", data.hash);
        }

        Ok(())
    }

    async fn poll_stacks_node(_tx: &mpsc::Sender<StacksEvent>,  _rpc_url: &str,  _storage: &Storage,  _http_client: &Client) -> anyhow::Result<()> {
        // Implementation of polling would go here (Hiro RPC / WebSockets)
        Ok(())
    }

    async fn get_latest_block_hash(&self) -> anyhow::Result<Option<String>> {
        let row = sqlx::query("SELECT hash FROM stacks_blocks ORDER BY height DESC LIMIT 1")
            .fetch_optional(&self.storage.pg_pool)
            .await?;
        Ok(row.map(|r| r.get(0)))
    }

    async fn handle_microblock_reorg(&self, data: &MicroblockData) -> anyhow::Result<()> {
        tracing::info!("Rolling back to last valid burn block height...");
        // 1. Mark orphaned blocks
        sqlx::query("UPDATE stacks_blocks SET state = 'orphaned' WHERE height >=  AND state = 'soft'")
            .bind(data.height as i64)
            .execute(&self.storage.pg_pool).await?;

        // 2. Clear state and rebuild from DB
        self.load_initial_state().await?;
        Ok(())
    }

    async fn persist_mmr_state(&self, height: u64, nodes: &[(u64, [u8; 32])]) -> anyhow::Result<()> {
        for (pos, hash) in nodes {
            sqlx::query("INSERT INTO mmr_nodes (pos, hash, block_height) VALUES (, , ) ON CONFLICT (pos) DO UPDATE SET hash = , block_height = ")
                .bind(*pos as i64)
                .bind(hex::encode(hash))
                .bind(height as i64)
                .execute(&self.storage.pg_pool).await?;
        }
        Ok(())
    }

    pub async fn persist_root_to_redis(&self, root: &str) -> anyhow::Result<()> {
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;
        let _: () = redis::cmd("SET").arg("nexus:state_root").arg(root).query_async(&mut conn).await?;
        Ok(())
    }
}
