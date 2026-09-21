-- Fail-closed anti-rollback high-water clock for the Nexus delivery runtime.
--
-- Each clock namespace stores the largest observed wall-clock second. A
-- consume-time observation that would move the clock backwards is rejected
-- (ClockRollback), preventing an attacker or a misconfigured replica from
-- replaying an old time window to bypass idempotency.
CREATE TABLE IF NOT EXISTS idempotency_high_water (
    clock TEXT PRIMARY KEY CHECK (char_length(clock) BETWEEN 1 AND 128),
    last_observed_secs BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
