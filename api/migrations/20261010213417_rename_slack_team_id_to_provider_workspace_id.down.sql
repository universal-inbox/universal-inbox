DROP INDEX integration_connection_provider_workspace_id_idx;

-- Only Slack used the column before it became generic.
UPDATE integration_connection SET provider_workspace_id = NULL WHERE provider_kind::TEXT <> 'Slack';

ALTER TABLE integration_connection RENAME COLUMN provider_workspace_id TO slack_team_id;
CREATE INDEX integration_connection_slack_team_id_idx ON integration_connection(slack_team_id);
