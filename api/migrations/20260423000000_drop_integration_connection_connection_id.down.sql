-- Recreate the column as nullable. The original Nango connection ids are not
-- recoverable once the column has been dropped, so rollback is best-effort.
ALTER TABLE integration_connection ADD COLUMN connection_id UUID;
