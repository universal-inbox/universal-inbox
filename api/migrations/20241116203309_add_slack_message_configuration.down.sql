CREATE TYPE third_party_item_kind_new AS ENUM (
    'TodoistItem',
    'LinearIssue',
    'SlackStar',
    'SlackReaction',
    'LinearNotification',
    'GithubNotification',
    'GoogleMailThread'
);

DELETE FROM notification
  USING third_party_item
WHERE
  notification.source_item_id = third_party_item.id
  AND third_party_item.kind = 'SlackThread';

DELETE FROM third_party_item WHERE kind = 'SlackThread';

-- Revert the `third_party_item.kind` column
ALTER TABLE third_party_item
DROP COLUMN kind;

DROP FUNCTION text_to_third_party_item_kind;

DROP TYPE THIRD_PARTY_ITEM_KIND;
ALTER TYPE third_party_item_kind_new RENAME TO third_party_item_kind;

CREATE FUNCTION text_to_third_party_item_kind(
    kind TEXT
) RETURNS THIRD_PARTY_ITEM_KIND
IMMUTABLE
RETURN kind::THIRD_PARTY_ITEM_KIND;

ALTER TABLE third_party_item
ADD COLUMN kind THIRD_PARTY_ITEM_KIND GENERATED ALWAYS AS (
    text_to_third_party_item_kind(data ->> 'type')
) STORED;

UPDATE
    integration_connection_config
SET
    config = jsonb_set(
        config::JSONB,
        '{content}',
        json_build_object(
            'star_config', (config -> 'content' -> 'star_config')::JSONB,
            'reaction_config', (config -> 'content' -> 'reaction_config')::JSONB
        )::JSONB
    )::JSON
WHERE
    kind = 'Slack';
