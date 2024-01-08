UPDATE
  integration_connection_config
SET config = jsonb_set(config::jsonb, ARRAY['content', 'create_notification_from_inbox_task'], 'true', TRUE)
WHERE kind = 'Todoist';
