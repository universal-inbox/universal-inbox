ALTER TABLE "user"
  ADD email_validated_at TIMESTAMP,
  ADD email_validation_sent_at TIMESTAMP,
  ADD email_validation_token UUID UNIQUE;

CREATE INDEX user_email_validation_token_idx
  ON "user"(email_validation_token);
