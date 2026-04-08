-- [CON-162] External Settlement Proposals and Time-locks
CREATE TABLE IF NOT EXISTS settlement_proposals (
    proposal_id TEXT PRIMARY KEY,
    external_id TEXT NOT NULL,
    source TEXT NOT NULL, -- 'ISO20022', 'PAPSS', 'BRICS'
    payload JSONB NOT NULL,
    status TEXT NOT NULL, -- 'pending', 'active', 'executed', 'cancelled'
    init_height BIGINT NOT NULL,
    unlock_height BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_settlement_proposals_external_id ON settlement_proposals(external_id);
CREATE INDEX IF NOT EXISTS idx_settlement_proposals_unlock_height ON settlement_proposals(unlock_height) WHERE status = 'active';
