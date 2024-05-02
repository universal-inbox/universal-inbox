ALTER TABLE integration_connection
  ADD sync_failures INTEGER NOT NULL DEFAULT 0;

CREATE INDEX integration_connection_sync_failures_idx
  ON integration_connection(sync_failures);
