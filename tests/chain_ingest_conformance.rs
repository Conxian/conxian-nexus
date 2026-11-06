//! Conformance tests for canonical Stacks chain ingestion (#516).
//!
//! The database cases require the explicit disposable
//! `CONFORMANCE_DATABASE_URL` + `ALLOW_DESTRUCTIVE_TEST_RESET=true` fixture
//! (see docs/REPLAY_CONFORMANCE.md). The parsing cases are pure.

#[path = "support/conformance_database.rs"]
mod conformance_database;
use conformance_database::ConformanceDatabase;

use conxian_nexus::storage::Storage;
use conxian_nexus::sync::chain_ingest::{
    blocks_url, parse_burn_blocks, persist_burn_blocks, processed_burn_height,
    CommittedBurnBlock, STATE_ORPHANED,
};

static SUITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn storage_for(database: &ConformanceDatabase) -> Storage {
    let pg_pool = database.pool().await;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS stacks_blocks (\
            hash TEXT PRIMARY KEY, height BIGINT NOT NULL, type TEXT NOT NULL, \
            state TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW())",
    )
    .execute(&pg_pool)
    .await
    .expect("create disposable stacks_blocks fixture");
    let redis_client =
        redis::Client::open("redis://127.0.0.1/").expect("static redis URL is valid");
    Storage {
        pg_pool,
        redis_client,
    }
}

fn burn(hash: &str, height: u64) -> CommittedBurnBlock {
    CommittedBurnBlock {
        hash: hash.to_string(),
        height,
    }
}

#[test]
fn parses_distinct_canonical_burn_blocks_and_drops_non_canonical_forks() {
    let body = r#"{
        "limit": 3, "offset": 0,
        "results": [
            {"hash": "0xaa", "burn_block_height": 974000, "burn_block_hash": "0xburn1", "canonical": true},
            {"hash": "0xbb", "burn_block_height": 974000, "burn_block_hash": "0xburn1", "canonical": true},
            {"hash": "0xcc", "burn_block_height": 974000, "burn_block_hash": "0xburn2", "canonical": false},
            {"hash": "0xdd", "burn_block_height": 974001, "burn_block_hash": "0xburn3", "canonical": true}
        ]
    }"#;
    let blocks = parse_burn_blocks(body).expect("valid page parses");
    assert_eq!(
        blocks,
        vec![burn("0xburn1", 974000), burn("0xburn3", 974001)]
    );
}

#[test]
fn malformed_page_is_an_error_not_empty_success() {
    assert!(parse_burn_blocks("not-json").is_err());
}

#[test]
fn blocks_url_never_exceeds_indexer_page_limit() {
    // The Stacks indexer rejects `limit > 30` with 400, which would starve the
    // safety monitor of any observation (#516). Oversized requests are clamped.
    assert!(blocks_url("https://api.mainnet.hiro.so", 50).ends_with("limit=30"));
    assert!(blocks_url("https://api.mainnet.hiro.so", 30).ends_with("limit=30"));
    assert!(blocks_url("https://api.mainnet.hiro.so", 0).ends_with("limit=1"));
    assert!(blocks_url("https://api.mainnet.hiro.so/", 5).ends_with("limit=5"));
}

#[tokio::test]
async fn ingestion_records_committed_height_and_reorgs_orphan_replaced_blocks() {
    let database = ConformanceDatabase::from_environment();
    let _guard = SUITE_LOCK.lock().await;
    database.prepare().await;
    let storage = storage_for(&database).await;

    assert_eq!(
        processed_burn_height(&storage).await.unwrap(),
        None,
        "no committed observation before ingestion"
    );

    persist_burn_blocks(&storage, &[burn("0xburn1", 974000)])
        .await
        .unwrap();
    assert_eq!(processed_burn_height(&storage).await.unwrap(), Some(974000));

    // Reorg: a different burn block at the same height replaces the first.
    persist_burn_blocks(&storage, &[burn("0xburn1b", 974000)])
        .await
        .unwrap();
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT hash, state FROM stacks_blocks ORDER BY hash")
            .fetch_all(&storage.pg_pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![
            ("0xburn1".to_string(), STATE_ORPHANED.to_string()),
            ("0xburn1b".to_string(), "hard".to_string()),
        ]
    );
    assert_eq!(processed_burn_height(&storage).await.unwrap(), Some(974000));
}
