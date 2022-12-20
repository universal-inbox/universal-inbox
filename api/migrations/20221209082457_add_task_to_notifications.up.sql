ALTER TABLE notification
  ADD COLUMN task_id UUID;

ALTER TABLE notification
  ADD FOREIGN KEY (task_id) REFERENCES task(id);

ALTER TABLE notification
  ADD COLUMN task_source_id TEXT;

CREATE INDEX notification_task_id_idx ON notification(task_id);
