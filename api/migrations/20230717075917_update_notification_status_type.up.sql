CREATE TYPE notification_status AS ENUM ('Unread', 'Read', 'Deleted', 'Unsubscribed');

ALTER TABLE notification
  ALTER COLUMN status
  SET DATA TYPE notification_status
  USING (
    CASE status::TEXT
      WHEN 'Unread'::TEXT THEN 'Unread'::NOTIFICATION_STATUS
      WHEN 'Read'::TEXT THEN 'Read'::NOTIFICATION_STATUS
      WHEN 'Deleted'::TEXT THEN 'Deleted'::NOTIFICATION_STATUS
      WHEN 'Unsubscribed'::TEXT THEN 'Unsubscribed'::NOTIFICATION_STATUS
      ELSE 'Unread'::NOTIFICATION_STATUS
    END
  );
