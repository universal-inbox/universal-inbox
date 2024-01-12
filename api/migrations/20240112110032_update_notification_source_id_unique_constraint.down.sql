DROP INDEX IF EXISTS notification_source_id_kind_user_id_idx;

ALTER TABLE notification
  DROP CONSTRAINT IF EXISTS notification_source_id_kind_user_id_key;

ALTER TABLE notification
  ADD UNIQUE (source_id, kind);

CREATE INDEX notification_source_id_kind_idx
  ON notification(source_id, kind);

DROP INDEX IF EXISTS task_source_id_kind_user_id_idx;

ALTER TABLE task
  DROP CONSTRAINT IF EXISTS task_source_id_kind_user_id_key;

ALTER TABLE task
  ADD UNIQUE (source_id, kind);

CREATE INDEX task_source_id_kind_idx
  ON task(source_id, kind);
