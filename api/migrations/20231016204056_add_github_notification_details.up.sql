CREATE TYPE notification_details_kind AS ENUM ('GithubPullRequest');

-- Create a cast function from TEXT to NOTIFICATION_DETAILS_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_notification_details_kind(kind TEXT) RETURNS NOTIFICATION_DETAILS_KIND
  IMMUTABLE
  RETURN kind::NOTIFICATION_DETAILS_KIND;

-- Create notification details table
CREATE TABLE notification_details(
  id UUID NOT NULL,
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL,
  PRIMARY KEY (id),
  notification_id UUID NOT NULL UNIQUE,
  FOREIGN KEY (notification_id) REFERENCES notification(id),
  kind NOTIFICATION_DETAILS_KIND GENERATED ALWAYS AS (text_to_notification_details_kind(details->>'type')) STORED,
  details JSON NOT NULL
);

CREATE INDEX notification_details_notification_id_idx ON notification_details(notification_id);
