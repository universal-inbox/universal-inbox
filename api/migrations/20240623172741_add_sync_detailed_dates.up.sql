ALTER TABLE integration_connection
  RENAME COLUMN last_sync_started_at TO last_notifications_sync_started_at;
ALTER TABLE integration_connection
  RENAME COLUMN last_sync_failure_message TO last_notifications_sync_failure_message;
ALTER TABLE integration_connection
  RENAME COLUMN sync_failures TO notifications_sync_failures;
ALTER TABLE integration_connection
  ADD COLUMN last_notifications_sync_completed_at TIMESTAMP,
  ADD COLUMN last_tasks_sync_started_at TIMESTAMP,
  ADD COLUMN last_tasks_sync_completed_at TIMESTAMP,
  ADD COLUMN last_tasks_sync_failure_message TEXT,
  ADD COLUMN tasks_sync_failures INTEGER NOT NULL DEFAULT 0;

UPDATE integration_connection
SET 
  last_tasks_sync_started_at = last_notifications_sync_started_at,
  last_tasks_sync_failure_message = last_notifications_sync_failure_message,
  tasks_sync_failures = notifications_sync_failures,
  last_notifications_sync_started_at = NULL,
  last_notifications_sync_failure_message = NULL,
  notifications_sync_failures = 0
WHERE
  provider_kind = 'Todoist';
