ALTER TABLE notification
  ALTER COLUMN status
  SET DATA TYPE TEXT;

DROP TYPE notification_status;
