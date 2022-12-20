DROP INDEX notification_task_id_idx;

ALTER TABLE notification
  DROP COLUMN task_id CASCADE;

ALTER TABLE notification
  DROP COLUMN task_source_id;
