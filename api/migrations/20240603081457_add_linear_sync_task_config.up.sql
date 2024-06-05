ALTER TYPE task_kind ADD VALUE IF NOT EXISTS 'Linear';
ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'LinearIssue';

UPDATE
  integration_connection_config
SET
  config = jsonb_set(config::jsonb, '{content,sync_task_config}', '{"enabled": true}'::jsonb, TRUE)::json
WHERE
  kind = 'Linear';
