use rstest::*;

use universal_inbox_api::configuration::Settings;

use crate::common::settings;

#[rstest]
fn test_vacuum_jobs_cron_settings(settings: Settings) {
    let cron_settings = settings.application.cron.vacuum_jobs;
    // Disabled in config/test.toml
    assert!(!cron_settings.is_enabled);
    // Other fields fall back to the values from config/default.toml
    assert_eq!(cron_settings.schedule, "0 */10 * * * *");
    assert_eq!(cron_settings.retention_hours, 6);
    assert_eq!(cron_settings.batch_size, 1000);
    assert_eq!(cron_settings.max_batches_per_tick, 200);
    assert_eq!(cron_settings.lock_ttl_seconds, 300);
}

#[rstest]
fn test_refresh_oauth_tokens_cron_settings(settings: Settings) {
    let cron_settings = settings.application.cron.refresh_oauth_tokens;
    // Disabled in config/test.toml
    assert!(!cron_settings.is_enabled);
    // Other fields fall back to their defaults
    assert_eq!(cron_settings.schedule, "0 */5 * * * *");
    assert_eq!(cron_settings.minutes_before_expiry, 10);
    assert_eq!(cron_settings.lock_ttl_seconds, 60);
}

#[rstest]
fn test_pause_integration_connections_cron_settings(settings: Settings) {
    let cron_settings = settings.application.cron.pause_integration_connections;
    // Disabled by default: it revokes grants at Slack
    assert!(!cron_settings.is_enabled);
    assert_eq!(cron_settings.schedule, "0 0 3 * * *");
    assert_eq!(cron_settings.inactivity_threshold_days, 90);
    assert_eq!(cron_settings.inactivity_warning_days, 7);
    assert_eq!(cron_settings.failing_threshold_days, 30);
    assert_eq!(cron_settings.lock_ttl_seconds, 300);
}

#[rstest]
fn test_purge_audit_events_cron_settings(settings: Settings) {
    let cron_settings = settings.application.cron.purge_audit_events;
    // Disabled in tests only: the purge is exercised by calling it directly
    assert!(!cron_settings.is_enabled);
    assert_eq!(cron_settings.schedule, "0 17 * * * *");
    assert_eq!(cron_settings.lock_ttl_seconds, 300);
    assert_eq!(settings.application.audit_log.retention_days, 7);
    assert!(settings.application.audit_log.strict_actor);
}
