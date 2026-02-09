-- Allow multiple authentication methods per user by removing the UNIQUE
-- constraint on user_id and adding a UNIQUE constraint on (user_id, kind)
-- to prevent duplicate auth methods of the same kind for the same user.

ALTER TABLE user_auth
  DROP CONSTRAINT user_auth_user_id_key;

ALTER TABLE user_auth
  ADD CONSTRAINT user_auth_user_id_kind_unique UNIQUE (user_id, kind);
