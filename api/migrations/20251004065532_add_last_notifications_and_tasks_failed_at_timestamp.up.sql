ALTER TABLE integration_connection
  ADD COLUMN last_notifications_sync_failed_at TIMESTAMP,
  ADD COLUMN last_tasks_sync_failed_at TIMESTAMP;
