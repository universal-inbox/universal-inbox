ALTER TABLE notification
  DROP kind;

ALTER TABLE notification
  ADD kind TEXT;

UPDATE notification
  SET kind = metadata->>'type';

ALTER TABLE notification
  ALTER COLUMN kind SET NOT NULL;

DROP INDEX IF EXISTS notification_source_id_kind_idx;

ALTER TABLE notification
  DROP CONSTRAINT IF EXISTS notification_source_id_kind_key;

ALTER TABLE notification
  ADD UNIQUE (source_id);

CREATE INDEX notification_source_id_idx
  ON notification(source_id);

