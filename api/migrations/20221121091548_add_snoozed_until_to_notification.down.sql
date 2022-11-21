ALTER TABLE notification
  DROP snoozed_until;

DROP INDEX IF EXISTS notification_snoozed_until_idx;
