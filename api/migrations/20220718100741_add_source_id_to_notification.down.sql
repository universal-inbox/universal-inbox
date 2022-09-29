ALTER TABLE notification
  DROP source_id;

DROP INDEX IF EXISTS notification_source_id_idx;
