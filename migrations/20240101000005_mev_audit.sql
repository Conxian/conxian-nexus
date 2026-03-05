CREATE TABLE IF NOT EXISTS mev_audit_log (
    id SERIAL PRIMARY KEY,
    tx_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    reason TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_mev_audit_log_tx_id ON mev_audit_log(tx_id);
CREATE INDEX IF NOT EXISTS idx_mev_audit_log_sender ON mev_audit_log(sender);
