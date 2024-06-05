CREATE TYPE task_kind_new AS ENUM ('Todoist', 'Slack');
DELETE FROM task
 WHERE kind IN ('Linear');
ALTER TABLE task
  ALTER COLUMN kind TYPE task_kind_new 
  USING (kind::text::task_kind_new);
DROP TYPE task_kind;
ALTER TYPE task_kind_new RENAME TO task_kind;

CREATE TYPE third_party_item_kind_new AS ENUM ('TodoistItem', 'SlackStar');
DELETE FROM third_party_item
  WHERE kind IN ('LinearIssue');
ALTER TABLE third_party_item
  ALTER COLUMN kind TYPE third_party_item_kind_new 
  USING (kind::text::third_party_item_kind_new);
DROP TYPE third_party_item_kind;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;
