-- [CON-1029] Multi-Chain Verification Audit Logs

CREATE TABLE IF NOT EXISTS bitvm_verified_transitions (
    trace_id TEXT PRIMARY KEY,
    prev_state_root TEXT NOT NULL,
    next_state_root TEXT NOT NULL,
    proof_hash TEXT NOT NULL,
    steps_verified BIGINT NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    verified_at TIMESTAMPTZ DEFAULT NOW()
);

-- Fix table name mismatch between code and earlier migrations for MEV audit
CREATE TABLE IF NOT EXISTS me_audit_log (
    tx_id TEXT PRIMARY KEY,
    payload_hash TEXT NOT NULL,
    sender TEXT NOT NULL,
    arrival_time TIMESTAMPTZ NOT NULL
);
