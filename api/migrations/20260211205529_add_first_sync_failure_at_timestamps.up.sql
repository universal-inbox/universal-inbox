ALTER TABLE integration_connection
  ADD COLUMN first_notifications_sync_failed_at TIMESTAMP,
  ADD COLUMN first_tasks_sync_failed_at TIMESTAMP;

-- Backfill existing failure streaks: set first_failed_at to last_failed_at
-- for connections that already have active failure streaks
UPDATE integration_connection
SET first_notifications_sync_failed_at = last_notifications_sync_failed_at
WHERE notifications_sync_failures > 0
  AND last_notifications_sync_failed_at IS NOT NULL;

UPDATE integration_connection
SET first_tasks_sync_failed_at = last_tasks_sync_failed_at
WHERE tasks_sync_failures > 0
  AND last_tasks_sync_failed_at IS NOT NULL;
