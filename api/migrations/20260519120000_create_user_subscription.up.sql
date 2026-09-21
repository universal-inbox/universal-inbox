CREATE TABLE user_subscription (
    user_id UUID PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
    -- Nullable: a user can have a subscription row before a Stripe customer
    -- exists (e.g. a Free placeholder carrying the over-limit grace deadline).
    -- "No customer yet" is NULL, not a sentinel string. UNIQUE still holds —
    -- Postgres allows multiple NULLs under a UNIQUE constraint.
    stripe_customer_id TEXT UNIQUE,
    stripe_subscription_id TEXT UNIQUE,
    stripe_price_id TEXT,
    plan TEXT NOT NULL DEFAULT 'free',
    status TEXT NOT NULL DEFAULT 'incomplete',
    current_period_start TIMESTAMPTZ,
    current_period_end TIMESTAMPTZ,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT FALSE,
    canceled_at TIMESTAMPTZ,
    over_limit_grace_deadline TIMESTAMPTZ,
    -- Stripe `event.created` of the most recent subscription-state webhook
    -- applied to this row. Stripe does not guarantee webhook delivery order, so
    -- the handler compares an incoming event's `created` against this column and
    -- ignores older events (e.g. a retried `subscription.updated` arriving after
    -- `subscription.deleted`). Distinct from `updated_at`, which every write
    -- bumps to now() — this is a clean per-event ordering marker.
    last_stripe_event_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Subscription lookup by Stripe identifiers (used when reconciling webhook events).
CREATE INDEX user_subscription_stripe_customer_id_idx
    ON user_subscription (stripe_customer_id);
CREATE INDEX user_subscription_stripe_subscription_id_idx
    ON user_subscription (stripe_subscription_id);

-- Reconciliation scans filter on status + grace deadline.
CREATE INDEX user_subscription_status_idx ON user_subscription (status);
CREATE INDEX user_subscription_over_limit_grace_deadline_idx
    ON user_subscription (over_limit_grace_deadline)
    WHERE over_limit_grace_deadline IS NOT NULL;
