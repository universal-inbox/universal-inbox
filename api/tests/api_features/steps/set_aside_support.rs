#![allow(clippy::too_many_arguments)]

use chrono::{DateTime, TimeZone, Timelike, Utc};
use http::StatusCode;
use pretty_assertions::assert_eq;
use reqwest::{Client, Response as ReqwestResponse};
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
            github::GithubNotification,
            google_calendar::GoogleCalendarEvent,
            google_drive::GoogleDriveComment,
            google_mail::{GoogleMailLabel, GoogleMailThread},
            linear::LinearNotification,
            slack::SlackThread,
            ticktick::TickTickItem,
            todoist::TodoistItem,
        },
        item::{ThirdPartyItem, ThirdPartyItemData},
    },
    user::User,
    utils::default_value::DefaultValue,
};

use universal_inbox_api::{
    configuration::Settings,
    repository::{
        integration_connection::IntegrationConnectionRepository,
        notification::NotificationRepository, task::TaskRepository,
        third_party::ThirdPartyItemRepository,
    },
};

use crate::{
    common::settings,
    helpers::{
        TestedApp,
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            create_ticktick_integration_connection, github_oauth_credential,
            google_calendar_oauth_credential, google_drive_oauth_credential,
            google_mail_oauth_credential, linear_oauth_credential, slack_oauth_credential,
            todoist_oauth_credential,
        },
        notification::{
            github::{create_notification_from_github_notification, github_notification},
            google_calendar::{
                create_notification_from_google_calendar_event, google_calendar_event,
            },
            google_drive::{
                create_notification_from_google_drive_comment, google_drive_comment_123,
            },
            google_mail::{
                create_notification_from_google_mail_thread, google_mail_thread_get_123,
                google_mail_user_profile, raw_google_mail_thread_get_123,
            },
            linear::{
                create_notification_from_linear_notification, sync_linear_notifications_response,
            },
            list_notifications, list_notifications_response,
            slack::{create_notification_from_slack_thread, slack_thread},
            ticktick::create_notification_from_ticktick_item,
            todoist::create_notification_from_todoist_item,
        },
        task::{ticktick::ticktick_item, todoist::todoist_item},
    },
};

pub struct Scene<'a> {
    pub app: &'a TestedApp,
    pub client: Client,
    pub user: User,
}

pub fn provider_fixtures() -> ProviderFixtures {
    let linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();

    ProviderFixtures {
        settings: settings(),
        github_notification: github_notification(),
        linear_notification: linear_notifications[2].clone(),
        google_mail_thread: google_mail_thread_get_123(
            raw_google_mail_thread_get_123(),
            google_mail_user_profile(),
        ),
        google_drive_comment: google_drive_comment_123(),
        google_calendar_event: google_calendar_event(),
        slack_thread: slack_thread(),
        todoist_item: todoist_item(),
        ticktick_item: ticktick_item(),
    }
}

pub struct ProviderFixtures {
    pub settings: Settings,
    pub github_notification: Box<GithubNotification>,
    pub linear_notification: LinearNotification,
    pub google_mail_thread: GoogleMailThread,
    pub google_drive_comment: GoogleDriveComment,
    pub google_calendar_event: GoogleCalendarEvent,
    pub slack_thread: Box<SlackThread>,
    pub todoist_item: Box<TodoistItem>,
    pub ticktick_item: Box<TickTickItem>,
}

pub struct ProviderCase {
    pub connection: Box<IntegrationConnection>,
    pub notification: Box<Notification>,
    pub harmless_config: IntegrationConnectionConfig,
    pub scope_narrowing_config: Option<IntegrationConnectionConfig>,
    pub enabled_config: IntegrationConnectionConfig,
    pub notifications_off_config: Option<IntegrationConnectionConfig>,
}

pub async fn seed_provider(
    app: &Scene<'_>,
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
                app.app,
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
                app.app,
                &fixtures.linear_notification,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
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
                app.app,
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
                app.app,
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
                app.app,
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
                app.app,
                &fixtures.slack_thread,
                app.user.id,
                connection.id,
            )
            .await;

            let mut harmless_config = SlackConfig::enabled_as_notifications();
            harmless_config.reaction_config.completion_reaction_name =
                Some(SlackReactionName("white_check_mark".to_string()));
            let mut scope_narrowing_config = SlackConfig::enabled_as_notifications();
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
                app.app,
                &fixtures.todoist_item,
                app.user.id,
                connection.id,
            )
            .await;

            ProviderCase {
                connection,
                notification,
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
                app.app,
                app.user.id,
                &fixtures.settings,
                IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
                None,
            )
            .await;
            let notification = create_notification_from_ticktick_item(
                app.app,
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
                "{provider_kind} has no case: Notion does not create notifications yet, and \
                 `API` has no configuration a user can save"
            )
        }
    }
}

pub async fn create_provider_connection(
    app: &Scene<'_>,
    config: IntegrationConnectionConfig,
    credential: OAuthCredentialFixture,
    settings: &Settings,
) -> Box<IntegrationConnection> {
    create_and_mock_integration_connection(
        app.app,
        app.user.id,
        config,
        settings,
        credential,
        None,
        None,
    )
    .await
}

pub fn last_read_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
}

pub fn snoozed_until() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2027, 6, 7, 8, 9, 10).unwrap()
}

pub async fn triage_notification(
    app: &Scene<'_>,
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

pub async fn assert_config_updated(
    app: &Scene<'_>,
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

pub async fn assert_triage_state_preserved(app: &Scene<'_>, expected: &Notification) {
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

pub async fn assert_notification_set_aside(app: &Scene<'_>, expected: &Notification) {
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

pub async fn complete_notifications_sync(app: &Scene<'_>, provider_kind: IntegrationProviderKind) {
    let service = app.app.integration_connection_service.read().await;
    let mut transaction = app.app.repository.begin().await.unwrap();

    service
        .complete_notifications_sync_status(&mut transaction, provider_kind, app.user.id)
        .await
        .unwrap();

    transaction.commit().await.unwrap();
}

pub async fn complete_tasks_sync(app: &Scene<'_>, provider_kind: IntegrationProviderKind) {
    let service = app.app.integration_connection_service.read().await;
    let mut transaction = app.app.repository.begin().await.unwrap();

    service
        .complete_tasks_sync_status(&mut transaction, provider_kind, app.user.id)
        .await
        .unwrap();

    transaction.commit().await.unwrap();
}

pub async fn complete_restoring_sync(app: &Scene<'_>, provider_kind: IntegrationProviderKind) {
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

pub async fn reconnect_connection(
    app: &Scene<'_>,
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

pub async fn fetch_set_aside_at(
    app: &Scene<'_>,
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

pub async fn update_integration_connection_config_response(
    app: &Scene<'_>,
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
