-- Migration: Create audit log for Stacks / sBTC verified transactions
CREATE TABLE IF NOT EXISTS stacks_verified_transactions (
    tx_id TEXT PRIMARY KEY,
    sender TEXT NOT NULL,
    amount_sbtc BIGINT NOT NULL,
    status TEXT NOT NULL,
    verified_at_height BIGINT NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_stacks_verified_tx_sender ON stacks_verified_transactions(sender);
