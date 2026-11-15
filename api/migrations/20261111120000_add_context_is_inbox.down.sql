DROP INDEX IF EXISTS context_one_inbox_per_provider_idx;
ALTER TABLE context DROP COLUMN IF EXISTS is_inbox;
