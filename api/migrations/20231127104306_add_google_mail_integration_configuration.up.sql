CREATE TYPE integration_connection_config_kind AS ENUM (
  'GoogleMail',
  'Github',
  'Linear',
  'Todoist'
);

-- Create a cast function from TEXT to INTEGRATION_CONNECTION_CONFIG_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_integration_connection_config_kind(kind TEXT) RETURNS INTEGRATION_CONNECTION_CONFIG_KIND
  IMMUTABLE
  RETURN kind::INTEGRATION_CONNECTION_CONFIG_KIND;

-- Create integration connection config table
CREATE TABLE integration_connection_config(
  id UUID NOT NULL,
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL,
  PRIMARY KEY (id),
  integration_connection_id UUID NOT NULL UNIQUE,
  FOREIGN KEY (integration_connection_id) REFERENCES integration_connection(id),
  kind INTEGRATION_CONNECTION_CONFIG_KIND GENERATED ALWAYS AS (text_to_integration_connection_config_kind(config->>'type')) STORED,
  config JSON NOT NULL
);

CREATE INDEX integration_connection_config_integration_connection_id_idx ON integration_connection_config(integration_connection_id);

INSERT INTO
  integration_connection_config (id, created_at, updated_at, integration_connection_id, config)
SELECT
  gen_random_uuid(),
  CURRENT_TIMESTAMP,
  CURRENT_TIMESTAMP,
  id,
  '{"type": "GoogleMail", "content": {"sync_notifications_enabled": true, "synced_label": {"id": "STARRED", "name": "STARRED"}}}'
FROM
  integration_connection
WHERE
  provider_kind = 'GoogleMail';

INSERT INTO
  integration_connection_config (id, created_at, updated_at, integration_connection_id, config)
SELECT
  gen_random_uuid(),
  CURRENT_TIMESTAMP,
  CURRENT_TIMESTAMP,
  id,
  '{"type": "Github", "content": {"sync_notifications_enabled": true}}'
FROM
  integration_connection
WHERE
  provider_kind = 'Github';

INSERT INTO
  integration_connection_config (id, created_at, updated_at, integration_connection_id, config)
SELECT
  gen_random_uuid(),
  CURRENT_TIMESTAMP,
  CURRENT_TIMESTAMP,
  id,
  '{"type": "Linear", "content": {"sync_notifications_enabled": true}}'
FROM
  integration_connection
WHERE
  provider_kind = 'Linear';

INSERT INTO
  integration_connection_config (id, created_at, updated_at, integration_connection_id, config)
SELECT
  gen_random_uuid(),
  CURRENT_TIMESTAMP,
  CURRENT_TIMESTAMP,
  id,
  '{"type": "Todoist", "content": {"sync_tasks_enabled": true}}'
FROM
  integration_connection
WHERE
  provider_kind = 'Todoist';
