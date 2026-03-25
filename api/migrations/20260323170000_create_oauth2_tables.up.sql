-- OAuth2 client registration (RFC 7591)
CREATE TABLE oauth2_client (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_id TEXT UNIQUE NOT NULL,
    client_name TEXT,
    redirect_uris TEXT[] NOT NULL,
    grant_types TEXT[] NOT NULL DEFAULT '{authorization_code}',
    response_types TEXT[] NOT NULL DEFAULT '{code}',
    token_endpoint_auth_method TEXT NOT NULL DEFAULT 'none',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX oauth2_client_client_id_idx ON oauth2_client (client_id);

-- OAuth2 authorization codes (short-lived, one-time use)
CREATE TABLE oauth2_authorization_code (
    code TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth2_client(client_id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    redirect_uri TEXT NOT NULL,
    scope TEXT,
    code_challenge TEXT NOT NULL,
    code_challenge_method TEXT NOT NULL DEFAULT 'S256',
    resource TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX oauth2_authorization_code_client_id_idx ON oauth2_authorization_code (client_id);

-- OAuth2 refresh tokens (long-lived, rotated on use)
CREATE TABLE oauth2_refresh_token (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash TEXT UNIQUE NOT NULL,
    client_id TEXT NOT NULL REFERENCES oauth2_client(client_id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    scope TEXT,
    resource TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX oauth2_refresh_token_token_hash_idx ON oauth2_refresh_token (token_hash);
CREATE INDEX oauth2_refresh_token_user_id_idx ON oauth2_refresh_token (user_id);
