use std::{env, fmt::Debug, fs, str::FromStr, sync::Arc};

use anyhow::Context;
use chrono::{Timelike, Utc};
use email_address::EmailAddress;
use graphql_client::Response;
use secrecy::SecretBox;
use slack_morphism::{
    api::{
        SlackApiConversationsHistoryResponse, SlackApiConversationsInfoResponse,
        SlackApiTeamInfoResponse, SlackApiUsersInfoResponse,
    },
    SlackReactionName,
};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{
            SlackConfig, SlackMessageConfig, SlackReactionConfig, SlackStarConfig,
            SlackSyncTaskConfig, SlackSyncType,
        },
        provider::IntegrationProviderKind,
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
    },
    notification::{service::NotificationPatch, Notification, NotificationSource},
    task::{service::TaskPatch, Task, TaskSource},
    third_party::{
        integrations::{
            github::GithubNotification,
            google_calendar::GoogleCalendarEvent,
            google_drive::GoogleDriveComment,
            google_mail::GoogleMailThread,
            linear::{LinearIssue, LinearNotification},
            slack::{
                SlackMessageDetails, SlackMessageSenderDetails, SlackReaction, SlackReactionItem,
                SlackReactionState, SlackStar, SlackStarItem, SlackStarState, SlackThread,
            },
            todoist::TodoistItem,
        },
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    },
    user::{Password, User, UserId},
};

use crate::{
    configuration::Settings,
    integrations::{
        google_mail::{GoogleMailUserProfile, RawGoogleMailThread},
        linear::{
            graphql::{assigned_issues_query, notifications_query},
            LinearService,
        },
        notification::ThirdPartyNotificationSourceService,
        slack::SlackService,
        task::ThirdPartyTaskService,
        todoist::TodoistService,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService,
        task::service::TaskService,
        third_party::service::ThirdPartyItemService,
        user::{
            model::{LocalUserAuth, UserAuth},
            service::UserService,
        },
        UniversalInboxError,
    },
};

const DEFAULT_PASSWORD: &str = "test123456";

#[tracing::instrument(name = "generate-testing-user", level = "info", skip_all, err)]
pub async fn generate_testing_user(
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: Settings,
) -> Result<(), UniversalInboxError> {
    let service = user_service.clone();

    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while generating new testing user")?;

    let user = generate_user(&mut transaction, user_service).await?;

    generate_todoist_notifications(
        &mut transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        task_service.clone(),
        third_party_item_service.clone(),
        &settings,
        user.id,
    )
    .await?;

    generate_github_notifications(
        &mut transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
        &settings,
        user.id,
    )
    .await?;

    generate_linear_notifications_and_tasks(
        &mut transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        task_service.clone(),
        third_party_item_service.clone(),
        &settings,
        user.id,
    )
    .await?;

    generate_slack_notifications_and_tasks(
        &mut transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        task_service.clone(),
        third_party_item_service.clone(),
        &settings,
        user.id,
    )
    .await?;

    let google_mail_integration_connection = generate_google_mail_notifications(
        &mut transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
        &settings,
        user.id,
    )
    .await?;

    generate_google_calendar_notifications(
        &mut transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
        &settings,
        user.id,
        &google_mail_integration_connection,
    )
    .await?;

    generate_google_drive_notifications(
        &mut transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
        &settings,
        user.id,
    )
    .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction while generating new testing user")?;

    info!(
        "Test user {} successfully generated with password {DEFAULT_PASSWORD}",
        user.email
            .map(|email| email.to_string())
            .unwrap_or(user.id.to_string())
    );

    Ok(())
}

async fn generate_github_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
) -> Result<IntegrationConnection, UniversalInboxError> {
    info!("Generating Github notifications");
    let integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::Github,
        settings
            .integrations
            .get("github")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        None,
    )
    .await?;

    let github_notification: GithubNotification =
        load_json_fixture_file("github_notification.json")?;
    let github_service = notification_service
        .clone()
        .read()
        .await
        .github_service
        .clone();
    create_notification_from_source_item(
        executor,
        github_notification.id.to_string(),
        ThirdPartyItemData::GithubNotification(Box::new(github_notification.clone())),
        user_id,
        integration_connection.id,
        github_service,
        notification_service,
        third_party_item_service,
    )
    .await?;

    Ok(integration_connection)
}

async fn generate_linear_notifications_and_tasks(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
) -> Result<IntegrationConnection, UniversalInboxError> {
    let integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::Linear,
        settings
            .integrations
            .get("linear")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        None,
    )
    .await?;

    let linear_notifications_response: Response<notifications_query::ResponseData> =
        load_json_fixture_file("sync_linear_notifications.json")?;
    let linear_notifications: Vec<LinearNotification> = linear_notifications_response
        .data
        .unwrap()
        .try_into()
        .unwrap();

    create_linear_notification(
        executor,
        notification_service.clone(),
        third_party_item_service.clone(),
        linear_notifications[1].clone(), // Get a ProjectNotification
        integration_connection.id,
        user_id,
    )
    .await?;

    create_linear_notification(
        executor,
        notification_service.clone(),
        third_party_item_service.clone(),
        linear_notifications[2].clone(), // Get an IssueNotification
        integration_connection.id,
        user_id,
    )
    .await?;

    let linear_service = notification_service
        .clone()
        .read()
        .await
        .linear_service
        .clone();
    let sync_linear_tasks_response: Response<assigned_issues_query::ResponseData> =
        load_json_fixture_file("sync_linear_tasks.json")?;
    let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()?;
    create_task_from_source_item::<LinearIssue, LinearService>(
        executor,
        linear_issues[0].id.to_string(),
        ThirdPartyItemData::LinearIssue(Box::new(linear_issues[0].clone())),
        user_id,
        &integration_connection,
        linear_service,
        task_service,
        third_party_item_service,
    )
    .await?;

    Ok(integration_connection)
}

async fn create_linear_notification(
    executor: &mut Transaction<'_, Postgres>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    linear_notification: LinearNotification,
    integration_connection_id: IntegrationConnectionId,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let linear_notification_id = match &linear_notification {
        LinearNotification::IssueNotification { id, .. } => id.to_string(),
        LinearNotification::ProjectNotification { id, .. } => id.to_string(),
    };
    let linear_service = notification_service
        .clone()
        .read()
        .await
        .linear_service
        .clone();
    create_notification_from_source_item(
        executor,
        linear_notification_id,
        ThirdPartyItemData::LinearNotification(Box::new(linear_notification.clone())),
        user_id,
        integration_connection_id,
        linear_service,
        notification_service,
        third_party_item_service,
    )
    .await?;

    Ok(())
}

async fn generate_slack_notifications_and_tasks(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
) -> Result<IntegrationConnection, UniversalInboxError> {
    let integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::Slack,
        settings
            .integrations
            .get("slack")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        Some(IntegrationConnectionConfig::Slack(SlackConfig {
            star_config: SlackStarConfig {
                sync_enabled: true,
                sync_type: SlackSyncType::AsNotifications,
            },
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsTasks(SlackSyncTaskConfig::default()),
            },
            message_config: SlackMessageConfig {
                sync_enabled: true,
                is_2way_sync: false,
            },
        })),
    )
    .await?;

    let slack_service = notification_service
        .clone()
        .read()
        .await
        .slack_service
        .clone();

    let slack_star = slack_star_added()?;
    create_notification_from_source_item::<SlackStar, SlackService>(
        executor,
        slack_star.item.id(),
        ThirdPartyItemData::SlackStar(slack_star),
        user_id,
        integration_connection.id,
        slack_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
    )
    .await?;

    let slack_thread = slack_thread()?;
    create_notification_from_source_item::<SlackThread, SlackService>(
        executor,
        slack_thread.messages.first().origin.ts.to_string(),
        ThirdPartyItemData::SlackThread(slack_thread),
        user_id,
        integration_connection.id,
        slack_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
    )
    .await?;

    let slack_reaction = slack_reaction_added()?;
    create_task_from_source_item::<SlackReaction, SlackService>(
        executor,
        slack_reaction.item.id(),
        ThirdPartyItemData::SlackReaction(slack_reaction),
        user_id,
        &integration_connection,
        slack_service,
        task_service,
        third_party_item_service,
    )
    .await?;

    Ok(integration_connection)
}

pub fn slack_star_added() -> Result<Box<SlackStar>, UniversalInboxError> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_json_fixture_file("slack_fetch_message_response.json")?;
    let channel_response: SlackApiConversationsInfoResponse =
        load_json_fixture_file("slack_fetch_channel_response.json")?;
    let user_response: SlackApiUsersInfoResponse =
        load_json_fixture_file("slack_fetch_user_response.json")?;
    let sender = SlackMessageSenderDetails::User(Box::new(user_response.user.profile.unwrap()));
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json")?;

    Ok(Box::new(SlackStar {
        state: SlackStarState::StarAdded,
        created_at: Utc::now(),
        item: SlackStarItem::SlackMessage(Box::new(SlackMessageDetails {
            url: "https://example.com".parse().unwrap(),
            message: message_response.messages[0].clone(),
            channel: channel_response.channel,
            sender,
            team: team_response.team,
            references: None,
        })),
    }))
}

pub fn slack_reaction_added() -> Result<Box<SlackReaction>, UniversalInboxError> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_json_fixture_file("slack_fetch_message_response.json")?;
    let channel_response: SlackApiConversationsInfoResponse =
        load_json_fixture_file("slack_fetch_channel_response.json")?;
    let user_response: SlackApiUsersInfoResponse =
        load_json_fixture_file("slack_fetch_user_response.json")?;
    let sender = SlackMessageSenderDetails::User(Box::new(user_response.user.profile.unwrap()));
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json")?;

    Ok(Box::new(SlackReaction {
        name: SlackReactionName("eyes".to_string()),
        state: SlackReactionState::ReactionAdded,
        created_at: Utc::now(),
        item: SlackReactionItem::SlackMessage(SlackMessageDetails {
            url: "https://example.com".parse().unwrap(),
            message: message_response.messages[0].clone(),
            channel: channel_response.channel,
            sender,
            team: team_response.team,
            references: None,
        }),
    }))
}

pub fn slack_thread() -> Result<Box<SlackThread>, UniversalInboxError> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_json_fixture_file("slack_fetch_thread_response.json")?;
    let channel_response: SlackApiConversationsInfoResponse =
        load_json_fixture_file("slack_fetch_channel_response.json")?;
    let team_response: SlackApiTeamInfoResponse =
        load_json_fixture_file("slack_fetch_team_response.json")?;

    Ok(Box::new(SlackThread {
        url: "https://example.com".parse().unwrap(),
        messages: message_response.messages.try_into().unwrap(),
        subscribed: true,
        last_read: None,
        channel: channel_response.channel.clone(),
        team: team_response.team.clone(),
        references: None,
        sender_profiles: Default::default(),
    }))
}

async fn generate_google_mail_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
) -> Result<IntegrationConnection, UniversalInboxError> {
    info!("Generating Google Mail notifications");
    let integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::GoogleMail,
        settings
            .integrations
            .get("google_mail")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        None,
    )
    .await?;

    let google_mail_thread = google_mail_thread()?;
    let google_mail_service = (*notification_service
        .read()
        .await
        .google_mail_service
        .read()
        .await)
        .clone()
        .into();
    create_notification_from_source_item(
        executor,
        google_mail_thread.id.to_string(),
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread.clone())),
        user_id,
        integration_connection.id,
        google_mail_service,
        notification_service,
        third_party_item_service,
    )
    .await?;

    Ok(integration_connection)
}

fn google_mail_thread() -> Result<GoogleMailThread, UniversalInboxError> {
    let raw_google_mail_thread_get_123: RawGoogleMailThread =
        load_json_fixture_file("google_mail_thread_get_123.json")?;
    let google_mail_user_profile: GoogleMailUserProfile =
        load_json_fixture_file("google_mail_user_profile.json")?;
    let user_email_address = EmailAddress::from_str(&google_mail_user_profile.email_address)
        .context("Unable to parse email address from google mail user profile")?;

    Ok(raw_google_mail_thread_get_123.into_google_mail_thread(user_email_address))
}

async fn generate_google_calendar_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    google_mail_integration_connection: &IntegrationConnection,
) -> Result<IntegrationConnection, UniversalInboxError> {
    let google_calendar_integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::GoogleCalendar,
        settings
            .integrations
            .get("google_calendar")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        None,
    )
    .await?;

    let google_mail_thread = google_mail_thread()?;
    let google_calendar_event: GoogleCalendarEvent =
        load_json_fixture_file("google_calendar_event.json")?;

    let google_calendar_service = notification_service
        .read()
        .await
        .google_calendar_service
        .clone();

    let gmail_third_party_item = ThirdPartyItem::new(
        google_mail_thread.id.to_string(),
        ThirdPartyItemData::GoogleMailThread(Box::new(google_mail_thread.clone())),
        user_id,
        google_mail_integration_connection.id,
    );
    let gmail_third_party_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(executor, Box::new(gmail_third_party_item))
        .await
        .unwrap()
        .value();

    let mut gcal_third_party_item = ThirdPartyItem::new(
        google_calendar_event.id.to_string(),
        ThirdPartyItemData::GoogleCalendarEvent(Box::new(google_calendar_event.clone())),
        user_id,
        google_calendar_integration_connection.id,
    );
    gcal_third_party_item.source_item = Some(gmail_third_party_item);
    let gcal_third_party_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(executor, Box::new(gcal_third_party_item))
        .await
        .unwrap()
        .value();

    notification_service
        .read()
        .await
        .create_notification_from_third_party_item(
            executor,
            *gcal_third_party_item,
            google_calendar_service,
            user_id,
        )
        .await?
        .unwrap();

    Ok(google_calendar_integration_connection)
}

async fn generate_todoist_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
) -> Result<IntegrationConnection, UniversalInboxError> {
    let integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::Todoist,
        settings
            .integrations
            .get("todoist")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        None,
    )
    .await?;

    let todoist_item: TodoistItem = load_json_fixture_file("todoist_item.json")?;
    let todoist_service = task_service.read().await.todoist_service.clone();
    let notification = create_notification_from_source_item(
        executor,
        todoist_item.id.to_string(),
        ThirdPartyItemData::TodoistItem(Box::new(todoist_item.clone())),
        user_id,
        integration_connection.id,
        todoist_service,
        notification_service.clone(),
        third_party_item_service,
    )
    .await?;

    let third_party_item = notification.source_item;
    let task_request = TodoistService::build_task_with_project_name(
        &todoist_item,
        "INBOX".to_string(),
        &third_party_item,
        user_id,
    )
    .await;
    let upsert_status = task_service
        .read()
        .await
        .create_or_update_task(executor, task_request)
        .await?;

    let task = upsert_status.value();
    notification_service
        .read()
        .await
        .patch_notification(
            executor,
            notification.id,
            &NotificationPatch {
                task_id: Some(task.id),
                ..Default::default()
            },
            false,
            false,
            user_id,
        )
        .await?;

    Ok(integration_connection)
}

async fn generate_google_drive_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
) -> Result<IntegrationConnection, UniversalInboxError> {
    info!("Generating Google Drive comment notifications");
    let integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::GoogleDrive,
        settings
            .integrations
            .get("google_drive")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        None,
    )
    .await?;

    let google_drive_comment: GoogleDriveComment =
        load_json_fixture_file("google_drive/google_drive_comment_123.json")?;
    let google_drive_service = (*notification_service
        .read()
        .await
        .google_drive_service
        .read()
        .await)
        .clone()
        .into();
    create_notification_from_source_item(
        executor,
        google_drive_comment.id.to_string(),
        ThirdPartyItemData::GoogleDriveComment(Box::new(google_drive_comment.clone())),
        user_id,
        integration_connection.id,
        google_drive_service,
        notification_service,
        third_party_item_service,
    )
    .await?;

    Ok(integration_connection)
}

async fn create_integration_connection(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    integration_provider_kind: IntegrationProviderKind,
    registered_oauth_scopes: Vec<String>,
    user_id: UserId,
    integration_connection_config: Option<IntegrationConnectionConfig>,
) -> Result<IntegrationConnection, UniversalInboxError> {
    let integration_connection = integration_connection_service
        .read()
        .await
        .create_integration_connection(
            executor,
            integration_provider_kind,
            IntegrationConnectionStatus::Created,
            user_id,
        )
        .await?;

    if let Some(integration_connection_config) = integration_connection_config {
        integration_connection_service
            .read()
            .await
            .update_integration_connection_config(
                executor,
                integration_connection.id,
                integration_connection_config,
                user_id,
            )
            .await?;
    }

    integration_connection_service
        .read()
        .await
        .update_integration_connection_status(
            executor,
            integration_connection.id,
            user_id,
            IntegrationConnectionStatus::Validated,
            registered_oauth_scopes,
        )
        .await
        .map(|update_status| *(update_status.result.unwrap()))
}

async fn generate_user(
    executor: &mut Transaction<'_, Postgres>,
    user_service: Arc<UserService>,
) -> Result<User, UniversalInboxError> {
    let id = Uuid::new_v4();
    let user = User {
        id: id.into(),
        first_name: None,
        last_name: None,
        email: Some(format!("test+{}@test.com", id).parse().unwrap()),
        email_validated_at: Some(Utc::now().with_nanosecond(0).unwrap()),
        email_validation_sent_at: Some(Utc::now().with_nanosecond(0).unwrap()),
        is_testing: true,
        created_at: Utc::now().with_nanosecond(0).unwrap(),
        updated_at: Utc::now().with_nanosecond(0).unwrap(),
    };
    let user_auth = UserAuth::Local(Box::new(LocalUserAuth {
        password_hash: user_service.get_new_password_hash(SecretBox::new(Box::new(Password(
            DEFAULT_PASSWORD.to_string(),
        ))))?,
        password_reset_at: None,
        password_reset_sent_at: None,
    }));

    user_service.register_user(executor, user, user_auth).await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_notification_from_source_item<T, U>(
    executor: &mut Transaction<'_, Postgres>,
    source_item_id: String,
    third_party_item_data: ThirdPartyItemData,
    user_id: UserId,
    integration_connection_id: IntegrationConnectionId,
    third_party_notification_service: Arc<U>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
) -> Result<Box<Notification>, UniversalInboxError>
where
    T: TryFrom<ThirdPartyItem> + Debug,
    U: ThirdPartyNotificationSourceService<T> + NotificationSource + Send + Sync,
    <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
{
    let third_party_item = Box::new(ThirdPartyItem::new(
        source_item_id,
        third_party_item_data,
        user_id,
        integration_connection_id,
    ));
    let third_party_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(executor, third_party_item)
        .await?
        .value();

    let notification = notification_service
        .read()
        .await
        .create_notification_from_third_party_item(
            executor,
            *third_party_item,
            third_party_notification_service,
            user_id,
        )
        .await?
        .unwrap();

    Ok(Box::new(notification))
}

#[allow(clippy::too_many_arguments)]
pub async fn create_task_from_source_item<T, U>(
    executor: &mut Transaction<'_, Postgres>,
    source_item_id: String,
    third_party_item_data: ThirdPartyItemData,
    user_id: UserId,
    integration_connection: &IntegrationConnection,
    third_party_task_service: Arc<U>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
) -> Result<Box<Task>, UniversalInboxError>
where
    T: TryFrom<ThirdPartyItem> + Debug,
    U: ThirdPartyTaskService<T> + NotificationSource + TaskSource + Send + Sync,
    <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
{
    let third_party_item = Box::new(ThirdPartyItem::new(
        source_item_id,
        third_party_item_data,
        user_id,
        integration_connection.id,
    ));
    let third_party_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(executor, third_party_item)
        .await?
        .value();

    let task_creation_config = integration_connection
        .provider
        .get_task_creation_default_values(&third_party_item);

    let upsert_task = task_service
        .read()
        .await
        .save_third_party_item_as_task(
            executor,
            third_party_task_service,
            &third_party_item,
            task_creation_config,
            user_id,
        )
        .await?;

    let mut task = upsert_task.value();
    let todoist_item: TodoistItem = load_json_fixture_file("todoist_item.json")?;

    let sink_third_party_item =
        todoist_item.into_third_party_item(user_id, integration_connection.id);
    let upsert_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(executor, Box::new(sink_third_party_item))
        .await?;
    let uptodate_sink_party_item = upsert_item.value();

    task.sink_item = Some(*uptodate_sink_party_item.clone());
    task_service
        .read()
        .await
        .patch_task(
            executor,
            task.id,
            &TaskPatch {
                sink_item_id: Some(uptodate_sink_party_item.id),
                ..Default::default()
            },
            user_id,
        )
        .await?;

    Ok(task)
}

pub fn fixture_path(fixture_file_name: &str) -> Result<String, UniversalInboxError> {
    Ok(format!(
        "{}/tests/api/fixtures/{fixture_file_name}",
        env::var("CARGO_MANIFEST_DIR")
            .context("Missing `CARGO_MANIFEST_DIR` environement variable")?
    ))
}

pub fn load_json_fixture_file<T: for<'de> serde::de::Deserialize<'de>>(
    fixture_file_name: &str,
) -> Result<T, UniversalInboxError> {
    let fixture_file_path = fixture_path(fixture_file_name)?;
    let input_str = fs::read_to_string(&fixture_file_path)
        .context(format!("Unable to load fixture file {fixture_file_path}"))?;
    Ok(serde_json::from_str::<T>(&input_str).context(format!(
        "Failed to deserialize JSON from file {fixture_file_path}"
    ))?)
}
