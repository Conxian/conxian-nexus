-- [CON-161][CON-164] External Settlement Logs
-- Extends the persistence layer to record external settlement events without mutating primary on-chain trackers.

CREATE TABLE IF NOT EXISTS cxn_external_settlement_logs (
    log_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    external_tx_reference TEXT NOT NULL,
    settlement_network_origin TEXT NOT NULL, -- 'ISO20022', 'PAPSS', 'BRICS'
    fiat_value_pegged NUMERIC(20, 8),
    native_tx_hash TEXT, -- Link to native transaction if available
    raw_payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cxn_ext_settlement_ref ON cxn_external_settlement_logs(external_tx_reference);
CREATE INDEX IF NOT EXISTS idx_cxn_ext_settlement_native ON cxn_external_settlement_logs(native_tx_hash);
