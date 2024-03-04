ALTER TYPE notification_details_kind ADD VALUE IF NOT EXISTS 'SlackMessage';
ALTER TYPE notification_details_kind ADD VALUE IF NOT EXISTS 'SlackFile';
ALTER TYPE notification_details_kind ADD VALUE IF NOT EXISTS 'SlackFileComment';
ALTER TYPE notification_details_kind ADD VALUE IF NOT EXISTS 'SlackChannel';
ALTER TYPE notification_details_kind ADD VALUE IF NOT EXISTS 'SlackIm';
ALTER TYPE notification_details_kind ADD VALUE IF NOT EXISTS 'SlackGroup';
