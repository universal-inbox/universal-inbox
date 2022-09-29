ALTER TABLE notification
  ADD source_id TEXT NOT NULL UNIQUE;

CREATE INDEX notification_source_id_idx
  ON notification(source_id);
