-- Email lookups become case-insensitive (blind index of the lowercased address): refuse to
-- migrate when two accounts would collide, before anything is changed.
DO $$
DECLARE
  duplicates TEXT;
BEGIN
  SELECT string_agg(user_ids, '; ')
    INTO duplicates
    FROM (
      SELECT string_agg(id::TEXT, ', ') AS user_ids
        FROM "user"
       WHERE email IS NOT NULL
       GROUP BY lower(trim(email))
      HAVING count(*) > 1
    ) AS collisions;
  IF duplicates IS NOT NULL THEN
    RAISE EXCEPTION 'Users sharing an email address that differs only by case must be merged or changed before encrypting email addresses: %', duplicates;
  END IF;
END $$;

-- Encrypted email (see api/src/utils/crypto.rs) and its blind index for lookups and
-- uniqueness. Plaintext values are moved by `data-encryption encrypt-plaintext` and the
-- clear columns will be dropped by a later migration.
ALTER TABLE "user" ADD COLUMN email_enc BYTEA;
ALTER TABLE "user" ADD COLUMN email_hash TEXT;
ALTER TABLE "user" ADD CONSTRAINT user_email_hash_key UNIQUE (email_hash);
ALTER TABLE "user" DROP CONSTRAINT unique_email;
ALTER TABLE "user"
  ADD CONSTRAINT user_email_check CHECK (email IS NULL OR email_enc IS NULL);

ALTER TABLE user_email_change ADD COLUMN new_email_enc BYTEA;
ALTER TABLE user_email_change ALTER COLUMN new_email DROP NOT NULL;
ALTER TABLE user_email_change
  ADD CONSTRAINT user_email_change_new_email_check CHECK (new_email IS NOT NULL OR new_email_enc IS NOT NULL);

-- Integration contexts hold the Google account email: encrypt them whole. The Slack
-- workspace id, used for lookups, gets its own clear column.
ALTER TABLE integration_connection ADD COLUMN context_enc BYTEA;
ALTER TABLE integration_connection ADD COLUMN slack_team_id TEXT;
UPDATE integration_connection
   SET slack_team_id = context -> 'content' ->> 'team_id'
 WHERE provider_kind = 'Slack' AND context IS NOT NULL;
CREATE INDEX integration_connection_slack_team_id_idx ON integration_connection(slack_team_id);

ALTER TABLE oauth_grant_revocation ADD COLUMN provider_context_enc BYTEA;
