-- OAuth2 Client ID Metadata Discovery (CIMD) — cached client metadata documents
-- per draft-ietf-oauth-client-id-metadata-document and MCP 2025-11-25.
-- The metadata URL itself is the client_id (URL == document.client_id);
-- the JSON body is what the AS validates redirect_uris / scopes / forbidden
-- fields against on every /authorize and /token call.
CREATE TABLE oauth2_client_metadata_cache (
    client_id   TEXT PRIMARY KEY,
    document    JSONB NOT NULL,
    body_sha256 BYTEA NOT NULL,
    fetched_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX oauth2_client_metadata_cache_expires_at_idx
    ON oauth2_client_metadata_cache (expires_at);
