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
    pub kind: IntegrationProviderKind,
    pub connection: Box<IntegrationConnection>,
    pub notification: Box<Notification>,
    pub harmless_config: IntegrationConnectionConfig,
    pub scope_narrowing_config: Option<IntegrationConnectionConfig>,
    pub notifications_off_config: Option<IntegrationConnectionConfig>,
}

impl ProviderCase {
    pub fn enabled_config(&self) -> IntegrationConnectionConfig {
        self.connection.provider.config()
    }
}

pub async fn seed_provider(
    scene: &Scene<'_>,
    kind: IntegrationProviderKind,
    fixtures: &ProviderFixtures,
) -> Result<ProviderCase, String> {
    let case = match kind {
        IntegrationProviderKind::Github => github_case(scene, fixtures).await,
        IntegrationProviderKind::Linear => linear_case(scene, fixtures).await,
        IntegrationProviderKind::GoogleMail => google_mail_case(scene, fixtures).await,
        IntegrationProviderKind::GoogleDrive => google_drive_case(scene, fixtures).await,
        IntegrationProviderKind::GoogleCalendar => google_calendar_case(scene, fixtures).await,
        IntegrationProviderKind::Slack => slack_case(scene, fixtures).await,
        IntegrationProviderKind::Todoist => todoist_case(scene, fixtures).await,
        IntegrationProviderKind::TickTick => ticktick_case(scene, fixtures).await,
        IntegrationProviderKind::Notion | IntegrationProviderKind::API => {
            return Err(format!(
                "{kind:?} has no scenario: Notion does not create notifications yet, and `API` \
                 has no configuration a user can save"
            ));
        }
    };
    Ok(case)
}

pub async fn create_provider_connection(
    scene: &Scene<'_>,
    config: IntegrationConnectionConfig,
    credential: OAuthCredentialFixture,
    settings: &Settings,
) -> Box<IntegrationConnection> {
    create_and_mock_integration_connection(
        scene.app,
        scene.user.id,
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
    scene: &Scene<'_>,
    notification: &Notification,
    todoist_item: &TodoistItem,
) -> Notification {
    let mut transaction = scene.app.repository.begin().await.unwrap();

    let read_notification = scene
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
    let task_source_item = scene
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

    let task = scene
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

    let triaged_notification = scene
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
    scene: &Scene<'_>,
    integration_connection_id: IntegrationConnectionId,
    config: &IntegrationConnectionConfig,
) {
    let response =
        update_integration_connection_config_response(scene, integration_connection_id, config)
            .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated_config: Box<IntegrationConnectionConfig> =
        response.json().await.expect("Failed to parse JSON result");
    assert_eq!(*updated_config, *config);
}

pub async fn assert_triage_state_preserved(scene: &Scene<'_>, expected: &Notification) {
    let notifications = list_notifications(
        &scene.client,
        &scene.app.api_address,
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

pub async fn assert_notification_set_aside(scene: &Scene<'_>, expected: &Notification) {
    let notifications_page: Page<NotificationWithTask> = list_notifications_response(
        &scene.client,
        &scene.app.api_address,
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

    let response = scene
        .client
        .get(format!(
            "{}notifications/{}",
            scene.app.api_address, expected.id
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

pub enum Sync {
    Notifications,
    Tasks,
}

pub async fn complete_sync(scene: &Scene<'_>, sync: Sync, provider_kind: IntegrationProviderKind) {
    let service = scene.app.integration_connection_service.read().await;
    let mut transaction = scene.app.repository.begin().await.unwrap();

    match sync {
        Sync::Notifications => {
            service
                .complete_notifications_sync_status(&mut transaction, provider_kind, scene.user.id)
                .await
        }
        Sync::Tasks => {
            service
                .complete_tasks_sync_status(&mut transaction, provider_kind, scene.user.id)
                .await
        }
    }
    .unwrap();

    transaction.commit().await.unwrap();
}

pub async fn complete_restoring_sync(scene: &Scene<'_>, provider_kind: IntegrationProviderKind) {
    match provider_kind {
        IntegrationProviderKind::Todoist | IntegrationProviderKind::TickTick => {
            complete_sync(scene, Sync::Tasks, provider_kind).await
        }
        IntegrationProviderKind::GoogleCalendar => {
            complete_sync(
                scene,
                Sync::Notifications,
                IntegrationProviderKind::GoogleMail,
            )
            .await
        }
        _ => complete_sync(scene, Sync::Notifications, provider_kind).await,
    }
}

pub async fn reconnect_connection(
    scene: &Scene<'_>,
    integration_connection_id: IntegrationConnectionId,
) {
    let mut transaction = scene.app.repository.begin().await.unwrap();
    scene
        .app
        .repository
        .update_integration_connection_status(
            &mut transaction,
            integration_connection_id,
            IntegrationConnectionStatus::Validated,
            None,
            None,
            scene.user.id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

pub async fn fetch_set_aside_at(
    scene: &Scene<'_>,
    notification: &Notification,
) -> Option<DateTime<Utc>> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT set_aside_at FROM notification WHERE id = $1",
    )
    .bind(notification.id.0)
    .fetch_one(&*scene.app.repository.pool)
    .await
    .expect("Failed to fetch the notification set aside timestamp")
}

pub async fn update_integration_connection_config_response(
    scene: &Scene<'_>,
    integration_connection_id: IntegrationConnectionId,
    config: &IntegrationConnectionConfig,
) -> ReqwestResponse {
    scene
        .client
        .put(format!(
            "{}integration-connections/{}/config",
            scene.app.api_address, integration_connection_id
        ))
        .json(config)
        .send()
        .await
        .expect("Failed to execute request")
}

async fn github_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        github_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let notification = create_notification_from_github_notification(
        scene.app,
        &fixtures.github_notification,
        scene.user.id,
        connection.id,
    )
    .await;

    ProviderCase {
        kind: IntegrationProviderKind::Github,
        connection,
        notification,
        harmless_config: IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        scope_narrowing_config: None,
        notifications_off_config: Some(IntegrationConnectionConfig::Github(
            GithubConfig::disabled(),
        )),
    }
}

async fn linear_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::Linear(LinearConfig::default()),
        linear_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let notification = create_notification_from_linear_notification(
        scene.app,
        &fixtures.linear_notification,
        scene.user.id,
        connection.id,
    )
    .await;

    ProviderCase {
        kind: IntegrationProviderKind::Linear,
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
        notifications_off_config: Some(IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: false,
            ..LinearConfig::default()
        })),
    }
}

async fn google_mail_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        google_mail_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let notification = create_notification_from_google_mail_thread(
        scene.app,
        &fixtures.google_mail_thread,
        scene.user.id,
        connection.id,
    )
    .await;

    ProviderCase {
        kind: IntegrationProviderKind::GoogleMail,
        connection,
        notification,
        harmless_config: IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        scope_narrowing_config: Some(IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
            sync_notifications_enabled: true,
            synced_label: GoogleMailLabel {
                id: "Label_2".to_string(),
                name: "Label 2".to_string(),
            },
        })),
        notifications_off_config: Some(IntegrationConnectionConfig::GoogleMail(
            GoogleMailConfig::disabled(),
        )),
    }
}

async fn google_drive_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
        google_drive_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let notification = create_notification_from_google_drive_comment(
        scene.app,
        &fixtures.google_drive_comment,
        scene.user.id,
        connection.id,
    )
    .await;

    ProviderCase {
        kind: IntegrationProviderKind::GoogleDrive,
        connection,
        notification,
        harmless_config: IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
        scope_narrowing_config: None,
        notifications_off_config: Some(IntegrationConnectionConfig::GoogleDrive(
            GoogleDriveConfig::disabled(),
        )),
    }
}

async fn google_calendar_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let google_mail_connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
        google_mail_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
        google_calendar_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let notification = create_notification_from_google_calendar_event(
        scene.app,
        &fixtures.google_mail_thread,
        &fixtures.google_calendar_event,
        scene.user.id,
        google_mail_connection.id,
        connection.id,
    )
    .await;

    ProviderCase {
        kind: IntegrationProviderKind::GoogleCalendar,
        connection,
        notification,
        harmless_config: IntegrationConnectionConfig::GoogleCalendar(
            GoogleCalendarConfig::enabled(),
        ),
        scope_narrowing_config: Some(IntegrationConnectionConfig::GoogleCalendar(
            GoogleCalendarConfig::disabled(),
        )),
        notifications_off_config: None,
    }
}

async fn slack_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        slack_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let notification = create_notification_from_slack_thread(
        scene.app,
        &fixtures.slack_thread,
        scene.user.id,
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
        kind: IntegrationProviderKind::Slack,
        connection,
        notification,
        harmless_config: IntegrationConnectionConfig::Slack(harmless_config),
        scope_narrowing_config: Some(IntegrationConnectionConfig::Slack(scope_narrowing_config)),
        notifications_off_config: Some(IntegrationConnectionConfig::Slack(SlackConfig::disabled())),
    }
}

async fn todoist_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let connection = create_provider_connection(
        scene,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        todoist_oauth_credential(),
        &fixtures.settings,
    )
    .await;
    let notification = create_notification_from_todoist_item(
        scene.app,
        &fixtures.todoist_item,
        scene.user.id,
        connection.id,
    )
    .await;

    ProviderCase {
        kind: IntegrationProviderKind::Todoist,
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
        notifications_off_config: Some(IntegrationConnectionConfig::Todoist(
            TodoistConfig::disabled(),
        )),
    }
}

async fn ticktick_case(scene: &Scene<'_>, fixtures: &ProviderFixtures) -> ProviderCase {
    let connection = create_ticktick_integration_connection(
        scene.app,
        scene.user.id,
        &fixtures.settings,
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        None,
    )
    .await;
    let notification = create_notification_from_ticktick_item(
        scene.app,
        &fixtures.ticktick_item,
        scene.user.id,
        connection.id,
    )
    .await;

    ProviderCase {
        kind: IntegrationProviderKind::TickTick,
        connection,
        notification,
        harmless_config: IntegrationConnectionConfig::TickTick(TickTickConfig {
            default_priority: Some(TaskPriority::P2),
            ..TickTickConfig::enabled()
        }),
        scope_narrowing_config: Some(IntegrationConnectionConfig::TickTick(TickTickConfig {
            create_notification_from_inbox_task: false,
            ..TickTickConfig::enabled()
        })),
        notifications_off_config: Some(IntegrationConnectionConfig::TickTick(
            TickTickConfig::disabled(),
        )),
    }
}
