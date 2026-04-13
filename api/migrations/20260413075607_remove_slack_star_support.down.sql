-- Re-add SlackStar to third_party_item_kind enum
ALTER TABLE third_party_item
DROP COLUMN kind;

DROP FUNCTION text_to_third_party_item_kind;

CREATE TYPE third_party_item_kind_new AS ENUM (
  'TodoistItem',
  'SlackStar',
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

DROP TYPE third_party_item_kind;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;

CREATE FUNCTION text_to_third_party_item_kind(kind TEXT) RETURNS THIRD_PARTY_ITEM_KIND
IMMUTABLE
RETURN kind::THIRD_PARTY_ITEM_KIND;

ALTER TABLE third_party_item
ADD COLUMN kind THIRD_PARTY_ITEM_KIND GENERATED ALWAYS AS (text_to_third_party_item_kind(data ->> 'type')) STORED;

-- Restore default star_config to Slack integration connection configs
UPDATE integration_connection_config
SET config = jsonb_set(
    config::jsonb,
    '{content}',
    jsonb_set(
        config::jsonb -> 'content',
        '{star_config}',
        '{"sync_enabled": false, "sync_type": {"type": "AsNotifications"}}'::jsonb
    )
)::json
WHERE kind = 'Slack';
