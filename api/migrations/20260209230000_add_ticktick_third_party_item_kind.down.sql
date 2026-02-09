-- Remove 'TickTickItem' from third_party_item_kind enum
ALTER TABLE third_party_item DROP COLUMN kind;
DROP FUNCTION text_to_third_party_item_kind;

CREATE TYPE third_party_item_kind_new AS ENUM ('TodoistItem', 'LinearIssue', 'SlackStar', 'SlackReaction', 'LinearNotification', 'GithubNotification', 'GoogleMailThread', 'GoogleCalendarEvent', 'WebPage', 'SlackThread', 'GoogleDriveComment');

DROP TYPE third_party_item_kind;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;

CREATE FUNCTION text_to_third_party_item_kind(kind TEXT) RETURNS THIRD_PARTY_ITEM_KIND
AS $$
BEGIN RETURN kind::THIRD_PARTY_ITEM_KIND; END;
$$ LANGUAGE plpgsql IMMUTABLE;

ALTER TABLE third_party_item
ADD COLUMN kind THIRD_PARTY_ITEM_KIND GENERATED ALWAYS AS (text_to_third_party_item_kind(data ->> 'type')) STORED;

-- Remove 'TickTick' from notification_source_kind enum

CREATE TYPE notification_source_kind_new AS ENUM ('Todoist', 'Linear', 'Github', 'GoogleMail', 'Slack', 'GoogleCalendar', 'GoogleDrive', 'API');

ALTER TABLE notification
ALTER COLUMN source_kind TYPE notification_source_kind_new
USING (source_kind::text::notification_source_kind_new);

DROP TYPE notification_source_kind;
ALTER TYPE notification_source_kind_new RENAME TO notification_source_kind;

-- Remove 'TickTick' from task_kind enum
ALTER TABLE task DROP COLUMN kind;
DROP FUNCTION text_to_task_kind;

CREATE TYPE task_kind_new AS ENUM ('Todoist', 'Slack', 'Linear');

DROP TYPE task_kind;
ALTER TYPE task_kind_new RENAME TO task_kind;

CREATE FUNCTION text_to_task_kind(kind TEXT) RETURNS TASK_KIND
AS $$
BEGIN RETURN kind::TASK_KIND; END;
$$ LANGUAGE plpgsql IMMUTABLE;

ALTER TABLE task
ADD COLUMN kind TASK_KIND GENERATED ALWAYS AS (text_to_task_kind(metadata->>'type')) STORED;
