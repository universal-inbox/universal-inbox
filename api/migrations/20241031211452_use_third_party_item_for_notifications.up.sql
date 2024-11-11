-- ⚠️⚠️⚠️
-- Backup your database before running this migration as it won't be able to
-- revert the changes made to the `notification` table if needed.
-- ⚠️⚠️⚠️
ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'LinearNotification';
ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'GithubNotification';
ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'GoogleMailThread';

COMMIT;

-- Migrate GithubNotification data to ThirdPartyItem
INSERT INTO
third_party_item (
    id,
    source_id,
    data,
    created_at,
    updated_at,
    user_id,
    integration_connection_id
)
SELECT
    gen_random_uuid() AS id,
    notification.source_id,
    json_build_object(
        'type', 'GithubNotification',
        'content', json_build_object(
            'id', notification.metadata -> 'content' ->> 'id',
            'repository', notification.metadata -> 'content' -> 'repository',
            'subject', notification.metadata -> 'content' -> 'subject',
            'reason', notification.metadata -> 'content' ->> 'reason',
            'unread', (notification.metadata -> 'content' ->> 'unread')::bool,
            'updated_at', notification.metadata -> 'content' ->> 'updated_at',
            'last_read_at',
            notification.metadata -> 'content' ->> 'last_read_at',
            'url', notification.metadata -> 'content' ->> 'url',
            'subscription_url', notification.metadata -> 'content' ->> 'subscription_url',
            'item', nd.details
        )
    ) AS data,
    notification.updated_at AS created_at,
    notification.updated_at,
    notification.user_id,
    ic.id AS integration_connection_id
FROM notification
LEFT JOIN notification_details AS nd ON notification.id = nd.notification_id
INNER JOIN
    integration_connection AS ic
    ON notification.user_id = ic.user_id AND ic.provider_kind = 'Github'
WHERE
    notification.kind = 'Github';

-- Migrate LinearNotification data to ThirdPartyItem
INSERT INTO
third_party_item (
    id,
    source_id,
    data,
    created_at,
    updated_at,
    user_id,
    integration_connection_id
)
SELECT
    gen_random_uuid() AS id,
    notification.source_id,
    json_build_object(
        'type', 'LinearNotification',
        'content', notification.metadata -> 'content'
    ) AS data,
    notification.updated_at AS created_at,
    notification.updated_at,
    notification.user_id,
    ic.id AS integration_connection_id
FROM notification
LEFT JOIN notification_details AS nd ON notification.id = nd.notification_id
INNER JOIN
    integration_connection AS ic
    ON notification.user_id = ic.user_id AND ic.provider_kind = 'Linear'
WHERE
    notification.kind = 'Linear';

-- Migrate SlackStar data to ThirdPartyItem
INSERT INTO
third_party_item (
    id,
    source_id,
    data,
    created_at,
    updated_at,
    user_id,
    integration_connection_id
)
SELECT
    gen_random_uuid() AS id,
    notification.source_id,
    json_build_object(
        'type', 'SlackStar',
        'content', json_build_object(
            'state', json_build_object('type', (
                CASE
                    WHEN
                        notification.metadata -> 'content' -> 'event' ->> 'type'
                        = 'star_added'
                        THEN 'StarAdded'
                    WHEN
                        notification.metadata -> 'content' -> 'event' ->> 'type'
                        = 'star_removed'
                        THEN 'StarRemoved'
                END
            )),
            'created_at', to_char(to_timestamp((notification.metadata -> 'content' ->> 'event_time')::int) AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
            'item', nd.details
        )
    ) AS data,
    notification.updated_at AS created_at,
    notification.updated_at,
    notification.user_id,
    ic.id AS integration_connection_id
FROM notification
INNER JOIN notification_details AS nd ON notification.id = nd.notification_id
INNER JOIN
    integration_connection AS ic
    ON notification.user_id = ic.user_id AND ic.provider_kind = 'Slack'
WHERE
    notification.metadata -> 'content' -> 'event' ->> 'type' = 'star_added'
    OR notification.metadata -> 'content' -> 'event' ->> 'type' = 'star_removed';

-- Migrate SlackStar data to ThirdPartyItem
INSERT INTO
third_party_item (
    id,
    source_id,
    data,
    created_at,
    updated_at,
    user_id,
    integration_connection_id
)
SELECT
    gen_random_uuid() AS id,
    notification.source_id,
    json_build_object(
        'type', 'SlackReaction',
        'content', json_build_object(
            'name', notification.metadata
            -> 'content'
            -> 'event'
            ->> 'reaction',
            'state', json_build_object('type', (
                CASE
                    WHEN
                        notification.metadata -> 'content' -> 'event' ->> 'type'
                        = 'reaction_added'
                        THEN 'ReactionAdded'
                    WHEN
                        notification.metadata -> 'content' -> 'event' ->> 'type'
                        = 'reaction_removed'
                        THEN 'ReactionRemoved'
                END
            )),
            'created_at', to_char(to_timestamp((notification.metadata -> 'content' ->> 'event_time')::int) AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
            'item', nd.details
        )
    ) AS data,
    notification.updated_at AS created_at,
    notification.updated_at,
    notification.user_id,
    ic.id AS integration_connection_id
FROM notification
INNER JOIN notification_details AS nd ON notification.id = nd.notification_id
INNER JOIN
    integration_connection AS ic
    ON notification.user_id = ic.user_id AND ic.provider_kind = 'Slack'
WHERE
    notification.metadata -> 'content' -> 'event' ->> 'type' = 'reaction_added'
    OR notification.metadata -> 'content' -> 'event' ->> 'type' = 'reaction_removed';

-- Migrate GoogleMailThread data to ThirdPartyItem
INSERT INTO
third_party_item (
    id,
    source_id,
    data,
    created_at,
    updated_at,
    user_id,
    integration_connection_id
)
SELECT
    gen_random_uuid() AS id,
    notification.source_id,
    json_build_object(
        'type', 'GoogleMailThread',
        'content', notification.metadata -> 'content'
    ) AS data,
    notification.updated_at AS created_at,
    notification.updated_at,
    notification.user_id,
    ic.id AS integration_connection_id
FROM notification
INNER JOIN
    integration_connection AS ic
    ON notification.user_id = ic.user_id AND ic.provider_kind = 'GoogleMail'
WHERE
    notification.kind = 'GoogleMail';

CREATE TYPE notification_source_kind AS ENUM (
    'Github',
    'Todoist',
    'Linear',
    'GoogleMail',
    'Slack'
);

-- Link Notification to ThirdPartyItem
ALTER TABLE notification
ADD COLUMN created_at timestamp,
ADD COLUMN kind_tmp notification_source_kind,
ADD COLUMN source_item_id uuid,
ADD CONSTRAINT fk_source_item FOREIGN KEY (
    source_item_id
) REFERENCES third_party_item (id);

-- Copy generated column values `kind` into `kind_tmp`
UPDATE
    notification
SET
    kind_tmp = notification.kind::notification_source_kind,
    created_at = notification.updated_at,
    source_item_id = third_party_item.id
FROM third_party_item
WHERE third_party_item.source_id = notification.source_id;

-- Remove deprecated indexes
DROP INDEX notification_source_id_kind_user_id_idx;

-- Remove deprecated Notification columns and constraints
ALTER TABLE notification
DROP COLUMN kind;
ALTER TABLE notification
ALTER COLUMN source_item_id SET NOT NULL,
ALTER COLUMN created_at SET NOT NULL,
DROP COLUMN metadata,
DROP COLUMN source_id;
ALTER TABLE notification
RENAME COLUMN kind_tmp TO kind;

-- Remove notification_details table
DROP INDEX notification_details_notification_id_idx;

DROP TABLE notification_details;

DROP FUNCTION text_to_notification_details_kind;

DROP TYPE notification_details_kind;
