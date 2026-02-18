CREATE TABLE IF NOT EXISTS stacks_transactions (
    tx_id TEXT PRIMARY KEY,
    block_hash TEXT NOT NULL REFERENCES stacks_blocks(hash),
    payload TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_stacks_transactions_block_hash ON stacks_transactions(block_hash);
CREATE INDEX IF NOT EXISTS idx_stacks_blocks_type_height ON stacks_blocks(type, height DESC);
