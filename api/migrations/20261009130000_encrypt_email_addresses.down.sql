-- Only possible before `data-encryption encrypt-plaintext` moved the values to the
-- encrypted columns: they cannot be decrypted in SQL. Google `provider_user_id` blind
-- indexes cannot be reverted to email addresses.
ALTER TABLE oauth_grant_revocation DROP COLUMN provider_context_enc;

DROP INDEX integration_connection_slack_team_id_idx;
ALTER TABLE integration_connection DROP COLUMN slack_team_id;
ALTER TABLE integration_connection DROP COLUMN context_enc;

ALTER TABLE user_email_change DROP CONSTRAINT user_email_change_new_email_check;
ALTER TABLE user_email_change ALTER COLUMN new_email SET NOT NULL;
ALTER TABLE user_email_change DROP COLUMN new_email_enc;

ALTER TABLE "user" DROP CONSTRAINT user_email_check;
ALTER TABLE "user" ADD CONSTRAINT unique_email UNIQUE (email);
ALTER TABLE "user" DROP CONSTRAINT user_email_hash_key;
ALTER TABLE "user" DROP COLUMN email_hash;
ALTER TABLE "user" DROP COLUMN email_enc;
