ALTER TABLE integration_connection
  ADD CONSTRAINT integration_connection_user_id_provider_kind_key UNIQUE (user_id, provider_kind);
