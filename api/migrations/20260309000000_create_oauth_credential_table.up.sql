CREATE TABLE oauth_credential (
    integration_connection_id UUID NOT NULL PRIMARY KEY
        REFERENCES integration_connection(id) ON DELETE CASCADE,
    encrypted_access_token BYTEA NOT NULL,
    encrypted_refresh_token BYTEA,
    access_token_expires_at TIMESTAMPTZ,
    raw_token_response JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
