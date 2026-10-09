-- One-time tokens sent by email (and OAuth authorization codes) are only stored as their
-- SHA-256 (hex): a database dump does not let anyone use them. Pending tokens are hashed
-- in place so the links already sent keep working.
ALTER TABLE "user" ADD COLUMN email_validation_token_hash TEXT UNIQUE;
UPDATE "user"
  SET email_validation_token_hash = encode(sha256(convert_to(email_validation_token::TEXT, 'UTF8')), 'hex')
  WHERE email_validation_token IS NOT NULL;
ALTER TABLE "user" DROP COLUMN email_validation_token;

ALTER TABLE user_auth ADD COLUMN password_reset_token_hash TEXT UNIQUE;
UPDATE user_auth
  SET password_reset_token_hash = encode(sha256(convert_to(password_reset_token::TEXT, 'UTF8')), 'hex')
  WHERE password_reset_token IS NOT NULL;
ALTER TABLE user_auth DROP COLUMN password_reset_token;

ALTER TABLE user_email_change ADD COLUMN validation_token_hash TEXT;
UPDATE user_email_change
  SET validation_token_hash = encode(sha256(convert_to(validation_token::TEXT, 'UTF8')), 'hex');
ALTER TABLE user_email_change ALTER COLUMN validation_token_hash SET NOT NULL;
ALTER TABLE user_email_change DROP COLUMN validation_token;

-- Authorization codes live for minutes: pending ones are dropped, the clients restart the flow
DELETE FROM oauth2_authorization_code;
ALTER TABLE oauth2_authorization_code RENAME COLUMN code TO code_hash;

-- Encrypted OpenID Connect ID tokens (see api/src/utils/crypto.rs). Plaintext ones are moved
-- by `data-encryption encrypt-plaintext` and the column will be dropped by a later migration.
ALTER TABLE user_auth ADD COLUMN auth_id_token_enc BYTEA;
ALTER TABLE user_auth DROP CONSTRAINT user_auth_auth_id_token_key;
ALTER TABLE user_auth DROP CONSTRAINT user_auth_type_chk;
ALTER TABLE user_auth
  ADD CONSTRAINT user_auth_type_chk CHECK
    (
      CASE
        WHEN kind = 'Passkey' THEN username IS NOT NULL AND passkey IS NOT NULL
        WHEN kind = 'OIDCGoogleAuthorizationCode' THEN auth_user_id IS NOT NULL
          AND (auth_id_token IS NOT NULL OR auth_id_token_enc IS NOT NULL)
        WHEN kind = 'OIDCAuthorizationCodePKCE' THEN auth_user_id IS NOT NULL
          AND (auth_id_token IS NOT NULL OR auth_id_token_enc IS NOT NULL)
        WHEN kind = 'Local' THEN password_hash IS NOT NULL
      END
    );

-- Encrypted raw OAuth token responses
ALTER TABLE oauth_credential ADD COLUMN raw_token_response_enc BYTEA;
ALTER TABLE oauth_credential ALTER COLUMN raw_token_response DROP NOT NULL;
ALTER TABLE oauth_credential ALTER COLUMN raw_token_response DROP DEFAULT;
