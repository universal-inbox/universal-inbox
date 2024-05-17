CREATE TYPE task_kind_new AS ENUM ('Todoist');

DELETE FROM task
  WHERE kind IN ('Slack');

ALTER TABLE task
  ALTER COLUMN kind TYPE task_kind_new 
  USING (kind::text::task_kind_new);

DROP TYPE task_kind;
ALTER TYPE task_kind_new RENAME TO task_kind;

DROP INDEX third_party_item_source_id_kind_idx;

DROP TABLE third_party_item;

DROP FUNCTION text_to_third_party_item_kind;

DROP TYPE third_party_item_kind;

ALTER TABLE task
  DROP COLUMN updated_at;
