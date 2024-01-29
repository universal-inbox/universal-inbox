CREATE TABLE authentication_token (
  id UUID NOT NULL,
  PRIMARY KEY (id),
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
  user_id UUID NOT NULL,
  FOREIGN KEY (user_id) REFERENCES "user"(id),
  jwt_token TEXT NOT NULL UNIQUE,
  expire_at TIMESTAMP,
  is_revoked BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX authentication_token_expire_at_idx ON authentication_token(expire_at);
CREATE INDEX authentication_token_is_revoked_idx ON authentication_token(is_revoked);
