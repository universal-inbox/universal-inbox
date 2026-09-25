-- An email change requested through PATCH /api/users/me is held here until the
-- new address is verified, instead of being written to "user".email straight
-- away. Writing it immediately surfaced the unique-email constraint violation
-- to the caller (409), which let any account probe whether an address is
-- registered. The change is applied, and uniqueness checked, only when the
-- owner of the new mailbox follows the verification link.
CREATE TABLE user_email_change (
  user_id UUID PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
  new_email TEXT NOT NULL,
  validation_token UUID NOT NULL,
  requested_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
