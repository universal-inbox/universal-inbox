-- `kind` will be auto generated from the JSON from `metadata`
ALTER TABLE notification
  DROP kind;

ALTER TABLE notification
  ADD kind TEXT GENERATED ALWAYS AS (metadata->>'type') STORED;

-- Dropping index and unique constraint on `source_id`
DROP INDEX IF EXISTS notification_source_id_idx;

ALTER TABLE notification
  DROP CONSTRAINT IF EXISTS notification_source_id_key;

-- And replace it by new constraint and index on (`source_id`, `kind`)
ALTER TABLE notification
  ADD CONSTRAINT notification_source_id_kind_key UNIQUE (source_id, kind);

CREATE INDEX notification_source_id_kind_idx
  ON notification(source_id, kind);
