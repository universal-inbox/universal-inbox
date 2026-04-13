-- Delete all SlackStar third-party items (cascades handle notification/task cleanup)
DELETE FROM third_party_item WHERE kind = 'SlackStar';

-- Recreate third_party_item_kind enum without SlackStar
CREATE TYPE third_party_item_kind_new AS ENUM (
  'TodoistItem',
  'SlackReaction',
  'SlackThread',
  'LinearIssue',
  'LinearNotification',
  'GithubNotification',
  'GoogleMailThread',
  'GoogleCalendarEvent',
  'WebPage',
  'GoogleDriveComment'
);

ALTER TABLE third_party_item
DROP COLUMN kind;

DROP FUNCTION text_to_third_party_item_kind;

DROP TYPE third_party_item_kind;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;

CREATE FUNCTION text_to_third_party_item_kind(kind TEXT) RETURNS THIRD_PARTY_ITEM_KIND
IMMUTABLE
RETURN kind::THIRD_PARTY_ITEM_KIND;

ALTER TABLE third_party_item
ADD COLUMN kind THIRD_PARTY_ITEM_KIND GENERATED ALWAYS AS (text_to_third_party_item_kind(data ->> 'type')) STORED;

-- Strip star_config from Slack integration connection configs
UPDATE integration_connection_config
SET config = jsonb_set(
    config::jsonb,
    '{content}',
    (config::jsonb -> 'content') - 'star_config'
)::json
WHERE kind = 'Slack';
