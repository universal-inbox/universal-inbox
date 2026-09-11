#![allow(clippy::too_many_arguments)]

//! Saving an integration configuration saves the configuration and nothing
//! else: no notification is removed and none of the triage state a user
//! invested in it (`status`, `last_read_at`, `snoozed_until`, `task_id`) is
//! disturbed.
//!
//! The invariant is written once and the providers are cases, so a provider
//! added later shows up as a missing `#[case]` line rather than a missing file.

use chrono::{DateTime, TimeZone, Timelike, Utc};
use graphql_client::Response;
use http::StatusCode;
use reqwest::Response as ReqwestResponse;
use rstest::*;
use slack_morphism::SlackReactionName;
use uuid::Uuid;

use universal_inbox::{
    Page,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig,
            google_calendar::GoogleCalendarConfig,
            google_drive::GoogleDriveConfig,
            google_mail::GoogleMailConfig,
            linear::{LinearConfig, LinearSyncTaskConfig},
            slack::SlackConfig,
            ticktick::TickTickConfig,
            todoist::TodoistConfig,
        },
        provider::IntegrationProviderKind,
    },
    notification::{
        Notification, NotificationStatus, NotificationWithTask, service::NotificationPatch,
    },
    task::{CreateOrUpdateTaskRequest, PresetDueDate, TaskPriority, TaskSourceKind, TaskStatus},
    third_party::{
        integrations::{
            api::{APISource, WebPage},
            github::GithubNotification,
            google_calendar::GoogleCalendarEvent,
            google_drive::GoogleDriveComment,
            google_mail::{GoogleMailLabel, GoogleMailThread},
            linear::LinearNotification,
            slack::SlackThread,
            ticktick::TickTickItem,
            todoist::TodoistItem,
        },
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
    utils::default_value::DefaultValue,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::linear::graphql::notifications_query,
    repository::{
        integration_connection::IntegrationConnectionRepository,
        notification::NotificationRepository, task::TaskRepository,
        third_party::ThirdPartyItemRepository,
    },
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection,
        create_ticktick_integration_connection, get_integration_connection_per_provider,
        github_oauth_credential, google_calendar_oauth_credential, google_drive_oauth_credential,
        google_mail_oauth_credential, linear_oauth_credential, slack_oauth_credential,
        todoist_oauth_credential,
    },
    notification::{
        github::{create_notification_from_github_notification, github_notification},
        google_calendar::{create_notification_from_google_calendar_event, google_calendar_event},
        google_drive::{create_notification_from_google_drive_comment, google_drive_comment_123},
        google_mail::{create_notification_from_google_mail_thread, google_mail_thread_get_123},
        linear::{
            create_notification_from_linear_notification, sync_linear_notifications_response,
        },
        list_notifications, list_notifications_response,
        slack::{create_notification_from_slack_thread, slack_thread},
        ticktick::create_notification_from_ticktick_item,
        todoist::create_notification_from_todoist_item,
    },
    rest::{create_resource, delete_resource},
    settings,
    task::{ticktick::ticktick_item, todoist::todoist_item},
};

/// One notification per provider, triaged, then a configuration `PUT`. Nothing
/// may be removed and nothing may be disturbed — for any provider, any setting.
///
/// `Notion` and `API` have no case: neither is a connection a user can
/// configure. `Notion` is not even a value of the `integration_provider_kind`
/// Postgres enum, so such a connection cannot be persisted at all.
#[rstest]
#[case(IntegrationProviderKind::Github)]
#[case(IntegrationProviderKind::Linear)]
#[case(IntegrationProviderKind::GoogleMail)]
#[case(IntegrationProviderKind::GoogleDrive)]
#[case(IntegrationProviderKind::GoogleCalendar)]
#[case(IntegrationProviderKind::Slack)]
#[case(IntegrationProviderKind::Todoist)]
#[case(IntegrationProviderKind::TickTick)]
#[tokio::test]
async fn test_config_update_preserves_triage_state(
    #[case] provider_kind: IntegrationProviderKind,
    #[future] authenticated_app: AuthenticatedApp,
    provider_fixtures: ProviderFixtures,
) {
    let app = authenticated_app.await;

    let case = seed_provider(&app, provider_kind, &provider_fixtures).await;
    let triaged_notification =
        triage_notification(&app, &case.notification, &provider_fixtures.todoist_item).await;

    // A change with no bearing on which notifications belong in the inbox
    assert_config_updated(&app, case.connection.id, &case.harmless_config).await;
    assert_triage_state_preserved(&app, &triaged_notification).await;

    // A change that narrows what belongs in the inbox is just as harmless at
    // save time: reconciling the inbox is the following sync's job.
    if let Some(scope_narrowing_config) = case.scope_narrowing_config {
        assert_config_updated(&app, case.connection.id, &scope_narrowing_config).await;
        assert_triage_state_preserved(&app, &triaged_notification).await;
    }
}

/// Switching an integration's notifications off takes them out of the inbox
/// immediately, so the screen matches what the user just asked for — but they
/// are set aside rather than removed, and reading one by id still resolves.
/// Switching it back on brings them back exactly as they were, once the sync
/// that reconciles the inbox has completed.
#[rstest]
#[case(IntegrationProviderKind::Github)]
#[case(IntegrationProviderKind::Linear)]
#[case(IntegrationProviderKind::GoogleMail)]
#[case(IntegrationProviderKind::GoogleDrive)]
#[case(IntegrationProviderKind::Slack)]
#[tokio::test]
async fn test_mute_sets_aside_and_unmute_restores(
    #[case] provider_kind: IntegrationProviderKind,
    #[future] authenticated_app: AuthenticatedApp,
    provider_fixtures: ProviderFixtures,
) {
    let app = authenticated_app.await;

    let case = seed_provider(&app, provider_kind, &provider_fixtures).await;
    let triaged_notification =
        triage_notification(&app, &case.notification, &provider_fixtures.todoist_item).await;
    let muted_config = case
        .notifications_off_config
        .unwrap_or_else(|| panic!("{provider_kind} is expected to own a notification mute toggle"));

    assert_config_updated(&app, case.connection.id, &muted_config).await;
    assert_notification_set_aside(&app, &triaged_notification).await;

    // Nothing comes back on the save itself: what is restored has to be a
    // reconciled inbox rather than an archive, so it waits for a successful sync.
    assert_config_updated(&app, case.connection.id, &case.enabled_config).await;
    assert_notification_set_aside(&app, &triaged_notification).await;

    complete_notifications_sync(&app, provider_kind).await;

    assert_triage_state_preserved(&app, &triaged_notification).await;
}

/// Todoist and TickTick notifications are a byproduct of their task sync, and
/// neither has a notifications sync of its own to restore them. Switching their
/// synchronization off must therefore leave the already collected notifications
/// in the inbox rather than setting them aside for good.
#[rstest]
#[case(IntegrationProviderKind::Todoist)]
#[case(IntegrationProviderKind::TickTick)]
#[tokio::test]
async fn test_mute_does_not_set_aside_task_derived_notifications(
    #[case] provider_kind: IntegrationProviderKind,
    #[future] authenticated_app: AuthenticatedApp,
    provider_fixtures: ProviderFixtures,
) {
    let app = authenticated_app.await;

    let case = seed_provider(&app, provider_kind, &provider_fixtures).await;
    let triaged_notification =
        triage_notification(&app, &case.notification, &provider_fixtures.todoist_item).await;
    let disabled_config = case
        .notifications_off_config
        .unwrap_or_else(|| panic!("{provider_kind} is expected to have an off switch"));

    assert_config_updated(&app, case.connection.id, &disabled_config).await;

    assert_triage_state_preserved(&app, &triaged_notification).await;
}

/// Disconnecting is the same user situation as muting, one notch stronger, so
/// it must not be the more destructive one: its notifications leave the inbox
/// and are set aside, and reconnecting brings them back exactly as they were
/// once the sync that reconciles them has completed.
///
/// `Notion` has no notifications of its own, and `API` notifications are pushed
/// in by an external client rather than collected by a sync, so nothing could
/// ever restore them — neither is set aside, and neither has a case here.
#[rstest]
#[case(IntegrationProviderKind::Github)]
#[case(IntegrationProviderKind::Linear)]
#[case(IntegrationProviderKind::GoogleMail)]
#[case(IntegrationProviderKind::GoogleDrive)]
#[case(IntegrationProviderKind::GoogleCalendar)]
#[case(IntegrationProviderKind::Slack)]
#[case(IntegrationProviderKind::Todoist)]
#[case(IntegrationProviderKind::TickTick)]
#[tokio::test]
async fn test_disconnect_sets_aside_and_reconnect_restores(
    #[case] provider_kind: IntegrationProviderKind,
    #[future] authenticated_app: AuthenticatedApp,
    provider_fixtures: ProviderFixtures,
) {
    let app = authenticated_app.await;

    let case = seed_provider(&app, provider_kind, &provider_fixtures).await;
    let triaged_notification =
        triage_notification(&app, &case.notification, &provider_fixtures.todoist_item).await;

    let disconnected_connection: Box<IntegrationConnection> = delete_resource(
        &app.client,
        &app.app.api_address,
        "integration-connections",
        case.connection.id.into(),
    )
    .await;
    assert_eq!(
        disconnected_connection.status,
        IntegrationConnectionStatus::Created
    );

    assert_notification_set_aside(&app, &triaged_notification).await;

    // Reconnecting is an OAuth round trip that enqueues no sync of its own, so
    // nothing comes back until the interval-throttled sync the next inbox load
    // enqueues has reconciled the inbox.
    reconnect_connection(&app, case.connection.id).await;
    assert_notification_set_aside(&app, &triaged_notification).await;

    complete_restoring_sync(&app, provider_kind).await;

    assert_triage_state_preserved(&app, &triaged_notification).await;
}

/// `API` notifications are pushed in by an external client rather than collected
/// by a synchronization, so no sync completion could ever bring them back.
/// Disconnecting an `API` connection therefore leaves them in the inbox, rather
/// than setting them aside for good.
#[rstest]
#[tokio::test]
async fn test_disconnect_does_not_set_aside_api_notifications(
    #[future] authenticated_app: AuthenticatedApp,
    provider_fixtures: ProviderFixtures,
) {
    let app = authenticated_app.await;

    // Pushing a web page through the API creates the `API` connection along with
    // the notification.
    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &app.client,
        &app.app.api_address,
        "third_party/notification/items",
        Box::new(ThirdPartyItemData::WebPage(Box::new(WebPage {
            url: "https://www.universal-inbox.com".parse().unwrap(),
            title: "Universal Inbox".to_string(),
            timestamp: Utc::now(),
            source: APISource::UniversalInboxExtension,
            favicon: None,
        }))),
    )
    .await;
    let notification = creation
        .notification
        .expect("Expected an API notification to be created");
    let triaged_notification =
        triage_notification(&app, &notification, &provider_fixtures.todoist_item).await;

    let api_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::API,
        None,
        None,
    )
    .await
    .expect("Expected an API integration connection to have been created");

    let disconnected_connection: Box<IntegrationConnection> = delete_resource(
        &app.client,
        &app.app.api_address,
        "integration-connections",
        api_connection.id.into(),
    )
    .await;
    assert_eq!(
        disconnected_connection.status,
        IntegrationConnectionStatus::Created
    );

    assert_triage_state_preserved(&app, &triaged_notification).await;
}

/// The API moves connections to `Failing` on its own when a refresh token
/// expires, so only a user's own action may set notifications aside: a failing
/// connection keeps its notifications, which are still work the user can read,
/// snooze or dismiss.
#[rstest]
#[tokio::test]
async fn test_failing_connection_keeps_notifications_visible(
    #[future] authenticated_app: AuthenticatedApp,
    provider_fixtures: ProviderFixtures,
) {
    let app = authenticated_app.await;

    let case = seed_provider(&app, IntegrationProviderKind::Github, &provider_fixtures).await;
    let triaged_notification =
        triage_notification(&app, &case.notification, &provider_fixtures.todoist_item).await;

    let mut transaction = app.app.repository.begin().await.unwrap();
    app.app
        .repository
        .update_integration_connection_status(
            &mut transaction,
            case.connection.id,
            IntegrationConnectionStatus::Failing,
            Some("Failed to refresh the access token".to_string()),
            None,
            app.user.id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert_config_updated(&app, case.connection.id, &case.enabled_config).await;

    assert_triage_state_preserved(&app, &triaged_notification).await;
}

/// Reconciling is idempotent: a second save while the integration is still muted
/// matches no visible notification, so an existing `set_aside_at` is never
/// overwritten with a fresher timestamp.
#[rstest]
#[tokio::test]
async fn test_repeated_mute_does_not_refresh_set_aside_at(
    #[future] authenticated_app: AuthenticatedApp,
    provider_fixtures: ProviderFixtures,
) {
    let app = authenticated_app.await;

    let case = seed_provider(&app, IntegrationProviderKind::Slack, &provider_fixtures).await;
    let triaged_notification =
        triage_notification(&app, &case.notification, &provider_fixtures.todoist_item).await;

    assert_config_updated(
        &app,
        case.connection.id,
        &IntegrationConnectionConfig::Slack(SlackConfig::disabled()),
    )
    .await;
    let set_aside_at = fetch_set_aside_at(&app, &triaged_notification).await;
    assert!(
        set_aside_at.is_some(),
        "Expected the Slack notification to have been set aside"
    );

    // Another save that leaves the integration muted
    let mut still_muted_config = SlackConfig::disabled();
    still_muted_config.reaction_config.completion_reaction_name =
        Some(SlackReactionName("white_check_mark".to_string()));
    assert_config_updated(
        &app,
        case.connection.id,
        &IntegrationConnectionConfig::Slack(still_muted_config),
    )
    .await;

    assert_eq!(
        fetch_set_aside_at(&app, &triaged_notification).await,
        set_aside_at,
        "Expected the notification to keep the moment it was set aside"
    );
}

struct ProviderFixtures {
    settings: Settings,
    github_notification: Box<GithubNotification>,
    linear_notification: LinearNotification,
    google_mail_thread: GoogleMailThread,
    google_drive_comment: GoogleDriveComment,
    google_calendar_event: GoogleCalendarEvent,
    slack_thread: Box<SlackThread>,
    todoist_item: Box<TodoistItem>,
    ticktick_item: Box<TickTickItem>,
}

#[fixture]
fn provider_fixtures(
    settings: Settings,
    github_notification: Box<GithubNotification>,
    sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    google_mail_thread_get_123: GoogleMailThread,
    google_drive_comment_123: GoogleDriveComment,
    google_calendar_event: GoogleCalendarEvent,
    slack_thread: Box<SlackThread>,
    todoist_item: Box<TodoistItem>,
    ticktick_item: Box<TickTickItem>,
) -> ProviderFixtures {
    let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
        .data
        .unwrap()
        .try_into()
        .unwrap();

    ProviderFixtures {
        settings,
        github_notification,
        // An `IssueNotification`, as the other Linear notification tests use
        linear_notification: linear_notifications[2].clone(),
        google_mail_thread: google_mail_thread_get_123,
        google_drive_comment: google_drive_comment_123,
        google_calendar_event,
        slack_thread,
        todoist_item,
        ticktick_item,
    }
}

struct ProviderCase {
    /// The connection whose configuration is saved.
    connection: Box<IntegrationConnection>,
    /// A notification that must survive that save.
    notification: Box<Notification>,
    /// A configuration change that does not touch what belongs in the inbox.
    /// Some providers (Github, Google Drive) expose nothing but a notification
    /// mute toggle, so their harmless payload is the stored one.
    harmless_config: IntegrationConnectionConfig,
    /// A configuration change that narrows what belongs in the inbox, where the
    /// provider has one. Muting is deliberately not exercised here: it is not a
    /// scope change but a statement about what the user wants to see, and it has
    /// its own coverage.
    scope_narrowing_config: Option<IntegrationConnectionConfig>,
    /// The stored configuration, with whatever the provider offers switched on.
    enabled_config: IntegrationConnectionConfig,
    /// The same configuration with the integration switched off, where the
    /// provider has such a control. Only the five providers whose configuration
    /// carries a notification sync flag actually mute their notifications; for
    /// Todoist and TickTick this is the off switch of their *task* sync, which
    /// must leave the already collected notifications alone.
    notifications_off_config: Option<IntegrationConnectionConfig>,
}

async fn seed_provider(
    app: &AuthenticatedApp,
    provider_kind: IntegrationProviderKind,
    fixtures: &ProviderFixtures,
) -> ProviderCase {
    match provider_kind {
        IntegrationProviderKind::Github => {
            let connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::Github(GithubConfig::enabled()),
                github_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let notification = create_notification_from_github_notification(
                &app.app,
                &fixtures.github_notification,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
                harmless_config: IntegrationConnectionConfig::Github(GithubConfig::enabled()),
                scope_narrowing_config: None,
                enabled_config: IntegrationConnectionConfig::Github(GithubConfig::enabled()),
                notifications_off_config: Some(IntegrationConnectionConfig::Github(
                    GithubConfig::disabled(),
                )),
            }
        }
        IntegrationProviderKind::Linear => {
            let connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::Linear(LinearConfig::default()),
                linear_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let notification = create_notification_from_linear_notification(
                &app.app,
                &fixtures.linear_notification,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
                // A default for tasks created from Linear issues: nothing to do
                // with which notifications are collected
                harmless_config: IntegrationConnectionConfig::Linear(LinearConfig {
                    sync_task_config: LinearSyncTaskConfig {
                        default_due_at: Some(PresetDueDate::Today),
                        ..LinearSyncTaskConfig::default()
                    },
                    ..LinearConfig::default()
                }),
                scope_narrowing_config: None,
                enabled_config: IntegrationConnectionConfig::Linear(LinearConfig::default()),
                notifications_off_config: Some(IntegrationConnectionConfig::Linear(LinearConfig {
                    sync_notifications_enabled: false,
                    ..LinearConfig::default()
                })),
            }
        }
        IntegrationProviderKind::GoogleMail => {
            let connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
                google_mail_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let notification = create_notification_from_google_mail_thread(
                &app.app,
                &fixtures.google_mail_thread,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
                harmless_config: IntegrationConnectionConfig::GoogleMail(
                    GoogleMailConfig::enabled(),
                ),
                // Synchronizing another label narrows what belongs in the inbox
                scope_narrowing_config: Some(IntegrationConnectionConfig::GoogleMail(
                    GoogleMailConfig {
                        sync_notifications_enabled: true,
                        synced_label: GoogleMailLabel {
                            id: "Label_2".to_string(),
                            name: "Label 2".to_string(),
                        },
                    },
                )),
                enabled_config: IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
                notifications_off_config: Some(IntegrationConnectionConfig::GoogleMail(
                    GoogleMailConfig::disabled(),
                )),
            }
        }
        IntegrationProviderKind::GoogleDrive => {
            let connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
                google_drive_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let notification = create_notification_from_google_drive_comment(
                &app.app,
                &fixtures.google_drive_comment,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
                harmless_config: IntegrationConnectionConfig::GoogleDrive(
                    GoogleDriveConfig::enabled(),
                ),
                scope_narrowing_config: None,
                enabled_config: IntegrationConnectionConfig::GoogleDrive(
                    GoogleDriveConfig::enabled(),
                ),
                notifications_off_config: Some(IntegrationConnectionConfig::GoogleDrive(
                    GoogleDriveConfig::disabled(),
                )),
            }
        }
        IntegrationProviderKind::GoogleCalendar => {
            // Google Calendar notifications are derived during the Google Mail
            // sync, so the notification hangs off both connections.
            let google_mail_connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
                google_mail_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
                google_calendar_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let notification = create_notification_from_google_calendar_event(
                &app.app,
                &fixtures.google_mail_thread,
                &fixtures.google_calendar_event,
                app.user.id,
                google_mail_connection.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
                harmless_config: IntegrationConnectionConfig::GoogleCalendar(
                    GoogleCalendarConfig::enabled(),
                ),
                scope_narrowing_config: Some(IntegrationConnectionConfig::GoogleCalendar(
                    GoogleCalendarConfig::disabled(),
                )),
                enabled_config: IntegrationConnectionConfig::GoogleCalendar(
                    GoogleCalendarConfig::enabled(),
                ),
                // Google Calendar notifications are derived during the Google
                // Mail sync: the connection carries no notification mute toggle
                // of its own.
                notifications_off_config: None,
            }
        }
        IntegrationProviderKind::Slack => {
            let connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
                slack_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let notification = create_notification_from_slack_thread(
                &app.app,
                &fixtures.slack_thread,
                app.user.id,
                connection.id,
            )
            .await;

            let mut harmless_config = SlackConfig::enabled_as_notifications();
            // The reaction added when a task is completed: it collects nothing
            harmless_config.reaction_config.completion_reaction_name =
                Some(SlackReactionName("white_check_mark".to_string()));
            let mut scope_narrowing_config = SlackConfig::enabled_as_notifications();
            // Watching another reaction narrows what belongs in the inbox
            scope_narrowing_config.reaction_config.reaction_name =
                SlackReactionName("bookmark".to_string());

            ProviderCase {
                connection,
                notification,
                harmless_config: IntegrationConnectionConfig::Slack(harmless_config),
                scope_narrowing_config: Some(IntegrationConnectionConfig::Slack(
                    scope_narrowing_config,
                )),
                enabled_config: IntegrationConnectionConfig::Slack(
                    SlackConfig::enabled_as_notifications(),
                ),
                // Slack's mute lives in `message_config.sync_enabled` rather than
                // in a `sync_notifications_enabled` field
                notifications_off_config: Some(IntegrationConnectionConfig::Slack(
                    SlackConfig::disabled(),
                )),
            }
        }
        IntegrationProviderKind::Todoist => {
            let connection = create_provider_connection(
                app,
                IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
                todoist_oauth_credential(),
                &fixtures.settings,
            )
            .await;
            let notification = create_notification_from_todoist_item(
                &app.app,
                &fixtures.todoist_item,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
                // A default for newly created tasks: no bearing on notifications
                harmless_config: IntegrationConnectionConfig::Todoist(TodoistConfig {
                    default_priority: Some(TaskPriority::P2),
                    ..TodoistConfig::enabled()
                }),
                scope_narrowing_config: Some(IntegrationConnectionConfig::Todoist(TodoistConfig {
                    create_notification_from_inbox_task: false,
                    ..TodoistConfig::enabled()
                })),
                enabled_config: IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
                notifications_off_config: Some(IntegrationConnectionConfig::Todoist(
                    TodoistConfig::disabled(),
                )),
            }
        }
        IntegrationProviderKind::TickTick => {
            let connection = create_ticktick_integration_connection(
                &app.app,
                app.user.id,
                &fixtures.settings,
                IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
                None,
            )
            .await;
            let notification = create_notification_from_ticktick_item(
                &app.app,
                &fixtures.ticktick_item,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
                harmless_config: IntegrationConnectionConfig::TickTick(TickTickConfig {
                    default_priority: Some(TaskPriority::P2),
                    ..TickTickConfig::enabled()
                }),
                scope_narrowing_config: Some(IntegrationConnectionConfig::TickTick(
                    TickTickConfig {
                        create_notification_from_inbox_task: false,
                        ..TickTickConfig::enabled()
                    },
                )),
                enabled_config: IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
                notifications_off_config: Some(IntegrationConnectionConfig::TickTick(
                    TickTickConfig::disabled(),
                )),
            }
        }
        IntegrationProviderKind::Notion | IntegrationProviderKind::API => {
            unreachable!(
                "{provider_kind} has no configuration a user can save: `Notion` is not even a \
                 value of the `integration_provider_kind` Postgres enum, so such a connection \
                 cannot be persisted"
            )
        }
    }
}

async fn create_provider_connection(
    app: &AuthenticatedApp,
    config: IntegrationConnectionConfig,
    credential: OAuthCredentialFixture,
    settings: &Settings,
) -> Box<IntegrationConnection> {
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        config,
        settings,
        credential,
        None,
        None,
    )
    .await
}

fn last_read_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
}

fn snoozed_until() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2027, 6, 7, 8, 9, 10).unwrap()
}

/// Invest triage work in the notification: read it, snooze it, and link it to a
/// task. Returns the notification as it stands afterwards, which is exactly
/// what a configuration save must give back.
async fn triage_notification(
    app: &AuthenticatedApp,
    notification: &Notification,
    todoist_item: &TodoistItem,
) -> Notification {
    let mut transaction = app.app.repository.begin().await.unwrap();

    let read_notification = app
        .app
        .repository
        .create_or_update_notification(
            &mut transaction,
            Box::new(Notification {
                status: NotificationStatus::Read,
                last_read_at: Some(last_read_at()),
                snoozed_until: Some(snoozed_until()),
                ..notification.clone()
            }),
            notification.kind,
            true,
        )
        .await
        .unwrap()
        .value();

    let task_source_id = format!("task-for-{}", notification.id);
    let task_source_item = app
        .app
        .repository
        .create_or_update_third_party_item(
            &mut transaction,
            Box::new(ThirdPartyItem::new(
                task_source_id.clone(),
                ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    id: task_source_id.clone(),
                    ..todoist_item.clone()
                })),
                notification.user_id,
                notification.source_item.integration_connection_id,
            )),
        )
        .await
        .unwrap()
        .value();

    let task = app
        .app
        .repository
        .create_or_update_task(
            &mut transaction,
            Box::new(CreateOrUpdateTaskRequest {
                id: Uuid::new_v4().into(),
                title: DefaultValue::new(
                    "Task created from a triaged notification".to_string(),
                    None,
                ),
                body: "".to_string(),
                status: TaskStatus::Active,
                completed_at: None,
                priority: TaskPriority::P4,
                due_at: DefaultValue::new(None, None),
                tags: vec![],
                parent_id: None,
                project: DefaultValue::new("Inbox".to_string(), None),
                is_recurring: false,
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                kind: TaskSourceKind::Todoist,
                source_item: *task_source_item,
                sink_item: None,
                user_id: notification.user_id,
            }),
        )
        .await
        .unwrap()
        .value();

    let triaged_notification = app
        .app
        .repository
        .update_notification(
            &mut transaction,
            read_notification.id,
            &NotificationPatch {
                task_id: Some(task.id),
                ..Default::default()
            },
            notification.user_id,
        )
        .await
        .unwrap()
        .result
        .unwrap();

    transaction.commit().await.unwrap();

    assert_eq!(triaged_notification.status, NotificationStatus::Read);
    assert_eq!(triaged_notification.last_read_at, Some(last_read_at()));
    assert_eq!(triaged_notification.snoozed_until, Some(snoozed_until()));
    assert_eq!(triaged_notification.task_id, Some(task.id));

    *triaged_notification
}

async fn assert_config_updated(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
    config: &IntegrationConnectionConfig,
) {
    let response =
        update_integration_connection_config_response(app, integration_connection_id, config).await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_config: Box<IntegrationConnectionConfig> =
        response.json().await.expect("Failed to parse JSON result");
    assert_eq!(*updated_config, *config);
}

/// The notification is still in the inbox, is still the same row, and still
/// carries the triage state it had before the configuration was saved.
async fn assert_triage_state_preserved(app: &AuthenticatedApp, expected: &Notification) {
    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![],
        true,
        None,
        Some(expected.kind),
        false,
    )
    .await;

    assert_eq!(
        notifications.len(),
        1,
        "Expected the {} notification to still be in the inbox",
        expected.kind
    );
    let notification = &notifications[0];
    assert_eq!(notification.id, expected.id);
    assert_eq!(notification.source_item.id, expected.source_item.id);
    assert_eq!(notification.status, expected.status);
    assert_eq!(notification.last_read_at, expected.last_read_at);
    assert_eq!(notification.snoozed_until, expected.snoozed_until);
    assert_eq!(notification.task_id, expected.task_id);
}

/// The notification is out of the inbox and out of its count, but it is not
/// gone: it is still the same row, still carries its triage state, and reading
/// it by id — a bookmarked link, or the notification a task points at — still
/// resolves rather than answering `404`.
async fn assert_notification_set_aside(app: &AuthenticatedApp, expected: &Notification) {
    let notifications_page: Page<NotificationWithTask> = list_notifications_response(
        &app.client,
        &app.app.api_address,
        vec![],
        true,
        None,
        Some(expected.kind),
        false,
    )
    .await
    .json()
    .await
    .expect("Cannot parse JSON result");

    assert_eq!(
        notifications_page.content.len(),
        0,
        "Expected the {} notification to be out of the inbox",
        expected.kind
    );
    assert_eq!(
        notifications_page.total, 0,
        "Expected the {} notification to be out of the inbox count",
        expected.kind
    );

    let response = app
        .client
        .get(format!(
            "{}notifications/{}",
            app.app.api_address, expected.id
        ))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected the set aside {} notification to still resolve by id",
        expected.kind
    );
    let notification: Box<NotificationWithTask> =
        response.json().await.expect("Failed to parse JSON result");
    assert_eq!(notification.id, expected.id);
    assert_eq!(notification.status, expected.status);
    assert_eq!(notification.last_read_at, expected.last_read_at);
    assert_eq!(notification.snoozed_until, expected.snoozed_until);
}

/// The moment a successful notifications sync completed for that provider, which
/// is what brings back whatever was set aside.
async fn complete_notifications_sync(
    app: &AuthenticatedApp,
    provider_kind: IntegrationProviderKind,
) {
    let service = app.app.integration_connection_service.read().await;
    let mut transaction = app.app.repository.begin().await.unwrap();

    service
        .complete_notifications_sync_status(&mut transaction, provider_kind, app.user.id)
        .await
        .unwrap();

    transaction.commit().await.unwrap();
}

/// The moment a successful tasks sync completed for that provider.
async fn complete_tasks_sync(app: &AuthenticatedApp, provider_kind: IntegrationProviderKind) {
    let service = app.app.integration_connection_service.read().await;
    let mut transaction = app.app.repository.begin().await.unwrap();

    service
        .complete_tasks_sync_status(&mut transaction, provider_kind, app.user.id)
        .await
        .unwrap();

    transaction.commit().await.unwrap();
}

/// The sync completion that reconciles a provider's notifications, and so the
/// one that brings back whatever it set aside. Only five notification kinds have
/// a notifications sync of their own: Todoist and TickTick notifications are a
/// byproduct of their *task* sync, and Google Calendar notifications are derived
/// during the Google Mail sync.
async fn complete_restoring_sync(app: &AuthenticatedApp, provider_kind: IntegrationProviderKind) {
    match provider_kind {
        IntegrationProviderKind::Todoist | IntegrationProviderKind::TickTick => {
            complete_tasks_sync(app, provider_kind).await
        }
        IntegrationProviderKind::GoogleCalendar => {
            complete_notifications_sync(app, IntegrationProviderKind::GoogleMail).await
        }
        _ => complete_notifications_sync(app, provider_kind).await,
    }
}

/// What a successful OAuth round trip leaves behind: the connection is
/// `Validated` again, and no sync has run yet.
async fn reconnect_connection(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
) {
    let mut transaction = app.app.repository.begin().await.unwrap();
    app.app
        .repository
        .update_integration_connection_status(
            &mut transaction,
            integration_connection_id,
            IntegrationConnectionStatus::Validated,
            None,
            None,
            app.user.id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

async fn fetch_set_aside_at(
    app: &AuthenticatedApp,
    notification: &Notification,
) -> Option<DateTime<Utc>> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT set_aside_at FROM notification WHERE id = $1",
    )
    .bind(notification.id.0)
    .fetch_one(&*app.app.repository.pool)
    .await
    .expect("Failed to fetch the notification set aside timestamp")
}

async fn update_integration_connection_config_response(
    app: &AuthenticatedApp,
    integration_connection_id: IntegrationConnectionId,
    config: &IntegrationConnectionConfig,
) -> ReqwestResponse {
    app.client
        .put(format!(
            "{}integration-connections/{}/config",
            app.app.api_address, integration_connection_id
        ))
        .json(config)
        .send()
        .await
        .expect("Failed to execute request")
}
