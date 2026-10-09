-- Drop the plaintext columns emptied by `data-encryption encrypt-plaintext` (see
-- doc/src/config/data_encryption.md). Refuse to migrate, before anything is changed, while
-- any of them still holds a value: it would be lost.
DO $$
DECLARE
  remaining TEXT;
BEGIN
  SELECT string_agg(format('%s (%s)', name, count), ', ')
    INTO remaining
    FROM (
      SELECT 'third_party_item.data' AS name, count(*) FROM third_party_item WHERE data IS NOT NULL
      UNION ALL
      SELECT 'task.body', count(*) FROM task WHERE body IS NOT NULL
      UNION ALL
      SELECT 'user.email', count(*) FROM "user" WHERE email IS NOT NULL
      UNION ALL
      SELECT 'user_email_change.new_email', count(*) FROM user_email_change WHERE new_email IS NOT NULL
      UNION ALL
      SELECT 'user_auth.auth_id_token', count(*) FROM user_auth WHERE auth_id_token IS NOT NULL
      UNION ALL
      SELECT 'oauth_credential.raw_token_response', count(*) FROM oauth_credential WHERE raw_token_response IS NOT NULL
      UNION ALL
      SELECT 'integration_connection.context', count(*) FROM integration_connection WHERE context IS NOT NULL
      UNION ALL
      SELECT 'oauth_grant_revocation.provider_context', count(*) FROM oauth_grant_revocation WHERE provider_context IS NOT NULL
    ) AS plaintext_values
   WHERE count > 0;
  IF remaining IS NOT NULL THEN
    RAISE EXCEPTION 'Plaintext values are not encrypted yet: %. Run `data-encryption encrypt-plaintext` with the previous release before upgrading.', remaining;
  END IF;
END $$;

ALTER TABLE third_party_item DROP CONSTRAINT third_party_item_data_check;
ALTER TABLE third_party_item DROP COLUMN data;
ALTER TABLE third_party_item ALTER COLUMN data_enc SET NOT NULL;

ALTER TABLE task DROP CONSTRAINT task_body_check;
ALTER TABLE task DROP COLUMN body;
ALTER TABLE task ALTER COLUMN body_enc SET NOT NULL;

ALTER TABLE "user" DROP CONSTRAINT user_email_check;
ALTER TABLE "user" DROP COLUMN email;

ALTER TABLE user_email_change DROP CONSTRAINT user_email_change_new_email_check;
ALTER TABLE user_email_change DROP COLUMN new_email;
ALTER TABLE user_email_change ALTER COLUMN new_email_enc SET NOT NULL;

ALTER TABLE user_auth DROP CONSTRAINT user_auth_type_chk;
ALTER TABLE user_auth DROP COLUMN auth_id_token;
ALTER TABLE user_auth
  ADD CONSTRAINT user_auth_type_chk CHECK
    (
      CASE
        WHEN kind = 'Passkey' THEN username IS NOT NULL AND passkey IS NOT NULL
        WHEN kind = 'OIDCGoogleAuthorizationCode' THEN auth_user_id IS NOT NULL
          AND auth_id_token_enc IS NOT NULL
        WHEN kind = 'OIDCAuthorizationCodePKCE' THEN auth_user_id IS NOT NULL
          AND auth_id_token_enc IS NOT NULL
        WHEN kind = 'Local' THEN password_hash IS NOT NULL
      END
    );

ALTER TABLE oauth_credential DROP COLUMN raw_token_response;

ALTER TABLE integration_connection DROP COLUMN context;

ALTER TABLE oauth_grant_revocation DROP COLUMN provider_context;
