-- Fail-closed consume-once idempotency records for the Nexus delivery runtime.
--
-- The unique primary key is the atomic conditional-write primitive
-- (equivalent to `INSERT ... ON CONFLICT DO NOTHING`), giving at-most-once
-- effect execution across replicas and restarts.
CREATE TABLE IF NOT EXISTS idempotency_records (
    idempotency_key TEXT PRIMARY KEY CHECK (char_length(idempotency_key) BETWEEN 1 AND 512),
    operation TEXT NOT NULL CHECK (char_length(operation) BETWEEN 1 AND 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ CHECK (expires_at IS NULL OR expires_at > created_at)
);

CREATE INDEX IF NOT EXISTS idempotency_records_expires_at_idx
    ON idempotency_records (expires_at);
