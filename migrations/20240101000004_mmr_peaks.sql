CREATE TABLE IF NOT EXISTS mmr_peaks (
    block_height BIGINT PRIMARY KEY,
    peaks BYTEA[] NOT NULL,
    size BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
