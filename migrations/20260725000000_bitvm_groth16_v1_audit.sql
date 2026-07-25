-- CON-1533: canonical Gateway-v1-compatible BN254 transition audit.
-- Legacy bitvm_verified_transitions data is intentionally untouched.
CREATE TABLE IF NOT EXISTS bitvm_groth16_v1_audit (
    statement_hash TEXT PRIMARY KEY,
    schema_version INTEGER NOT NULL,
    curve TEXT NOT NULL,
    circuit_id TEXT NOT NULL,
    verification_key_id TEXT NOT NULL,
    prev_state_root TEXT NOT NULL,
    next_state_root TEXT NOT NULL,
    public_inputs_hash TEXT NOT NULL,
    proof_digest TEXT NOT NULL,
    witness_commitment TEXT NOT NULL,
    steps_verified BIGINT NOT NULL,
    bitcoin_network TEXT NOT NULL,
    bitcoin_anchor_height BIGINT NOT NULL,
    bitcoin_anchor_hash TEXT NOT NULL,
    bitcoin_max_valid_height BIGINT,
    trace_id TEXT,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT bitvm_groth16_v1_schema CHECK (schema_version = 1),
    CONSTRAINT bitvm_groth16_v1_curve CHECK (curve = 'bn254'),
    CONSTRAINT bitvm_groth16_v1_circuit CHECK (
        circuit_id = 'conxian-nexus-bitvm-state-transition-v1'
    )
);

CREATE INDEX IF NOT EXISTS bitvm_groth16_v1_verified_at_idx
    ON bitvm_groth16_v1_audit (verified_at DESC);
