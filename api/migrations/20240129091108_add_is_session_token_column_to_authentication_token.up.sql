ALTER TABLE authentication_token
  ADD is_session_token BOOLEAN NOT NULL DEFAULT TRUE;
