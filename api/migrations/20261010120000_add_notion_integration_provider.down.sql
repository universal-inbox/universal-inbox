DELETE FROM oauth_grant_revocation WHERE provider_kind = 'Notion';
DELETE FROM integration_connection WHERE provider_kind = 'Notion';

CREATE TYPE integration_provider_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail', 'Slack', 'TickTick', 'GoogleCalendar', 'API', 'GoogleDrive');

ALTER TABLE integration_connection
  ALTER COLUMN provider_kind TYPE integration_provider_kind_new
  USING (provider_kind::text::integration_provider_kind_new);

ALTER TABLE oauth_grant_revocation
  ALTER COLUMN provider_kind TYPE integration_provider_kind_new
  USING (provider_kind::text::integration_provider_kind_new);

DROP TYPE integration_provider_kind;
ALTER TYPE integration_provider_kind_new RENAME TO integration_provider_kind;

-- `integration_connection_config.kind` is generated from the JSON config, and
-- a generated column cannot change type: drop it and recreate it on the new type.
DELETE FROM integration_connection_config WHERE kind = 'Notion';

ALTER TABLE integration_connection_config DROP COLUMN kind;
DROP FUNCTION text_to_integration_connection_config_kind;

CREATE TYPE integration_connection_config_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail', 'Slack', 'TickTick', 'GoogleCalendar', 'API', 'GoogleDrive');

DROP TYPE integration_connection_config_kind;
ALTER TYPE integration_connection_config_kind_new RENAME TO integration_connection_config_kind;

-- Create a cast function from TEXT to INTEGRATION_CONNECTION_CONFIG_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_integration_connection_config_kind(kind TEXT) RETURNS INTEGRATION_CONNECTION_CONFIG_KIND
  IMMUTABLE
  RETURN kind::INTEGRATION_CONNECTION_CONFIG_KIND;

ALTER TABLE integration_connection_config
  ADD COLUMN kind INTEGRATION_CONNECTION_CONFIG_KIND GENERATED ALWAYS AS (text_to_integration_connection_config_kind(config->>'type')) STORED;
