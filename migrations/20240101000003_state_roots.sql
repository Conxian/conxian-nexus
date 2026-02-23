CREATE TABLE IF NOT EXISTS nexus_state_roots (
    block_height BIGINT PRIMARY KEY,
    state_root TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
