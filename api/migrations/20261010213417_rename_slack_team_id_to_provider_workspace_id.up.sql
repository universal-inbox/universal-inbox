-- The provider workspace (Slack team, Notion workspace) a connection belongs
-- to, kept in clear next to the encrypted context to route webhook events.
ALTER TABLE integration_connection RENAME COLUMN slack_team_id TO provider_workspace_id;

DROP INDEX integration_connection_slack_team_id_idx;
CREATE INDEX integration_connection_provider_workspace_id_idx
  ON integration_connection(provider_kind, provider_workspace_id);
