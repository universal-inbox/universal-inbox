-- A notification whose integration stopped feeding the inbox (muted, and later
-- disconnected) is set aside rather than removed: it leaves the inbox and every
-- count, keeping its status, `last_read_at`, `snoozed_until` and `task_id`
-- untouched, and comes back as it was once the integration feeds again.
--
-- Additive and backfilling nothing: every existing notification starts visible.
ALTER TABLE notification
  ADD COLUMN set_aside_at TIMESTAMPTZ DEFAULT NULL;

CREATE INDEX notification_set_aside_at_idx
  ON notification(set_aside_at);
