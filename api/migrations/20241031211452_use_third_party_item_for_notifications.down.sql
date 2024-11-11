-- ⚠️⚠️⚠️
-- This migration will delete all `notifications` as it cannot restore the `metadata` column.
-- ⚠️⚠️⚠️
CREATE TYPE third_party_item_kind_new AS ENUM ('TodoistItem', 'LinearIssue', 'SlackStar', 'SlackReaction');

CREATE TYPE notification_details_kind AS ENUM (
    'GithubPullRequest',
    'GithubDiscussion',
    'SlackMessage',
    'SlackFile',
    'SlackFileComment',
    'SlackChannel',
    'SlackIm',
    'SlackGroup'
);

-- Create a cast function from TEXT to NOTIFICATION_DETAILS_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_notification_details_kind(kind TEXT) RETURNS NOTIFICATION_DETAILS_KIND
IMMUTABLE
RETURN kind::NOTIFICATION_DETAILS_KIND;

-- Create notification details table
CREATE TABLE notification_details (
    id UUID NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (id),
    notification_id UUID NOT NULL UNIQUE,
    FOREIGN KEY (notification_id) REFERENCES notification (id),
    kind NOTIFICATION_DETAILS_KIND GENERATED ALWAYS AS (text_to_notification_details_kind(details ->> 'type')) STORED,
    details JSON NOT NULL
);

CREATE INDEX notification_details_notification_id_idx ON notification_details (notification_id);

-- Delete notifications and associated third_party_items
DELETE FROM third_party_item
USING notification
WHERE notification.source_item_id = third_party_item.id;
DELETE FROM notification;

-- Restore the `notification` table
ALTER TABLE notification
DROP COLUMN created_at,
DROP COLUMN source_item_id,
DROP COLUMN kind;
ALTER TABLE notification
ADD COLUMN metadata JSON NOT NULL,
ADD COLUMN source_id TEXT NOT NULL;

ALTER TABLE notification
ADD kind TEXT GENERATED ALWAYS AS (metadata ->> 'type') STORED;

ALTER TABLE notification
ADD CONSTRAINT notification_source_id_kind_user_id_key UNIQUE (source_id, kind, user_id);

CREATE INDEX notification_source_id_kind_user_id_idx
ON notification (source_id, kind, user_id);

DROP TYPE NOTIFICATION_KIND;

-- Revert the `third_party_item.kind` column
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
