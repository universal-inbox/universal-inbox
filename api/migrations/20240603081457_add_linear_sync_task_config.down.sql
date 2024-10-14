CREATE TYPE task_kind_new AS ENUM ('Todoist', 'Slack');
DELETE FROM task
 WHERE kind IN ('Linear');
ALTER TABLE task
  ALTER COLUMN kind TYPE task_kind_new 
  USING (kind::text::task_kind_new);
DROP TYPE task_kind;
ALTER TYPE task_kind_new RENAME TO task_kind;

CREATE TYPE third_party_item_kind_new AS ENUM ('TodoistItem', 'SlackStar');

-- Delete tasks where source_item or sink_item has a LinearIssue kind
DELETE FROM task
  USING third_party_item
WHERE
  task.source_item_id = third_party_item.id
  AND third_party_item.kind = 'LinearIssue';

DELETE FROM third_party_item WHERE kind = 'LinearIssue';

ALTER TABLE third_party_item
  DROP COLUMN kind;

DROP FUNCTION text_to_third_party_item_kind;

DROP TYPE third_party_item_kind;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;

-- Create a cast function from TEXT to THIRD_PARTY_ITEM_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_third_party_item_kind(kind TEXT) RETURNS third_party_item_kind
  IMMUTABLE
  RETURN kind::third_party_item_kind;

ALTER TABLE third_party_item
  ADD COLUMN kind third_party_item_kind GENERATED ALWAYS AS (text_to_third_party_item_kind(data->>'type')) STORED;
