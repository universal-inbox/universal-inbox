DROP INDEX user_auth_password_reset_token_idx;

ALTER TABLE user_auth
  DROP COLUMN password_reset_at,
  DROP COLUMN password_reset_sent_at,
  DROP COLUMN password_reset_token;
