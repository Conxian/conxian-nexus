-- Audit trail for Fedimint e-cash blinded mint proof verification and double-spend tracking
CREATE TABLE IF NOT EXISTS fedimint_verified_proofs (
    proof_hash VARCHAR(64) PRIMARY KEY,
    federation_id VARCHAR(255) NOT NULL,
    amount_sats BIGINT NOT NULL,
    nonce_hash VARCHAR(64) NOT NULL UNIQUE,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status VARCHAR(50) NOT NULL DEFAULT 'verified'
);

CREATE INDEX IF NOT EXISTS idx_fedimint_verified_proofs_federation ON fedimint_verified_proofs(federation_id);
CREATE INDEX IF NOT EXISTS idx_fedimint_verified_proofs_nonce ON fedimint_verified_proofs(nonce_hash);
