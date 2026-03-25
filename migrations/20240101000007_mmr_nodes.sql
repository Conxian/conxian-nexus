CREATE TABLE IF NOT EXISTS mmr_nodes (
    pos BIGINT PRIMARY KEY,
    hash BYTEA NOT NULL,
    block_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_mmr_nodes_block_height ON mmr_nodes(block_height);
