ALTER TABLE oracle_fx_history ADD COLUMN IF NOT EXISTS confidence_intervals JSONB NOT NULL DEFAULT '{}';
