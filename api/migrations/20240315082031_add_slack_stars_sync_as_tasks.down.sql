UPDATE integration_connection_config
  SET config = '{"type": "Slack", "content": {"sync_stars_as_notifications": true}}'
  WHERE kind = 'Slack';
