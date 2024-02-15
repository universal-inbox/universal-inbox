ALTER TYPE integration_provider_kind ADD VALUE IF NOT EXISTS 'Slack';

ALTER TYPE integration_connection_config_kind ADD VALUE IF NOT EXISTS 'Slack';

ALTER TABLE integration_connection
  ADD provider_user_id TEXT;

CREATE INDEX integration_connection_provider_user_id_idx
  ON integration_connection(provider_user_id);
