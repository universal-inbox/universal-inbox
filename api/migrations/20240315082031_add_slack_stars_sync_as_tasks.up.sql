UPDATE integration_connection_config
  SET config = '{"type": "Slack", "content": {"sync_enabled": true, "sync_type": {"type": "AsNotifications"}}}'
  WHERE kind = 'Slack';
