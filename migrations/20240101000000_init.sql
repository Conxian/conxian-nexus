CREATE TABLE IF NOT EXISTS stacks_blocks (
    hash TEXT PRIMARY KEY,
    height BIGINT NOT NULL,
    type TEXT NOT NULL, -- 'microblock' or 'burn_block'
    state TEXT NOT NULL, -- 'soft' or 'hard'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_stacks_blocks_height ON stacks_blocks(height);
CREATE INDEX IF NOT EXISTS idx_stacks_blocks_created_at ON stacks_blocks(created_at);
