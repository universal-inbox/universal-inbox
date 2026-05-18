CREATE TABLE oauth2_user_consent (
    user_id UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL REFERENCES oauth2_client(client_id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, client_id)
);

CREATE INDEX oauth2_user_consent_client_id_idx ON oauth2_user_consent (client_id);
