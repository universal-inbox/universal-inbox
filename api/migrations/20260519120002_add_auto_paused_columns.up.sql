-- Columns the billing subsystem sets together when it auto-pauses an
-- integration connection on a Paid → Free downgrade that puts the user over the
-- plan's `max_integration_connections` cap.
--
-- `auto_paused_by_plan_at`: marker distinguishing a plan-driven pause from a
-- user-initiated one, so post-mortem audits and the reconciliation CLI can tell
-- them apart. TIMESTAMP (without time zone) to match the rest of this table's
-- temporal columns — the Rust mapping is `Option<NaiveDateTime>`.
--
-- `auto_paused_config_snapshot`: the connection's `IntegrationConnectionConfig`
-- captured at the instant of the pause. Pausing flips the provider's sync
-- toggles off; this preserves the user's exact pre-pause config so an upgrade
-- restores it byte-for-byte rather than blanket-enabling every toggle.
--
-- Invariant, upheld in the repository layer: `auto_paused_config_snapshot` is
-- non-NULL exactly when `auto_paused_by_plan_at` is non-NULL (both set together
-- on pause, both cleared together on restore).
ALTER TABLE integration_connection
    ADD COLUMN auto_paused_by_plan_at TIMESTAMP,
    ADD COLUMN auto_paused_config_snapshot JSONB;
