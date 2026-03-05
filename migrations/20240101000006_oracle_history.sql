CREATE TABLE IF NOT EXISTS oracle_fx_history (
    id SERIAL PRIMARY KEY,
    base_currency TEXT NOT NULL,
    rates JSONB NOT NULL,
    ppp_indices JSONB NOT NULL,
    timestamp BIGINT NOT NULL,
    tx_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oracle_fx_history_timestamp ON oracle_fx_history(timestamp);
