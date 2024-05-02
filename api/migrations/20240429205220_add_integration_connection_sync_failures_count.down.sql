ALTER TABLE integration_connection
  DROP sync_failures;

DROP INDEX integration_connection_sync_failures_idx;
