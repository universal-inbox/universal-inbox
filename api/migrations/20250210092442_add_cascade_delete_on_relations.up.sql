-- Add foreign key "on delete" constraints for `user`
ALTER TABLE authentication_token
  DROP CONSTRAINT authentication_token_user_id_fkey,
  ADD CONSTRAINT authentication_token_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES "user"(id) ON DELETE CASCADE;

ALTER TABLE third_party_item
  DROP CONSTRAINT third_party_item_user_id_fkey,
  ADD CONSTRAINT third_party_item_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES "user"(id) ON DELETE CASCADE;

ALTER TABLE user_auth
  DROP CONSTRAINT user_auth_user_id_fkey,
  ADD CONSTRAINT user_auth_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES "user"(id) ON DELETE CASCADE;

-- Add foreign key "on delete" constraints for `integration_connection`
ALTER TABLE integration_connection
  ADD CONSTRAINT integration_connection_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES "user"(id) ON DELETE CASCADE;

ALTER TABLE integration_connection_config
  DROP CONSTRAINT integration_connection_config_integration_connection_id_fkey,
  ADD CONSTRAINT integration_connection_config_integration_connection_id_fkey
    FOREIGN KEY (integration_connection_id) REFERENCES integration_connection(id) ON DELETE CASCADE;

ALTER TABLE third_party_item
  DROP CONSTRAINT third_party_item_integration_connection_id_fkey,
  ADD CONSTRAINT third_party_item_integration_connection_id_fkey
    FOREIGN KEY (integration_connection_id) REFERENCES integration_connection(id) ON DELETE CASCADE;

-- Add foreign key "on delete" constraints for `third_party_item`
ALTER TABLE notification
  DROP CONSTRAINT fk_source_item,
  ADD CONSTRAINT fk_source_item
    FOREIGN KEY (source_item_id) REFERENCES third_party_item(id) ON DELETE CASCADE;

ALTER TABLE task
  DROP CONSTRAINT fk_source_item,
  ADD CONSTRAINT fk_source_item
    FOREIGN KEY (source_item_id) REFERENCES third_party_item(id) ON DELETE CASCADE;

ALTER TABLE task
  DROP CONSTRAINT fk_sink_item,
  ADD CONSTRAINT fk_sink_item
    FOREIGN KEY (sink_item_id) REFERENCES third_party_item(id) ON DELETE SET NULL (sink_item_id);

ALTER TABLE third_party_item
  DROP CONSTRAINT fk_source_item,
  ADD CONSTRAINT fk_source_item
    FOREIGN KEY (source_item_id) REFERENCES third_party_item(id) ON DELETE CASCADE;

-- Add foreign key "on delete" constraints for `task`
ALTER TABLE notification
  DROP CONSTRAINT notification_task_id_fkey,
  ADD CONSTRAINT notification_task_id_fkey
    FOREIGN KEY (task_id) REFERENCES task(id) ON DELETE SET NULL (task_id);

ALTER TABLE task
  DROP CONSTRAINT fk_parent,
  ADD CONSTRAINT fk_parent
    FOREIGN KEY (parent_id) REFERENCES task(id) ON DELETE SET NULL (parent_id);
