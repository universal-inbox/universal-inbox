ALTER TABLE "user"
  ADD auth_id_token TEXT UNIQUE,
  ADD auth_user_id TEXT UNIQUE,
  DROP CONSTRAINT unique_email;

UPDATE "user"
  SET auth_id_token = user_auth.auth_id_token,
      auth_user_id = user_auth.auth_user_id
  FROM user_auth
  WHERE "user".id = user_auth.user_id;

ALTER TABLE "user"
  ALTER COLUMN auth_id_token SET NOT NULL,
  ALTER COLUMN auth_user_id SET NOT NULL;

DROP TABLE user_auth;

DROP INDEX IF EXISTS user_auth_user_id_idx;
DROP INDEX IF EXISTS user_auth_auth_user_id_idx;

DROP TYPE user_auth_kind;

CREATE INDEX user_auth_user_id_idx
  ON "user"(auth_user_id);
