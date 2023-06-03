ALTER TABLE integration_connection
  ADD last_sync_started_at TIMESTAMP,
  ADD last_sync_failure_message TEXT;
