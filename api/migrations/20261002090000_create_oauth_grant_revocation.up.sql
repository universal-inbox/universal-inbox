-- Pending: still to revoke. Revoked: the provider accepted the revocation, or
-- the token was already dead there. Cancelled: the same provider account was
-- connected again, so revoking would kill the new grant. Abandoned: retries
-- exhausted, or the tokens cannot be decrypted.
CREATE TYPE oauth_grant_revocation_status AS ENUM ('Pending', 'Revoked', 'Cancelled', 'Abandoned');

-- OAuth grants still to revoke at their provider. A row is written when an
-- inline revocation (disconnect, account deletion) fails, and retried by the
-- `retry-oauth-grant-revocations` cron until it leaves the Pending status.
--
-- The tokens are a snapshot of the revoked credential, re-encrypted with the
-- row id as AAD: the integration connection (the AAD of `oauth_credential`)
-- may be deleted with its user, and a reconnect stores new tokens that this
-- queue must never revoke.
CREATE TABLE oauth_grant_revocation (
    id UUID NOT NULL PRIMARY KEY,
    status OAUTH_GRANT_REVOCATION_STATUS NOT NULL DEFAULT 'Pending',
    provider_kind INTEGRATION_PROVIDER_KIND NOT NULL,
    integration_connection_id UUID
        REFERENCES integration_connection(id) ON DELETE SET NULL,
    provider_user_id TEXT,
    provider_context JSONB,
    -- Wiped once the grant is Revoked or Cancelled.
    encrypted_access_token BYTEA,
    encrypted_refresh_token BYTEA,
    access_token_expires_at TIMESTAMPTZ,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    next_attempt_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX oauth_grant_revocation_pending_idx
    ON oauth_grant_revocation (next_attempt_at)
    WHERE status = 'Pending';

CREATE INDEX oauth_grant_revocation_integration_connection_id_idx
    ON oauth_grant_revocation (integration_connection_id);
