ALTER TABLE stacks_transactions ADD COLUMN sender TEXT;
CREATE INDEX IF NOT EXISTS idx_stacks_transactions_sender ON stacks_transactions(sender);
