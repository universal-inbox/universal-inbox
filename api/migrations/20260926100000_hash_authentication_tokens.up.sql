-- Long-lived API bearer tokens used to be stored verbatim in
-- `authentication_token.jwt_token`: any read of the table (backup, replica,
-- SQL read primitive) yielded working credentials. Keep only a SHA-256 digest,
-- used to look the row up on every bearer-authenticated request (so
-- `is_revoked` / `expire_at` now gate authentication), plus the last five
-- characters for display.
--
-- Existing tokens keep working: their digest is computed from the stored value
-- before the cleartext column is dropped.
ALTER TABLE authentication_token
  ADD COLUMN jwt_token_hash TEXT,
  ADD COLUMN truncated_jwt_token TEXT;

UPDATE authentication_token
SET
  jwt_token_hash = encode(sha256(convert_to(jwt_token, 'UTF8')), 'hex'),
  truncated_jwt_token = right(jwt_token, 5);

ALTER TABLE authentication_token
  ALTER COLUMN jwt_token_hash SET NOT NULL,
  ALTER COLUMN truncated_jwt_token SET NOT NULL,
  ADD CONSTRAINT authentication_token_jwt_token_hash_key UNIQUE (jwt_token_hash);

ALTER TABLE authentication_token DROP COLUMN jwt_token;
