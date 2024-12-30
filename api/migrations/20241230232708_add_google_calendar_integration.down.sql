CREATE TYPE integration_provider_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail', 'Slack', 'TickTick');

DELETE FROM integration_connection WHERE provider_kind = 'GoogleCalendar';

ALTER TABLE integration_connection
  ALTER COLUMN provider_kind TYPE integration_provider_kind_new
  USING (provider_kind::text::integration_provider_kind_new);

DROP TYPE integration_provider_kind;
ALTER TYPE integration_provider_kind_new RENAME TO integration_provider_kind;


DELETE FROM integration_connection_config WHERE kind = 'GoogleCalendar';

CREATE TYPE integration_connection_config_kind_new AS ENUM ('Github', 'Todoist', 'Linear', 'GoogleMail', 'Slack', 'TickTick');

ALTER TABLE integration_connection_config
  ALTER COLUMN kind TYPE integration_connection_config_kind_new
  USING (kind::text::integration_connection_config_kind_new);

DROP TYPE integration_connection_config_kind;
ALTER TYPE integration_connection_config_kind_new RENAME TO integration_connection_config_kind;

-- Revert the `third_party_item.kind` column
DELETE FROM third_party_item WHERE kind = 'GoogleCalendarEvent';
CREATE TYPE third_party_item_kind_new AS ENUM ('TodoistItem', 'LinearIssue', 'SlackStar', 'SlackReaction', 'LinearNotification', 'GithubNotification', 'GoogleMailThread');
ALTER TABLE third_party_item
DROP COLUMN kind;

DROP FUNCTION text_to_third_party_item_kind;

DROP TYPE THIRD_PARTY_ITEM_KIND;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;

-- Create a cast function from TEXT to THIRD_PARTY_ITEM_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_third_party_item_kind(kind TEXT) RETURNS THIRD_PARTY_ITEM_KIND
IMMUTABLE
RETURN kind::THIRD_PARTY_ITEM_KIND;

ALTER TABLE third_party_item
ADD COLUMN kind THIRD_PARTY_ITEM_KIND GENERATED ALWAYS AS (text_to_third_party_item_kind(data ->> 'type')) STORED;

-- Remove 'GoogleCalendar' from notification_source_kind enum
DELETE FROM notification WHERE source_kind = 'GoogleCalendar';
CREATE TYPE notification_source_kind_new AS ENUM ('Todoist', 'Linear', 'Github', 'GoogleMail', 'Slack');
ALTER TABLE notification
    ALTER COLUMN source_kind TYPE notification_source_kind_new
    USING (source_kind::text::notification_source_kind_new);

DROP TYPE notification_source_kind;
ALTER TYPE notification_source_kind_new RENAME TO notification_source_kind;

ALTER TABLE third_party_item
DROP COLUMN source_item_id;
