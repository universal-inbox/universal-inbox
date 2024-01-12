-- Dropping index and unique constraint on (`source_id`, `kind`)
DROP INDEX IF EXISTS notification_source_id_kind_idx;

ALTER TABLE notification
  DROP CONSTRAINT IF EXISTS notification_source_id_kind_key;

-- And replace it by new constraint and index on (`source_id`, `kind`, `user_id`)
ALTER TABLE notification
  ADD CONSTRAINT notification_source_id_kind_user_id_key UNIQUE (source_id, kind, user_id);

CREATE INDEX notification_source_id_kind_user_id_idx
  ON notification(source_id, kind, user_id);

-- Dropping index and unique constraint on (`source_id`, `kind`)
DROP INDEX IF EXISTS task_source_id_kind_idx;

ALTER TABLE task
  DROP CONSTRAINT IF EXISTS task_source_id_kind_key;

-- And replace it by new constraint and index on (`source_id`, `kind`, `user_id`)
ALTER TABLE task
  ADD CONSTRAINT task_source_id_kind_user_id_key UNIQUE (source_id, kind, user_id);

CREATE INDEX task_source_id_kind_user_id_idx
  ON task(source_id, kind, user_id);
