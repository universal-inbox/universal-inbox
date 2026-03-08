CREATE TYPE subscription_status AS ENUM (
  'Trialing',
  'Active',
  'PastDue',
  'Canceled',
  'Expired',
  'Unlimited'
);

CREATE TABLE user_subscription (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL UNIQUE REFERENCES "user"(id) ON DELETE CASCADE,
  stripe_customer_id VARCHAR(255) NULL,
  subscription_status subscription_status NOT NULL DEFAULT 'Trialing',
  subscription_id VARCHAR(255) NULL,
  trial_started_at TIMESTAMPTZ NULL,
  trial_ends_at TIMESTAMPTZ NULL,
  subscription_ends_at TIMESTAMPTZ NULL,
  billing_interval VARCHAR(20) NULL,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_user_subscription_user_id ON user_subscription(user_id);
