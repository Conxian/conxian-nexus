-- [Hole 3.1] Persistence for SRL-1 Lightning Recovery
CREATE TABLE IF NOT EXISTS lightning_payment_intents (
    payment_id TEXT PRIMARY KEY,
    payment_hash TEXT NOT NULL,
    amount_msat BIGINT NOT NULL,
    status TEXT NOT NULL, -- 'pending', 'succeeded', 'failed', 'recovering', 'mpp_splitting'
    failure_type TEXT, -- 'permanent', 'transient', 'indeterminate', 'mpp_partial'
    retry_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_lightning_payment_intents_status ON lightning_payment_intents(status);
CREATE INDEX IF NOT EXISTS idx_lightning_payment_intents_last_updated_at ON lightning_payment_intents(last_updated_at);

CREATE TABLE IF NOT EXISTS lightning_payment_events (
    event_id TEXT PRIMARY KEY,
    payment_id TEXT NOT NULL REFERENCES lightning_payment_intents(payment_id),
    status TEXT NOT NULL,
    failure_type TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata TEXT
);

CREATE INDEX IF NOT EXISTS idx_lightning_payment_events_payment_id ON lightning_payment_events(payment_id);
