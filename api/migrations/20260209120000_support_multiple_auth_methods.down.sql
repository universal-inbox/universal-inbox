-- Revert: re-add UNIQUE(user_id) and remove UNIQUE(user_id, kind)
-- WARNING: This will fail if any user has multiple auth methods.

ALTER TABLE user_auth
  DROP CONSTRAINT user_auth_user_id_kind_unique;

ALTER TABLE user_auth
  ADD CONSTRAINT user_auth_user_id_key UNIQUE (user_id);
