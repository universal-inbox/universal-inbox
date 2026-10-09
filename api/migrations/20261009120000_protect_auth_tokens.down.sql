-- Hashed tokens cannot be restored: pending email validations, password resets and email
-- changes must be requested again. Encrypted values cannot be decrypted in SQL: only
-- possible before `data-encryption encrypt-plaintext` ran.
ALTER TABLE oauth_credential ALTER COLUMN raw_token_response SET DEFAULT '{}'::jsonb;
UPDATE oauth_credential SET raw_token_response = '{}'::jsonb WHERE raw_token_response IS NULL;
ALTER TABLE oauth_credential ALTER COLUMN raw_token_response SET NOT NULL;
ALTER TABLE oauth_credential DROP COLUMN raw_token_response_enc;

ALTER TABLE user_auth DROP CONSTRAINT user_auth_type_chk;
ALTER TABLE user_auth
  ADD CONSTRAINT user_auth_type_chk CHECK
    (
      CASE
        WHEN kind = 'Passkey' THEN username IS NOT NULL AND passkey IS NOT NULL
        WHEN kind = 'OIDCGoogleAuthorizationCode' THEN auth_user_id IS NOT NULL AND auth_id_token IS NOT NULL
        WHEN kind = 'OIDCAuthorizationCodePKCE' THEN auth_user_id IS NOT NULL AND auth_id_token IS NOT NULL
        WHEN kind = 'Local' THEN password_hash IS NOT NULL
      END
    );
ALTER TABLE user_auth ADD CONSTRAINT user_auth_auth_id_token_key UNIQUE (auth_id_token);
ALTER TABLE user_auth DROP COLUMN auth_id_token_enc;

DELETE FROM oauth2_authorization_code;
ALTER TABLE oauth2_authorization_code RENAME COLUMN code_hash TO code;

DELETE FROM user_email_change;
ALTER TABLE user_email_change ADD COLUMN validation_token UUID NOT NULL;
ALTER TABLE user_email_change DROP COLUMN validation_token_hash;

ALTER TABLE user_auth ADD COLUMN password_reset_token UUID UNIQUE;
CREATE INDEX user_auth_password_reset_token_idx ON user_auth(password_reset_token);
ALTER TABLE user_auth DROP COLUMN password_reset_token_hash;

ALTER TABLE "user" ADD COLUMN email_validation_token UUID UNIQUE;
CREATE INDEX user_email_validation_token_idx ON "user"(email_validation_token);
ALTER TABLE "user" DROP COLUMN email_validation_token_hash;
