-- [NIP-005] EVM and Cosmos Verification Audit Logs

CREATE TABLE IF NOT EXISTS evm_verified_receipts (
    block_hash TEXT NOT NULL,
    transaction_index BIGINT NOT NULL,
    receipt_root TEXT NOT NULL,
    status TEXT NOT NULL,
    verified_at_height BIGINT NOT NULL,
    verified_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (block_hash, transaction_index)
);

CREATE TABLE IF NOT EXISTS cosmos_verified_client_updates (
    client_id TEXT PRIMARY KEY,
    latest_height BIGINT NOT NULL,
    trust_level TEXT NOT NULL,
    verified_at TIMESTAMPTZ DEFAULT NOW()
);
