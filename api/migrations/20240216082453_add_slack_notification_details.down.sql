CREATE TYPE notification_details_kind_new AS ENUM ('GithubPullRequest', 'GithubDiscussion');

DELETE FROM notification_details
  WHERE provider_kind IN ('SlackMessage', 'SlackFile', 'SlackFileComment', 'SlackChannel', 'SlackIm', 'SlackGroup');

ALTER TABLE notification_details
  ALTER COLUMN kind TYPE notification_details_kind_new 
  USING (kind::text::notification_details_kind_new);

DROP TYPE notification_details_kind;
ALTER TYPE notification_details_kind_new RENAME TO notification_details_kind;
