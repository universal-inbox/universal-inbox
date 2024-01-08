UPDATE
  integration_connection_config
SET
  config = config::jsonb #- '{content,create_notification_from_inbox_task}'
WHERE
  kind = 'Todoist';
