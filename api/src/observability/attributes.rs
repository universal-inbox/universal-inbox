//! Span and event attribute names, the single registry of every key the API
//! records in its traces.
//!
//! Convention (see `.claude/docs/learnings/observability.md`):
//! - OpenTelemetry semantic conventions where they exist (`user.id`,
//!   `error.type`, `http.*`, `url.*`, ...).
//! - Domain attributes are dotted lower-case `<entity>.<attr>`, without any
//!   `app.` prefix (`integration_connection.id`, `third_party_item.source_id`).
//! - Vendor-specific attributes live under the vendor name (`slack.*`,
//!   `stripe.*`, `linear.*`).
//!
//! Always reference these constants, never a string literal:
//! `fields({ attr::USER_ID } = %user_id)` in `#[tracing::instrument]` and
//! tracing event macros, `span.set_attribute(attr::SLACK_TEAM_ID, ...)` and
//! `span.record(attr::SYNC_ITEMS_COUNT, ...)` elsewhere.

// OpenTelemetry semantic conventions
pub const USER_ID: &str = "user.id";
pub const ERROR_TYPE: &str = "error.type";
pub const ERROR_MESSAGE: &str = "error.message";
pub const HTTP_REQUEST_METHOD: &str = "http.request.method";
pub const HTTP_REQUEST_HEADER_ORIGIN: &str = "http.request.header.origin";
pub const URL_PATH: &str = "url.path";
/// Declared by the `tracing-actix-web` root span
pub const HTTP_STATUS_CODE: &str = "http.status_code";
/// Special field interpreted by `tracing-opentelemetry` as the span status
pub const OTEL_STATUS_CODE: &str = "otel.status_code";

// HTTP request forwarding and rate limiting
pub const HTTP_X_FORWARDED_FOR_MASKED: &str = "http.x_forwarded_for.masked";
pub const HTTP_X_FORWARDED_FOR_ENTRIES: &str = "http.x_forwarded_for.entries";
pub const RATE_LIMIT_TRUSTED_PROXY_HOPS: &str = "rate_limit.trusted_proxy_hops";
pub const LOGIN_THROTTLE_SCOPE: &str = "login_throttle.scope";

pub const PANIC_LOCATION: &str = "panic.location";

// User and authentication
pub const USER_AUTH_KIND: &str = "user.auth.kind";
pub const USER_USERNAME: &str = "user.username";
pub const USER_EMAIL_VALIDATED_AT: &str = "user.email_validated_at";
pub const USER_EMAIL_VALIDATION_SENT_AT: &str = "user.email_validation_sent_at";
pub const USER_PASSWORD_RESET_SENT_AT: &str = "user.password_reset_sent_at";
pub const USER_FOR_UPDATE: &str = "user.for_update";
/// Subject of the user at the authentication provider (OIDC `sub`)
pub const AUTH_PROVIDER_USER_ID: &str = "auth.provider_user_id";
pub const AUTH_JWT_EXPIRES_AT: &str = "auth.jwt.exp";
pub const AUTH_JWT_ISSUED_AT: &str = "auth.jwt.iat";
pub const AUTH_JWT_AUDIENCE: &str = "auth.jwt.aud";
pub const AUTH_TOKEN_ID: &str = "auth_token.id";
pub const AUTH_TOKEN_IS_SESSION: &str = "auth_token.is_session";
pub const AUTH_TOKEN_EXCLUDE_SESSION: &str = "auth_token.exclude_session";

// OAuth: Universal Inbox as an authorization server (MCP clients) and as a
// client of the integration providers
pub const OAUTH_CLIENT_ID: &str = "oauth.client.id";
pub const OAUTH_CLIENT_ID_URL: &str = "oauth.client.id_url";
pub const OAUTH_SCOPE_GRANTED: &str = "oauth.scope.granted";
pub const OAUTH_SCOPE_REQUIRED: &str = "oauth.scope.required";
pub const OAUTH_MINUTES_BEFORE_EXPIRY: &str = "oauth.minutes_before_expiry";
pub const OAUTH_REVOKED_AT: &str = "oauth.revoked_at";
pub const OAUTH_GRANT_REVOCATION_ID: &str = "oauth_grant_revocation.id";

// Integration connections
pub const INTEGRATION_CONNECTION_ID: &str = "integration_connection.id";
pub const INTEGRATION_CONNECTION_STATUS: &str = "integration_connection.status";
pub const INTEGRATION_CONNECTION_PAUSED_REASON: &str = "integration_connection.paused_reason";
pub const INTEGRATION_CONNECTION_INACTIVE_BEFORE: &str = "integration_connection.inactive_before";
pub const INTEGRATION_CONNECTION_FAILING_BEFORE: &str = "integration_connection.failing_before";
pub const INTEGRATION_CONNECTION_SYNCED_BEFORE: &str = "integration_connection.synced_before";
pub const INTEGRATION_CONNECTION_LOCK_ROWS: &str = "integration_connection.lock_rows";
pub const INTEGRATION_CONNECTION_WARNED_BEFORE: &str = "integration_connection.warned_before";
pub const INTEGRATION_CONNECTION_PAUSE_ON: &str = "integration_connection.pause_on";
pub const INTEGRATION_PROVIDER_KIND: &str = "integration.provider_kind";
pub const INTEGRATION_PROVIDER_KINDS: &str = "integration.provider_kinds";
pub const INTEGRATION_PROVIDER_USER_IDS: &str = "integration.provider_user_ids";

// Synchronization
pub const SYNC_SOURCE_KIND: &str = "sync.source_kind";
pub const SYNC_TYPE: &str = "sync.type";
pub const SYNC_FORCE: &str = "sync.force";
pub const SYNC_INCREMENTAL: &str = "sync.incremental";
pub const SYNC_ALL_SOURCES: &str = "sync.all_sources";
pub const SYNC_ALL_USERS: &str = "sync.all_users";
pub const SYNC_MIN_INTERVAL_MINUTES: &str = "sync.min_interval_minutes";
pub const SYNC_ITEMS_COUNT: &str = "sync.items.count";
pub const SYNC_DELETED_ITEMS_COUNT: &str = "sync.items.deleted_count";

// Third party items
pub const THIRD_PARTY_ITEM_ID: &str = "third_party_item.id";
pub const THIRD_PARTY_ITEM_SOURCE_ID: &str = "third_party_item.source_id";
pub const THIRD_PARTY_ITEM_KIND: &str = "third_party_item.kind";

// Notifications
pub const NOTIFICATION_ID: &str = "notification.id";
pub const NOTIFICATION_IDS: &str = "notification.ids";
pub const NOTIFICATION_KIND: &str = "notification.kind";
pub const NOTIFICATION_STATUS: &str = "notification.status";
pub const NOTIFICATION_COUNT: &str = "notification.count";
pub const NOTIFICATION_LIST_SOURCES: &str = "notification.list.sources";
pub const NOTIFICATION_LIST_ORDER_BY: &str = "notification.list.order_by";
pub const NOTIFICATION_LIST_INCLUDE_SNOOZED: &str = "notification.list.include_snoozed";
pub const NOTIFICATION_UPDATE_SNOOZED_UNTIL: &str = "notification.update_snoozed_until";
pub const NOTIFICATION_APPLY_SIDE_EFFECTS: &str = "notification.apply_side_effects";
pub const NOTIFICATION_APPLY_TASK_SIDE_EFFECTS: &str = "notification.apply_task_side_effects";

// Tasks
pub const TASK_ID: &str = "task.id";
pub const TASK_IDS: &str = "task.ids";
pub const TASK_KIND: &str = "task.kind";
pub const TASK_STATUS: &str = "task.status";
pub const TASK_LIST_ONLY_SYNCED: &str = "task.list.only_synced";
pub const TASK_OVERWRITE_EXISTING_SINK_ITEM: &str = "task.overwrite_existing_sink_item";

// Background jobs
pub const JOB_ID: &str = "job.id";
pub const JOB_NAME: &str = "job.name";
pub const JOB_ATTEMPT: &str = "job.attempt";
pub const CRON_TICK: &str = "cron.tick";

// Emails
pub const EMAIL_SUBJECT: &str = "email.subject";

// MCP server
pub const MCP_TOOL_NAME: &str = "mcp.tool.name";
pub const MCP_ALLOWED_ORIGINS: &str = "mcp.allowed_origins";
pub const MCP_SUPPORTED_PROTOCOL_VERSIONS: &str = "mcp.supported_protocol_versions";

// Slack bridge
pub const SLACK_BRIDGE_ACTION_ID: &str = "slack_bridge.action.id";
pub const SLACK_BRIDGE_ACTION_TYPE: &str = "slack_bridge.action.type";

// Billing
pub const STRIPE_EVENT_ID: &str = "stripe.event.id";
pub const STRIPE_EVENT_TYPE: &str = "stripe.event.type";
pub const STRIPE_EVENT_RETENTION_DAYS: &str = "stripe.event.retention_days";

// Linear
pub const LINEAR_ISSUE_ID: &str = "linear.issue.id";

// Google Calendar
pub const GOOGLE_CALENDAR_RESPONSE_STATUS: &str = "google_calendar.response_status";

// Slack
pub const SLACK_TEAM_ID: &str = "slack.team.id";
pub const SLACK_USER_ID: &str = "slack.user.id";
pub const SLACK_CHANNEL_ID: &str = "slack.channel.id";
pub const SLACK_MESSAGE_TS: &str = "slack.message.ts";
pub const SLACK_MESSAGE_THREAD_TS: &str = "slack.message.thread_ts";
pub const SLACK_REACTION_NAME: &str = "slack.reaction.name";
pub const SLACK_SYNC_TYPE: &str = "slack.sync_type";
pub const SLACK_MESSAGE_SYNC_ENABLED: &str = "slack.message_sync_enabled";
pub const SLACK_EVENT_ID: &str = "slack.event.id";
pub const SLACK_EVENT_TYPE: &str = "slack.event.type";
pub const SLACK_EVENT_OUTCOME: &str = "slack.event.outcome";
pub const SLACK_EVENT_DISCARD_REASON: &str = "slack.event.discard_reason";
pub const SLACK_EVENT_QUEUE_REASON: &str = "slack.event.queue_reason";
pub const SLACK_MESSAGE_OUTCOME: &str = "slack.message.outcome";
pub const SLACK_MESSAGE_DISCARD_REASON: &str = "slack.message.discard_reason";
pub const SLACK_REACTION_OUTCOME: &str = "slack.reaction.outcome";
pub const SLACK_REACTION_DISCARD_REASON: &str = "slack.reaction.discard_reason";
pub const SLACK_FETCH_ITEM_TYPE: &str = "slack.fetch.item_type";
pub const SLACK_FETCH_OUTCOME: &str = "slack.fetch.outcome";
pub const SLACK_FETCH_SKIP_REASON: &str = "slack.fetch.skip_reason";
pub const SLACK_KNOWN_THREAD_ITEMS_COUNT: &str = "slack.known_thread_items.count";
pub const SLACK_MATCHED_INTEGRATION_CONNECTIONS_COUNT: &str =
    "slack.matched_integration_connections.count";
pub const SLACK_REFERENCED_USERS_COUNT: &str = "slack.referenced_users.count";
pub const SLACK_REVOKED_CONNECTIONS_COUNT: &str = "slack.revoked_connections.count";
