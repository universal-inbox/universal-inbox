CREATE TYPE third_party_item_kind AS ENUM (
  'TodoistItem',
  'SlackStar'
);
ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'Slack';

-- Create a cast function from TEXT to THIRD_PARTY_ITEM_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_third_party_item_kind(kind TEXT) RETURNS THIRD_PARTY_ITEM_KIND
  IMMUTABLE
  RETURN kind::THIRD_PARTY_ITEM_KIND;

CREATE TABLE third_party_item(
  id UUID NOT NULL,
  PRIMARY KEY (id),
  source_id TEXT NOT NULL,
  kind THIRD_PARTY_ITEM_KIND GENERATED ALWAYS AS (text_to_third_party_item_kind(data->>'type')) STORED,
  CONSTRAINT third_party_item_source_id_kind_integration_connection_id_key UNIQUE (source_id, kind, integration_connection_id),
  data JSON NOT NULL,
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL,
  user_id UUID NOT NULL,
  FOREIGN KEY (user_id) REFERENCES "user"(id),
  integration_connection_id UUID NOT NULL,
  FOREIGN KEY (integration_connection_id) REFERENCES integration_connection(id)
);

CREATE INDEX third_party_item_source_id_kind_idx
  ON third_party_item(source_id, kind);

-- Migrate Task data to ThirdPartyItem
INSERT INTO
  third_party_item(id, source_id, data, created_at, updated_at, user_id, integration_connection_id)
SELECT
  gen_random_uuid(),
  source_id,
  json_build_object('type', 'TodoistItem', 'content', task.metadata->'content'),
  task.created_at,
  task.created_at,
  task.user_id,
  ic.id
FROM task
JOIN integration_connection ic ON ic.user_id = task.user_id AND ic.provider_kind = 'Todoist';

-- Add foreign key columns to third_party_item to the task table
ALTER TABLE task
  ADD COLUMN source_item_id UUID,
  ADD CONSTRAINT fk_source_item FOREIGN KEY(source_item_id) REFERENCES third_party_item(id),
  ADD CONSTRAINT source_item_id_key UNIQUE(source_item_id),
  ADD COLUMN sink_item_id UUID,
  ADD CONSTRAINT fk_sink_item FOREIGN KEY(sink_item_id) REFERENCES third_party_item(id),
  ADD CONSTRAINT sink_item_id_key UNIQUE(sink_item_id),
  ADD COLUMN kind_tmp TASK_KIND;

-- Copy generated column values `kind` into `kind_tmp`
UPDATE
  task
SET
  kind_tmp = task.kind,
  source_item_id = third_party_item.id,
  sink_item_id = third_party_item.id
FROM third_party_item
WHERE third_party_item.source_id = task.source_id;

-- Remove deprecated indexes
DROP INDEX task_source_id_kind_user_id_idx;

-- Remove deprecated Task columns and constraints
ALTER TABLE task
  DROP COLUMN kind;
ALTER TABLE task
  ALTER COLUMN source_item_id SET NOT NULL,
  DROP COLUMN metadata,
  DROP COLUMN source_id;
ALTER TABLE task
  RENAME COLUMN kind_tmp TO kind;
ALTER TABLE task
  ADD COLUMN updated_at TIMESTAMP NOT NULL DEFAULT NOW();
