DROP INDEX IF EXISTS user_auth_user_id_idx;

CREATE TYPE user_auth_kind AS ENUM ('OpenIdConnect', 'Local');

CREATE TABLE user_auth (
    id UUID NOT NULL,
    PRIMARY KEY (id),
    user_id UUID NOT NULL UNIQUE,
    FOREIGN KEY (user_id) REFERENCES "user"(id),
    kind USER_AUTH_KIND NOT NULL,
    auth_user_id TEXT UNIQUE,
    auth_id_token TEXT UNIQUE,
    password_hash TEXT,
    CONSTRAINT user_auth_type_chk CHECK
    (
      CASE
        WHEN kind = 'OpenIdConnect' THEN auth_user_id IS NOT NULL AND auth_id_token IS NOT NULL
        WHEN kind = 'Local' THEN password_hash IS NOT NULL
      END
    )
);

CREATE INDEX user_auth_user_id_idx ON user_auth(user_id);
CREATE INDEX user_auth_auth_user_id_idx ON user_auth(auth_user_id);

INSERT INTO
  user_auth
SELECT
  gen_random_uuid(),
  id AS user_id,
  'OpenIdConnect',
  auth_user_id,
  auth_id_token,
  NULL -- password_hash
FROM
  "user";

ALTER TABLE "user"
  DROP COLUMN auth_user_id,
  DROP COLUMN auth_id_token,
  ADD CONSTRAINT unique_email UNIQUE (email);
