ALTER TABLE notification
  ADD snoozed_until TIMESTAMP;

CREATE INDEX notification_snoozed_until_idx
  ON notification(snoozed_until);
