ALTER TABLE integration_connection
  ADD COLUMN registered_oauth_scopes JSON NOT NULL DEFAULT '[]'::json;
