ALTER TABLE notification
  DROP user_id;

DROP INDEX IF EXISTS notification_user_id_idx;

ALTER TABLE task
  DROP user_id;

DROP INDEX IF EXISTS task_user_id_idx;

