-- Create user table
CREATE TABLE IF NOT EXISTS "user" (
  id UUID NOT NULL,
  PRIMARY KEY (id),
  auth_user_id TEXT NOT NULL UNIQUE,
  first_name TEXT NOT NULL,
  last_name TEXT NOT NULL,
  email TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX user_auth_user_id_idx
  ON "user"(auth_user_id);
