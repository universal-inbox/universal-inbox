CREATE TYPE integration_provider_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail', 'Slack');

DELETE FROM integration_connection WHERE provider_kind = 'TickTick';

ALTER TABLE integration_connection
  ALTER COLUMN provider_kind TYPE integration_provider_kind_new 
  USING (provider_kind::text::integration_provider_kind_new);

DROP TYPE integration_provider_kind;
ALTER TYPE integration_provider_kind_new RENAME TO integration_provider_kind;

CREATE TYPE integration_connection_config_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail', 'Slack');

DELETE FROM integration_connection WHERE kind = 'TickTick';

ALTER TABLE integration_connection
  ALTER COLUMN integration_connection_config_kind TYPE integration_connection_config_kind_new 
  USING (integration_connection_config_kind::text::integration_connection_config_kind_new);

DROP TYPE integration_connection_config_kind;
ALTER TYPE integration_connection_config_kind_new RENAME TO integration_connection_config_kind;
