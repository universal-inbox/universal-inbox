-- The `connection_id` column stored the legacy Nango connection identifier. Since every
-- integration connection now authenticates through the internal OAuth flow keyed on `id`,
-- this column has no remaining readers or writers.
ALTER TABLE integration_connection DROP COLUMN connection_id;
