UPDATE integration_connection
SET
  last_notifications_sync_started_at = last_tasks_sync_started_at,
  last_notifications_sync_failure_message = last_tasks_sync_failure_message,
  notifications_sync_failures = tasks_sync_failures
WHERE
  provider_kind = 'Todoist';

ALTER TABLE integration_connection
  DROP COLUMN tasks_sync_failures,
  DROP COLUMN last_tasks_sync_failure_message,
  DROP COLUMN last_tasks_sync_completed_at,
  DROP COLUMN last_tasks_sync_started_at,
  DROP COLUMN last_notifications_sync_completed_at;
ALTER TABLE integration_connection
  RENAME COLUMN notifications_sync_failures TO sync_failures;
ALTER TABLE integration_connection
  RENAME COLUMN last_notifications_sync_failure_message TO last_sync_failure_message;
ALTER TABLE integration_connection
  RENAME COLUMN last_notifications_sync_started_at TO last_sync_started_at;

