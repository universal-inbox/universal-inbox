ALTER TABLE integration_connection
  DROP COLUMN last_notifications_sync_failed_at,
  DROP COLUMN last_tasks_sync_failed_at;
