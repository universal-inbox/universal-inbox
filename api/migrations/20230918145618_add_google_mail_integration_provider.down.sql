CREATE TYPE integration_provider_kind_new AS ENUM ('Github', 'Todoist', 'Linear');

DELETE FROM integration_connection WHERE provider_kind = 'GoogleMail';

ALTER TABLE integration_connection
  ALTER COLUMN provider_kind TYPE integration_provider_kind_new 
  USING (provider_kind::text::integration_provider_kind_new);

DROP TYPE integration_provider_kind;
ALTER TYPE integration_provider_kind_new RENAME TO integration_provider_kind;
