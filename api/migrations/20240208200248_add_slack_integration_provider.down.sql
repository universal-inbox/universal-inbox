CREATE TYPE integration_provider_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail');

DELETE FROM integration_connection WHERE provider_kind = 'Slack';

ALTER TABLE integration_connection
  ALTER COLUMN provider_kind TYPE integration_provider_kind_new 
  USING (provider_kind::text::integration_provider_kind_new);

DROP TYPE integration_provider_kind;
ALTER TYPE integration_provider_kind_new RENAME TO integration_provider_kind;

ALTER TABLE integration_connection
  DROP provider_user_id;

DROP INDEX IF EXISTS integration_connection_provider_user_id_idx;
