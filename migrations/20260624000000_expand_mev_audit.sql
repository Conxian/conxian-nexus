-- [Hole 4.1] Expand MEV Audit Log Detail
ALTER TABLE me_audit_log ADD COLUMN IF NOT EXISTS payload TEXT;
ALTER TABLE me_audit_log ADD COLUMN IF NOT EXISTS sequencing_priority INTEGER DEFAULT 0;
