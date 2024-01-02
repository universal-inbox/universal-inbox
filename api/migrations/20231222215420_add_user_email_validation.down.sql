DROP INDEX user_email_validation_token_idx;

ALTER TABLE "USER"
  DROP COLUMN email_validated_at,
  DROP COLUMN email_validation_sent_at,
  DROP COLUMN email_validation_token;
