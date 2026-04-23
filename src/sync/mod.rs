//! nexus-sync module handles the ingestion and processing of Stacks L1 events,
//! maintaining a local representation of the Stacks L1 state.

use crate::state::NexusState;
use crate::storage::kwil::{KwilAdapter, KwilBlockCommitment, KwilStateRootCommitment};
use crate::storage::tableland::{TablelandAdapter, TablelandStateCommitment};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use reqwest::Client;
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

const LAST_POLLED_BURN_TIP_KEY: &str = "nexus:sync:last_polled_burn_tip:v1";

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

        // [CON-330] Commit block + state root to Kwil (Sovereign SQL Pilot)
        if let Some(kwil) = &self.kwil {
            let _ = kwil
                .persist_block(KwilBlockCommitment {
                    hash: data.hash.clone(),
                    height: data.height,
                    block_type: "microblock".to_string(),
                    state: "soft".to_string(),
                })
                .await
                .ok();

            let _ = kwil
                .persist_state_root(KwilStateRootCommitment {
                    block_height: data.height,
                    state_root: root.clone(),
                })
                .await
                .ok();
        }

        Ok(())
    }

    async fn process_burn_block(&self, data: BurnBlockData) -> anyhow::Result<()> {
        tracing::info!("Processing hard-finality tip: {}", data.hash);

        // `BurnBlockData.height` must be expressed in the same height domain as
        // `stacks_blocks.height` for microblocks (Stacks chain height).
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

        let Some((burn_tip_height, burn_tip_hash)) = extract_burn_tip_from_info(&info) else {
            tracing::debug!("Skipping /v2/info poll: burn tip fields missing or empty");
            return Ok(());
        };

        let burn_tip_marker = format!("{}:{}", burn_tip_height, burn_tip_hash);

        let mut redis_conn = match storage
            .redis_client
            .get_multiplexed_async_connection()
            .await
        {
            Ok(conn) => Some(conn),
            Err(err) => {
                tracing::warn!("Unable to connect to Redis for burn-tip dedupe: {}", err);
                None
            }
        };

        if let Some(conn) = redis_conn.as_mut() {
            let last_polled_tip: Option<String> = match redis::cmd("GET")
                .arg(LAST_POLLED_BURN_TIP_KEY)
                .query_async(conn)
                .await
            {
                Ok(value) => value,
                Err(err) => {
                    tracing::warn!(
                        "Unable to read burn-tip dedupe key from Redis ({}): {}",
                        LAST_POLLED_BURN_TIP_KEY,
                        err
                    );
                    None
                }
            };

            if last_polled_tip.as_deref() == Some(burn_tip_marker.as_str()) {
                return Ok(());
            }
        }

        let inserted = sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state, created_at)
             VALUES ($1, $2, 'burn_block', 'hard', $3)
             ON CONFLICT (hash) DO NOTHING",
        )
        .bind(&burn_tip_hash)
        .bind(burn_tip_height as i64)
        .bind(Utc::now())
        .execute(&storage.pg_pool)
        .await?
        .rows_affected()
            > 0;

        if inserted {
            tracing::info!(
                "Found new hard-finality burn tip: height={}, hash={}",
                burn_tip_height,
                burn_tip_hash
            );

            tx.send(StacksEvent::BurnBlock(BurnBlockData {
                hash: burn_tip_hash.clone(),
                height: burn_tip_height,
                timestamp: Utc::now(),
            }))
            .await?;
        }

        if let Some(conn) = redis_conn.as_mut() {
            if let Err(err) = redis::cmd("SET")
                .arg(LAST_POLLED_BURN_TIP_KEY)
                .arg(&burn_tip_marker)
                .query_async::<_, ()>(conn)
                .await
            {
                tracing::warn!(
                    "Unable to persist burn-tip dedupe key to Redis ({}): {}",
                    LAST_POLLED_BURN_TIP_KEY,
                    err
                );
            }
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

fn extract_burn_tip_from_info(info: &serde_json::Value) -> Option<(u64, String)> {
    let burn_tip_height = info["burn_block_height"].as_u64().unwrap_or(0);

    // `burn_block_hash` is not always present in `/v2/info` across Stacks node versions.
    // Fall back to `pox_consensus` (burnchain view identifier) to keep a stable tip identifier.
    let burn_tip_hash = info["burn_block_hash"]
        .as_str()
        .filter(|hash| !hash.is_empty())
        .or_else(|| {
            info["pox_consensus"]
                .as_str()
                .filter(|hash| !hash.is_empty())
        })
        .unwrap_or("");

    if burn_tip_height == 0 || burn_tip_hash.is_empty() {
        return None;
    }

    Some((burn_tip_height, burn_tip_hash.to_string()))
}

#[cfg(test)]
mod tests {
    use super::extract_burn_tip_from_info;
    use serde_json::json;

    #[test]
    fn extract_burn_tip_prefers_burn_block_hash() {
        let info = json!({
            "burn_block_height": 101,
            "burn_block_hash": "0xabc",
            "pox_consensus": "0xdef"
        });

        assert_eq!(
            extract_burn_tip_from_info(&info),
            Some((101, "0xabc".to_string()))
        );
    }

    #[test]
    fn extract_burn_tip_falls_back_to_pox_consensus() {
        let info = json!({
            "burn_block_height": 101,
            "pox_consensus": "0xdef"
        });

        assert_eq!(
            extract_burn_tip_from_info(&info),
            Some((101, "0xdef".to_string()))
        );
    }

    #[test]
    fn extract_burn_tip_returns_none_when_required_fields_missing() {
        let missing_hash = json!({ "burn_block_height": 101 });
        let missing_height = json!({ "pox_consensus": "0xdef" });

        assert_eq!(extract_burn_tip_from_info(&missing_hash), None);
        assert_eq!(extract_burn_tip_from_info(&missing_height), None);
    }
}
