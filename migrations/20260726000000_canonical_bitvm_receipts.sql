CREATE TABLE IF NOT EXISTS canonical_bitvm_receipts (
    receipt_id TEXT PRIMARY KEY CHECK (receipt_id ~ '^[0-9a-f]{64}$'),
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    curve TEXT NOT NULL CHECK (curve = 'bn254'),
    circuit_id TEXT NOT NULL,
    verification_key_id TEXT NOT NULL CHECK (verification_key_id ~ '^[0-9a-f]{64}$'),
    statement_hash TEXT NOT NULL CHECK (statement_hash ~ '^[0-9a-f]{64}$'),
    proof_digest TEXT NOT NULL CHECK (proof_digest ~ '^[0-9a-f]{64}$'),
    previous_state_root TEXT NOT NULL CHECK (previous_state_root ~ '^[0-9a-f]{64}$'),
    next_state_root TEXT NOT NULL CHECK (next_state_root ~ '^[0-9a-f]{64}$'),
    witness_commitment TEXT NOT NULL CHECK (witness_commitment ~ '^[0-9a-f]{64}$'),
    bitcoin_network TEXT NOT NULL CHECK (bitcoin_network IN ('mainnet', 'testnet', 'signet', 'regtest')),
    anchor_block_height BIGINT NOT NULL CHECK (anchor_block_height > 0),
    anchor_block_hash TEXT NOT NULL CHECK (anchor_block_hash ~ '^[0-9a-f]{64}$'),
    max_valid_height BIGINT CHECK (max_valid_height IS NULL OR max_valid_height >= anchor_block_height),
    backend_identity TEXT NOT NULL,
    backend_version TEXT NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL,
    UNIQUE (statement_hash, proof_digest)
);

CREATE INDEX IF NOT EXISTS canonical_bitvm_receipts_statement_hash_idx
    ON canonical_bitvm_receipts (statement_hash);
CREATE INDEX IF NOT EXISTS canonical_bitvm_receipts_vk_id_idx
    ON canonical_bitvm_receipts (verification_key_id);
CREATE INDEX IF NOT EXISTS canonical_bitvm_receipts_verified_at_idx
    ON canonical_bitvm_receipts (verified_at DESC);
