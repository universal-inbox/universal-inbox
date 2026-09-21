-- Idempotency log for Stripe webhook deliveries: each row records that
-- `event_id` has already been processed so re-deliveries are skipped.
CREATE TABLE stripe_event (
    event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
