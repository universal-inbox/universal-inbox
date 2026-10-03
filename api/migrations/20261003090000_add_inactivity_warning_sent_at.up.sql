-- When the user was last warned by email that this connection is about to be
-- paused for inactivity. The warning only counts for the current inactivity
-- period: once the user is active again (`user.last_active_at` moves past
-- it), the connection is warned again before a later pause.
ALTER TABLE integration_connection
    ADD COLUMN inactivity_warning_sent_at TIMESTAMP;
