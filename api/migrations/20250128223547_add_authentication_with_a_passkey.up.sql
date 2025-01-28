ALTER TABLE user_auth
  DROP CONSTRAINT user_auth_type_chk;

ALTER TABLE user_auth
  ALTER COLUMN kind TYPE TEXT;

DROP TYPE user_auth_kind;

CREATE TYPE user_auth_kind AS ENUM ('Passkey', 'OIDCAuthorizationCodePKCE', 'OIDCGoogleAuthorizationCode', 'Local');

ALTER TABLE user_auth
  ALTER COLUMN kind TYPE user_auth_kind
  USING (kind::user_auth_kind);

ALTER TABLE user_auth
  ADD COLUMN username TEXT UNIQUE;
ALTER TABLE user_auth
  ADD COLUMN passkey JSON;

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

ALTER TABLE "user"
  ALTER COLUMN email DROP NOT NULL;
