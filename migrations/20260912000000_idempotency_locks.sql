-- Distributed transactional idempotency locks for the Nexus delivery runtime (#251).
--
-- Supports atomic lock acquisition, owner-based lock extension/release, TTL expiration,
-- and optional payload persistence across distributed replicas.

CREATE TABLE IF NOT EXISTS idempotency_locks (
    key VARCHAR(512) PRIMARY KEY,
    owner VARCHAR(256) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    payload JSONB
);

CREATE INDEX IF NOT EXISTS idempotency_locks_expires_at_idx
    ON idempotency_locks (expires_at);
