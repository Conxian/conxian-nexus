//! Canonical Stacks chain ingestion for the Nexus observation surface.
//!
//! The safety monitor compares a local processed burn height against the
//! trusted Bitcoin height provider (`/v2/info` `burn_block_height`). Before
//! this module the deployed node never populated `stacks_blocks`, so the
//! comparison had no local observation and stayed permanently fail-closed in
//! safety mode (issue #516).
//!
//! This ingestor reads the canonical Stacks block indexer, derives the Bitcoin
//! burn block that each committed anchor block belongs to, and persists those
//! burn blocks (`type = 'burn_block'`, `height` = Bitcoin burn height) so the
//! safety comparison has a real observation. Ingestion is observation-only: it
//! never signs, submits transactions, or mutates provider configuration.
//!
//! Reorgs are handled by marking a replaced burn block at the same height
//! `orphaned` in the same transaction that records its replacement. Full
//! historical backfill and replay remain part of #516's wider acceptance.

use crate::storage::Storage;
use serde::Deserialize;
use sqlx::Row;
use std::collections::BTreeMap;
use std::time::Duration;

/// Stacks block type recorded in `stacks_blocks.type`.
pub const TYPE_BURN_BLOCK: &str = "burn_block";

/// `stacks_blocks.state` values.
pub const STATE_HARD: &str = "hard";
pub const STATE_ORPHANED: &str = "orphaned";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Maximum page size accepted by the Stacks indexer
/// (`/extended/v1/block`). Values above this return `400 FST_ERR_VALIDATION`
/// ("querystring/limit must be <= 30"), which previously produced an empty
/// observation set and left the node permanently in safety mode (#516).
pub const PAGE_LIMIT: u32 = 30;

/// A committed Bitcoin burn block observed through the Stacks indexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedBurnBlock {
    pub hash: String,
    pub height: u64,
}

#[derive(Debug, Deserialize)]
struct RawBlock {
    #[serde(default)]
    burn_block_hash: Option<String>,
    #[serde(default)]
    burn_block_height: Option<u64>,
    #[serde(default)]
    canonical: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawPage {
    #[serde(default)]
    results: Vec<RawBlock>,
}

/// Derives the distinct committed Bitcoin burn blocks from a
/// `/extended/v1/block` page.
///
/// Only canonical anchor blocks are considered; non-canonical forks and rows
/// missing a burn identity are skipped rather than invented. The result is
/// ordered by ascending burn height.
pub fn parse_burn_blocks(body: &str) -> anyhow::Result<Vec<CommittedBurnBlock>> {
    let page: RawPage = serde_json::from_str(body)?;
    let mut by_hash: BTreeMap<String, u64> = BTreeMap::new();
    for block in page.results {
        if !block.canonical.unwrap_or(true) {
            continue;
        }
        if let (Some(hash), Some(height)) = (block.burn_block_hash, block.burn_block_height) {
            if height > 0 {
                by_hash.insert(hash, height);
            }
        }
    }
    let mut blocks: Vec<CommittedBurnBlock> = by_hash
        .into_iter()
        .map(|(hash, height)| CommittedBurnBlock { hash, height })
        .collect();
    blocks.sort_by_key(|b| b.height);
    Ok(blocks)
}

/// Builds the canonical Stacks block list URL for the current tip.
///
/// The requested page size is clamped to [`PAGE_LIMIT`] so a caller can never
/// exceed the indexer's accepted maximum and trigger a `400`.
pub fn blocks_url(rpc_url: &str, limit: u32) -> String {
    let limit = limit.clamp(1, PAGE_LIMIT);
    format!(
        "{}/extended/v1/block?limit={limit}",
        rpc_url.trim_end_matches('/')
    )
}

/// Reads the highest committed burn height already observed.
pub async fn processed_burn_height(storage: &Storage) -> anyhow::Result<Option<u64>> {
    let row = sqlx::query(
        "SELECT MAX(height) AS max_height FROM stacks_blocks \
         WHERE type = 'burn_block' AND state = 'hard'",
    )
    .fetch_one(&storage.pg_pool)
    .await?;
    let max_height: Option<i64> = row.get("max_height");
    Ok(max_height.filter(|h| *h > 0).map(|h| h as u64))
}

/// Persists committed burn blocks and marks any previously committed block at
/// the same height with a different hash as `orphaned` (reorg).
///
/// The replacement and orphan marker are applied in one transaction so a crash
/// can never leave two `hard` burn blocks at the same height.
pub async fn persist_burn_blocks(
    storage: &Storage,
    blocks: &[CommittedBurnBlock],
) -> anyhow::Result<u64> {
    let mut tx = storage.pg_pool.begin().await?;
    let mut written = 0u64;
    for block in blocks {
        let height = block.height as i64;
        sqlx::query(
            "UPDATE stacks_blocks SET state = 'orphaned' \
             WHERE height = $1 AND type = $2 AND state = 'hard' AND hash <> $3",
        )
        .bind(height)
        .bind(TYPE_BURN_BLOCK)
        .bind(&block.hash)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO stacks_blocks (hash, height, type, state) \
             VALUES ($1, $2, $3, 'hard') \
             ON CONFLICT (hash) DO UPDATE SET height = EXCLUDED.height, state = EXCLUDED.state",
        )
        .bind(&block.hash)
        .bind(height)
        .bind(TYPE_BURN_BLOCK)
        .execute(&mut *tx)
        .await?;
        written += 1;
    }
    tx.commit().await?;
    Ok(written)
}

#[derive(Clone)]
pub struct ChainIngestor {
    storage: std::sync::Arc<Storage>,
    client: reqwest::Client,
    rpc_url: String,
}

impl ChainIngestor {
    pub fn new(storage: std::sync::Arc<Storage>, rpc_url: String) -> Self {
        Self {
            storage,
            client: reqwest::Client::new(),
            rpc_url,
        }
    }

    /// Fetches the canonical tip page and persists its distinct committed burn
    /// blocks. Returns the number of rows written.
    pub async fn ingest_tip(&self) -> anyhow::Result<u64> {
        let url = blocks_url(&self.rpc_url, PAGE_LIMIT);
        let response = self
            .client
            .get(&url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!("block indexer returned {}", response.status());
        }
        let body = response.text().await?;
        let blocks = parse_burn_blocks(&body)?;
        if blocks.is_empty() {
            return Ok(0);
        }
        persist_burn_blocks(&self.storage, &blocks).await
    }

    /// Catch-up loop: ingest, then wait. A failure is logged and retried; it is
    /// never converted into an empty success that would clear safety mode.
    pub async fn run(&self, interval: Duration) -> anyhow::Result<()> {
        loop {
            match self.ingest_tip().await {
                Ok(count) if count > 0 => {
                    tracing::info!(blocks = count, "Ingested committed Bitcoin burn blocks")
                }
                Ok(_) => tracing::debug!("No new committed burn blocks"),
                Err(error) => tracing::error!("Chain ingestion error: {error}"),
            }
            tokio::time::sleep(interval).await;
        }
    }
}
