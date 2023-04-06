ALTER TABLE notification
  ADD COLUMN user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE;

CREATE INDEX notification_user_id_idx
  ON notification(user_id);

ALTER TABLE task
  ADD COLUMN user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE;

CREATE INDEX task_user_id_idx
  ON task(user_id);
