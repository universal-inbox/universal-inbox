DELETE FROM notification WHERE kind = 'Notion';
DELETE FROM third_party_item WHERE kind IN ('NotionComment', 'NotionBlock');

-- Remove 'NotionComment' and 'NotionBlock' from third_party_item_kind.
-- `third_party_item.kind` is written by the application (no longer generated),
-- so a cast is enough; the unused `text_to_third_party_item_kind` function
-- depends on the type and is recreated on the new one.
DROP FUNCTION text_to_third_party_item_kind;

CREATE TYPE third_party_item_kind_new AS ENUM ('TodoistItem', 'SlackReaction', 'SlackThread', 'LinearIssue', 'LinearNotification', 'GithubNotification', 'GoogleMailThread', 'GoogleCalendarEvent', 'WebPage', 'GoogleDriveComment', 'TickTickItem');

ALTER TABLE third_party_item
  ALTER COLUMN kind TYPE third_party_item_kind_new
  USING (kind::text::third_party_item_kind_new);

DROP TYPE third_party_item_kind;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;

CREATE FUNCTION text_to_third_party_item_kind(kind TEXT) RETURNS THIRD_PARTY_ITEM_KIND
AS $$
BEGIN RETURN kind::THIRD_PARTY_ITEM_KIND; END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Remove 'Notion' from notification_source_kind.
CREATE TYPE notification_source_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail', 'Slack', 'GoogleCalendar', 'API', 'GoogleDrive', 'TickTick');

ALTER TABLE notification
  ALTER COLUMN kind TYPE notification_source_kind_new
  USING (kind::text::notification_source_kind_new);

DROP TYPE notification_source_kind;
ALTER TYPE notification_source_kind_new RENAME TO notification_source_kind;
