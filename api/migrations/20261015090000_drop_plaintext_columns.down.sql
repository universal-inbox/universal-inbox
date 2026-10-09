-- The plaintext columns come back empty: the values stay in the encrypted columns, which
-- the previous release reads.
ALTER TABLE oauth_grant_revocation ADD COLUMN provider_context JSONB;

ALTER TABLE integration_connection ADD COLUMN context JSON;

ALTER TABLE oauth_credential ADD COLUMN raw_token_response JSONB;

ALTER TABLE user_auth DROP CONSTRAINT user_auth_type_chk;
ALTER TABLE user_auth ADD COLUMN auth_id_token TEXT;
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

ALTER TABLE user_email_change ALTER COLUMN new_email_enc DROP NOT NULL;
ALTER TABLE user_email_change ADD COLUMN new_email TEXT;
ALTER TABLE user_email_change
  ADD CONSTRAINT user_email_change_new_email_check CHECK (new_email IS NOT NULL OR new_email_enc IS NOT NULL);

ALTER TABLE "user" ADD COLUMN email TEXT;
ALTER TABLE "user"
  ADD CONSTRAINT user_email_check CHECK (email IS NULL OR email_enc IS NULL);

ALTER TABLE task ALTER COLUMN body_enc DROP NOT NULL;
ALTER TABLE task ADD COLUMN body TEXT;
ALTER TABLE task ADD CONSTRAINT task_body_check CHECK (body IS NOT NULL OR body_enc IS NOT NULL);

ALTER TABLE third_party_item ALTER COLUMN data_enc DROP NOT NULL;
ALTER TABLE third_party_item ADD COLUMN data JSON;
ALTER TABLE third_party_item
  ADD CONSTRAINT third_party_item_data_check CHECK (data IS NOT NULL OR data_enc IS NOT NULL);
