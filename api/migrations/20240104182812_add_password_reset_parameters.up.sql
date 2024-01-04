ALTER TABLE user_auth
  ADD password_reset_at TIMESTAMP,
  ADD password_reset_sent_at TIMESTAMP,
  ADD password_reset_token UUID UNIQUE;

CREATE INDEX user_auth_password_reset_token_idx
  ON user_auth(password_reset_token);
