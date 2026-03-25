-- Allow multiple authentication methods per user by removing the UNIQUE
-- constraint on user_id and adding a UNIQUE constraint on (user_id, kind)
-- to prevent duplicate auth methods of the same kind for the same user.

-- Drop any UNIQUE constraint on user_id alone (name may vary across environments)
DO $$
DECLARE
  _constraint_name TEXT;
BEGIN
  SELECT con.conname INTO _constraint_name
  FROM pg_constraint con
  JOIN pg_class rel ON rel.oid = con.conrelid
  JOIN pg_attribute att ON att.attrelid = rel.oid AND att.attnum = ANY(con.conkey)
  WHERE rel.relname = 'user_auth'
    AND con.contype = 'u'
    AND array_length(con.conkey, 1) = 1
    AND att.attname = 'user_id';

  IF _constraint_name IS NOT NULL THEN
    EXECUTE format('ALTER TABLE user_auth DROP CONSTRAINT %I', _constraint_name);
  END IF;
END $$;

ALTER TABLE user_auth
  ADD CONSTRAINT user_auth_user_id_kind_unique UNIQUE (user_id, kind);
