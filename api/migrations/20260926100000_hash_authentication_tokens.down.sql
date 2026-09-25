-- The cleartext tokens cannot be restored: every row gets a placeholder value
-- and is marked revoked.
ALTER TABLE authentication_token ADD COLUMN jwt_token TEXT;

UPDATE authentication_token
SET
  jwt_token = 'unrecoverable:' || jwt_token_hash,
  is_revoked = TRUE;

ALTER TABLE authentication_token
  ALTER COLUMN jwt_token SET NOT NULL,
  ADD CONSTRAINT authentication_token_jwt_token_key UNIQUE (jwt_token),
  DROP CONSTRAINT authentication_token_jwt_token_hash_key,
  DROP COLUMN jwt_token_hash,
  DROP COLUMN truncated_jwt_token;
