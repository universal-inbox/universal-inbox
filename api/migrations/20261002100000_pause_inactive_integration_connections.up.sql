-- Last time the user made an authenticated request, bumped at most once a day
-- by the API. Existing users are backfilled with the migration time so that
-- the inactivity policy starts counting from now instead of pausing everyone
-- on its first run.
ALTER TABLE "user"
    ADD COLUMN last_active_at TIMESTAMP NOT NULL DEFAULT (NOW() AT TIME ZONE 'utc');

-- A connection paused by Universal Inbox (not by the user, not by the billing
-- plan): its grant was revoked at the provider and its credential deleted.
-- It behaves like a disconnected connection until the user reconnects it.
ALTER TYPE integration_connection_status ADD VALUE IF NOT EXISTS 'Paused';

-- `paused_at` and `paused_reason` are set together when a connection is moved
-- to `Paused` and cleared together when it leaves that status.
ALTER TABLE integration_connection
    ADD COLUMN paused_at TIMESTAMP,
    ADD COLUMN paused_reason TEXT;
