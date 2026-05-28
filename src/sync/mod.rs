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
        let mut conn = self.storage.redis_client.get_multiplexed_async_connection().await?;
        let _: () = redis::cmd("SET").arg("nexus:state_root").arg(root).query_async::<()>(&mut conn).await?;
        Ok(())
    }
}
