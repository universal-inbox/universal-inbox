ALTER TABLE integration_connection
    DROP COLUMN IF EXISTS paused_reason,
    DROP COLUMN IF EXISTS paused_at;

-- PostgreSQL cannot drop an enum value: rebuild the type without 'Paused'.
-- Paused connections hold no credential, so they go back to 'Created'
-- (disconnected).
UPDATE integration_connection SET status = 'Created' WHERE status = 'Paused';

ALTER TYPE integration_connection_status RENAME TO integration_connection_status_old;
CREATE TYPE integration_connection_status AS ENUM ('Created', 'Validated', 'Failing');
ALTER TABLE integration_connection
    ALTER COLUMN status TYPE integration_connection_status
    USING status::TEXT::integration_connection_status;
DROP TYPE integration_connection_status_old;

ALTER TABLE "user"
    DROP COLUMN IF EXISTS last_active_at;
