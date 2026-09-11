DROP INDEX IF EXISTS notification_set_aside_at_idx;

ALTER TABLE notification
  DROP COLUMN set_aside_at;
