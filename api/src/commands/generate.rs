use std::{collections::HashMap, env, fmt::Debug, fs, str::FromStr, sync::Arc, sync::OnceLock};

use anyhow::{Context, anyhow};
use chrono::{DateTime, Duration, Timelike, Utc};
use email_address::EmailAddress;
use graphql_client::Response;
use regex::{Captures, Regex};
use secrecy::SecretBox;
use slack_blocks_render::SlackReferences;
use slack_morphism::{
    SlackChannelId, SlackEmojiName, SlackEmojiRef, SlackReactionName, SlackUserGroupId,
    SlackUserId, SlackUserProfile,
    api::{
        SlackApiConversationsHistoryResponse, SlackApiConversationsInfoResponse,
        SlackApiTeamInfoResponse, SlackApiUsersInfoResponse,
    },
};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::slack::{
            SlackConfig, SlackMessageConfig, SlackReactionConfig, SlackSyncTaskConfig,
            SlackSyncType,
        },
        provider::IntegrationProviderKind,
    },
    notification::{Notification, NotificationSource, service::NotificationPatch},
    task::{Task, TaskSource, service::TaskPatch},
    third_party::{
        integrations::{
            github::{
                GithubDiscussion, GithubNotification, GithubNotificationItem,
                GithubNotificationSubject, GithubPullRequest,
            },
            google_calendar::GoogleCalendarEvent,
            google_drive::GoogleDriveComment,
            google_mail::GoogleMailThread,
            linear::{LinearIssue, LinearNotification},
            slack::{
                SlackMessageDetails, SlackMessageSenderDetails, SlackReaction, SlackReactionItem,
                SlackReactionState, SlackThread,
            },
            ticktick::TickTickItem,
            todoist::TodoistItem,
        },
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    },
    user::{Password, User, UserId},
};

use crate::{
    configuration::Settings,
    integrations::{
        github::graphql::{discussion_query, pull_request_query},
        google_mail::{GoogleMailUserProfile, RawGoogleMailThread},
        linear::{
            LinearService,
            graphql::{assigned_issues_query, notifications_query},
        },
        notification::ThirdPartyNotificationSourceService,
        slack::SlackService,
        task::ThirdPartyTaskService,
        ticktick::TickTickService,
        todoist::TodoistService,
    },
    universal_inbox::{
        UniversalInboxError,
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService,
        task::service::TaskService,
        third_party::service::ThirdPartyItemService,
        user::{
            model::{LocalUserAuth, UserAuth},
            service::UserService,
        },
    },
};

pub const DEFAULT_PASSWORD: &str = "test123456";
const SEED_FIXTURES_SUBDIR: &str = "fixtures/seed";

fn seed_fixture_path(fixture_file_name: &str) -> Result<String, UniversalInboxError> {
    Ok(format!(
        "{}/{SEED_FIXTURES_SUBDIR}/{fixture_file_name}",
        env::var("CARGO_MANIFEST_DIR")
            .context("Missing `CARGO_MANIFEST_DIR` environment variable")?
    ))
}

fn seed_token_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{([a-z0-9_]+)\}\}").expect("seed token regex"))
}

fn format_rfc3339(d: DateTime<Utc>) -> String {
    d.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn format_slack_ts(d: DateTime<Utc>) -> String {
    format!("{}.000000", d.timestamp())
}

fn format_rfc2822(d: DateTime<Utc>) -> String {
    d.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn format_date(d: DateTime<Utc>) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn parse_duration_token(s: &str) -> Option<Duration> {
    if s.len() < 2 {
        return None;
    }
    let (num, unit) = s.split_at(s.len() - 1);
    let n: i64 = num.parse().ok()?;
    Some(match unit {
        "m" => Duration::minutes(n),
        "h" => Duration::hours(n),
        "d" => Duration::days(n),
        "w" => Duration::weeks(n),
        _ => return None,
    })
}

fn parse_at_offset(rest: &str, base: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let mut parts = rest.split('_');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute: u32 = match parts.next() {
        Some(m) => m.parse().ok()?,
        None => 0,
    };
    base.with_hour(hour)?
        .with_minute(minute)?
        .with_second(0)?
        .with_nanosecond(0)
}

fn resolve_seed_token(token: &str, user_email: &str) -> Option<String> {
    let now = Utc::now().with_nanosecond(0).unwrap();

    match token {
        "user_email" => return Some(user_email.to_string()),
        "now" => return Some(format_rfc3339(now)),
        "ts_now" => return Some(format_slack_ts(now)),
        "epoch_ms_now" => return Some(now.timestamp_millis().to_string()),
        "date_today" => return Some(format_date(now)),
        "date_tomorrow" => return Some(format_date(now + Duration::days(1))),
        _ => {}
    }

    if let Some(rest) = token.strip_prefix("today_at_") {
        return parse_at_offset(rest, now).map(format_rfc3339);
    }
    if let Some(rest) = token.strip_prefix("tomorrow_at_") {
        return parse_at_offset(rest, now + Duration::days(1)).map(format_rfc3339);
    }

    let (formatter, after_prefix): (fn(DateTime<Utc>) -> String, &str) =
        if let Some(rest) = token.strip_prefix("ts_") {
            (format_slack_ts, rest)
        } else if let Some(rest) = token.strip_prefix("rfc2822_") {
            (format_rfc2822, rest)
        } else if let Some(rest) = token.strip_prefix("epoch_ms_") {
            (|d| d.timestamp_millis().to_string(), rest)
        } else if let Some(rest) = token.strip_prefix("date_") {
            (format_date, rest)
        } else {
            (format_rfc3339, token)
        };

    if let Some(rest) = after_prefix.strip_prefix("ago_") {
        let dur = parse_duration_token(rest)?;
        return Some(formatter(now - dur));
    }
    if let Some(rest) = after_prefix.strip_prefix("in_") {
        let dur = parse_duration_token(rest)?;
        return Some(formatter(now + dur));
    }

    None
}

fn apply_seed_template(input: &str, user_email: &str) -> String {
    seed_token_regex()
        .replace_all(input, |caps: &Captures| {
            let token = &caps[1];
            resolve_seed_token(token, user_email).unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned()
}

pub fn load_seed_fixture<T: for<'de> serde::de::Deserialize<'de>>(
    fixture_file_name: &str,
    user_email: &str,
) -> Result<T, UniversalInboxError> {
    let path = seed_fixture_path(fixture_file_name)?;
    let raw = fs::read_to_string(&path).context(format!("Unable to load seed fixture {path}"))?;
    let resolved = apply_seed_template(&raw, user_email);
    Ok(serde_json::from_str::<T>(&resolved)
        .context(format!("Failed to deserialize seed fixture {path}"))?)
}

#[tracing::instrument(name = "generate-testing-user", level = "info", skip_all, err)]
pub async fn generate_testing_user(
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: Settings,
) -> Result<String, UniversalInboxError> {
    let service = user_service.clone();

    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while generating new testing user")?;

    let user = generate_user(&mut transaction, user_service).await?;
    let email = user
        .email
        .as_ref()
        .map(|email| email.to_string())
        .unwrap_or_else(|| user.id.to_string());

    generate_all_notifications(
        &mut transaction,
        integration_connection_service,
        notification_service,
        task_service,
        third_party_item_service,
        &settings,
        user.id,
        &email,
    )
    .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction while generating new testing user")?;

    info!(
        "Test user {} successfully generated with password {DEFAULT_PASSWORD}",
        email
    );

    Ok(email)
}

#[tracing::instrument(name = "generate-empty-user", level = "info", skip_all, err)]
pub async fn generate_empty_user(
    user_service: Arc<UserService>,
) -> Result<String, UniversalInboxError> {
    let service = user_service.clone();

    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while generating new empty user")?;

    let user = generate_user(&mut transaction, user_service).await?;
    let email = user
        .email
        .as_ref()
        .map(|email| email.to_string())
        .unwrap_or_else(|| user.id.to_string());

    transaction
        .commit()
        .await
        .context("Failed to commit transaction while generating new empty user")?;

    info!(
        "Empty user {email} (id: {}) successfully generated with password {DEFAULT_PASSWORD}",
        user.id
    );

    Ok(email)
}

#[tracing::instrument(
    name = "connect-integration-for-user",
    level = "info",
    skip_all,
    fields(user.id = %user_id, provider = %provider_kind),
    err
)]
pub async fn connect_integration_for_user(
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    settings: Settings,
    user_id: UserId,
    provider_kind: IntegrationProviderKind,
) -> Result<(), UniversalInboxError> {
    let mut transaction = user_service
        .begin()
        .await
        .context("Failed to create new transaction while connecting integration for user")?;

    let scopes = settings
        .required_oauth_scopes()
        .get(&provider_kind)
        .cloned()
        .unwrap_or_default();

    create_integration_connection(
        &mut transaction,
        integration_connection_service,
        provider_kind,
        scopes,
        user_id,
        None,
    )
    .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction while connecting integration for user")?;

    info!(
        "Integration {provider_kind} successfully connected for user {user_id} in Validated state"
    );

    Ok(())
}

#[tracing::instrument(name = "generate-notifications-for-user", level = "info", skip_all, fields(user.id = %user_id), err)]
pub async fn generate_notifications_for_user(
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: Settings,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let mut transaction = user_service.begin().await.context(
        "Failed to create new transaction while generating notifications for existing user",
    )?;

    let user = user_service
        .get_user(&mut transaction, user_id)
        .await?
        .ok_or_else(|| UniversalInboxError::Unexpected(anyhow!("User {user_id} not found")))?;
    let email = user
        .email
        .as_ref()
        .map(|email| email.to_string())
        .unwrap_or_else(|| user.id.to_string());

    generate_all_notifications(
        &mut transaction,
        integration_connection_service,
        notification_service,
        task_service,
        third_party_item_service,
        &settings,
        user.id,
        &email,
    )
    .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction while generating notifications for existing user")?;

    info!(
        "Sample notifications successfully generated for user {} ({})",
        user.id,
        user.email
            .as_ref()
            .map(|e| e.to_string())
            .unwrap_or_default()
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn generate_all_notifications(
    transaction: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
) -> Result<(), UniversalInboxError> {
    generate_todoist_notifications(
        transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        task_service.clone(),
        third_party_item_service.clone(),
        settings,
        user_id,
        user_email,
    )
    .await?;

    generate_ticktick_notifications(
        transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        task_service.clone(),
        third_party_item_service.clone(),
        settings,
        user_id,
        user_email,
    )
    .await?;

    generate_github_notifications(
        transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
        settings,
        user_id,
        user_email,
    )
    .await?;

    generate_linear_notifications_and_tasks(
        transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        task_service.clone(),
        third_party_item_service.clone(),
        settings,
        user_id,
        user_email,
    )
    .await?;

    generate_slack_notifications_and_tasks(
        transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        task_service.clone(),
        third_party_item_service.clone(),
        settings,
        user_id,
        user_email,
    )
    .await?;

    let google_mail_integration_connection = generate_google_mail_notifications(
        transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
        settings,
        user_id,
        user_email,
    )
    .await?;

    generate_google_calendar_notifications(
        transaction,
        integration_connection_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
        settings,
        user_id,
        user_email,
        &google_mail_integration_connection,
    )
    .await?;

    generate_google_drive_notifications(
        transaction,
        integration_connection_service,
        notification_service,
        third_party_item_service,
        settings,
        user_id,
        user_email,
    )
    .await?;

    Ok(())
}

async fn generate_github_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
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
        load_seed_fixture("github_notification.json", user_email)?;
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
        github_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
    )
    .await?;

    let pr_response: Response<pull_request_query::ResponseData> =
        load_seed_fixture("github_pull_request_123_response.json", user_email)?;
    let github_pull_request: GithubPullRequest = pr_response
        .data
        .ok_or_else(|| anyhow!("Missing data in Github pull request fixture"))?
        .try_into()?;
    let github_pr_notification = GithubNotification {
        id: "2".to_string(),
        subject: GithubNotificationSubject {
            title: github_pull_request.title.clone(),
            url: Some(github_pull_request.url.clone()),
            latest_comment_url: None,
            r#type: "PullRequest".to_string(),
        },
        item: Some(GithubNotificationItem::GithubPullRequest(
            github_pull_request,
        )),
        ..github_notification.clone()
    };
    create_notification_from_source_item(
        executor,
        github_pr_notification.id.to_string(),
        ThirdPartyItemData::GithubNotification(Box::new(github_pr_notification)),
        user_id,
        integration_connection.id,
        github_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
    )
    .await?;

    let pr_review_response: Response<pull_request_query::ResponseData> =
        load_seed_fixture("github_pull_request_review_response.json", user_email)?;
    let github_pull_request_review: GithubPullRequest = pr_review_response
        .data
        .ok_or_else(|| anyhow!("Missing data in Github review pull request fixture"))?
        .try_into()?;
    let github_pr_review_notification = GithubNotification {
        id: "4".to_string(),
        subject: GithubNotificationSubject {
            title: github_pull_request_review.title.clone(),
            url: Some(github_pull_request_review.url.clone()),
            latest_comment_url: None,
            r#type: "PullRequest".to_string(),
        },
        reason: "review_requested".to_string(),
        item: Some(GithubNotificationItem::GithubPullRequest(
            github_pull_request_review,
        )),
        ..github_notification.clone()
    };
    create_notification_from_source_item(
        executor,
        github_pr_review_notification.id.to_string(),
        ThirdPartyItemData::GithubNotification(Box::new(github_pr_review_notification)),
        user_id,
        integration_connection.id,
        github_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
    )
    .await?;

    let discussion_response: Response<discussion_query::ResponseData> =
        load_seed_fixture("github_discussion_123_response.json", user_email)?;
    let github_discussion: GithubDiscussion = discussion_response
        .data
        .ok_or_else(|| anyhow!("Missing data in Github discussion fixture"))?
        .try_into()?;
    let github_discussion_notification = GithubNotification {
        id: "3".to_string(),
        subject: GithubNotificationSubject {
            title: github_discussion.title.clone(),
            url: Some(github_discussion.url.clone()),
            latest_comment_url: None,
            r#type: "Discussion".to_string(),
        },
        item: Some(GithubNotificationItem::GithubDiscussion(github_discussion)),
        ..github_notification.clone()
    };
    create_notification_from_source_item(
        executor,
        github_discussion_notification.id.to_string(),
        ThirdPartyItemData::GithubNotification(Box::new(github_discussion_notification)),
        user_id,
        integration_connection.id,
        github_service,
        notification_service,
        third_party_item_service,
    )
    .await?;

    Ok(integration_connection)
}

#[allow(clippy::too_many_arguments)]
async fn generate_linear_notifications_and_tasks(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
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
        load_seed_fixture("sync_linear_notifications.json", user_email)?;
    let linear_notifications: Vec<LinearNotification> = linear_notifications_response
        .data
        .unwrap()
        .try_into()
        .unwrap();

    create_linear_notification(
        executor,
        notification_service.clone(),
        third_party_item_service.clone(),
        linear_notifications[1].clone(), // ProjectNotification (lead)
        integration_connection.id,
        user_id,
    )
    .await?;

    create_linear_notification(
        executor,
        notification_service.clone(),
        third_party_item_service.clone(),
        linear_notifications[2].clone(), // IssueNotification — keyboard shortcuts
        integration_connection.id,
        user_id,
    )
    .await?;

    create_linear_notification(
        executor,
        notification_service.clone(),
        third_party_item_service.clone(),
        linear_notifications[3].clone(), // IssueNotification — sync stall
        integration_connection.id,
        user_id,
    )
    .await?;

    create_linear_notification(
        executor,
        notification_service.clone(),
        third_party_item_service.clone(),
        linear_notifications[4].clone(), // IssueNotification — on-call playbook
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
        load_seed_fixture("sync_linear_tasks.json", user_email)?;
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
        user_email,
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

#[allow(clippy::too_many_arguments)]
async fn generate_slack_notifications_and_tasks(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
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
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName("eyes".to_string()),
                sync_type: SlackSyncType::AsTasks(SlackSyncTaskConfig::default()),
                completion_reaction_name: None,
            },
            message_config: SlackMessageConfig {
                sync_enabled: true,
                is_2way_sync: false,
                extension_enabled: true,
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

    let slack_thread = slack_thread(user_email)?;
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

    let slack_design_thread = slack_design_thread(user_email)?;
    create_notification_from_source_item::<SlackThread, SlackService>(
        executor,
        slack_design_thread.messages.first().origin.ts.to_string(),
        ThirdPartyItemData::SlackThread(slack_design_thread),
        user_id,
        integration_connection.id,
        slack_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
    )
    .await?;

    let slack_mention_thread = slack_message_mention(user_email)?;
    create_notification_from_source_item::<SlackThread, SlackService>(
        executor,
        slack_mention_thread.messages.first().origin.ts.to_string(),
        ThirdPartyItemData::SlackThread(slack_mention_thread),
        user_id,
        integration_connection.id,
        slack_service.clone(),
        notification_service.clone(),
        third_party_item_service.clone(),
    )
    .await?;

    let slack_reaction = slack_reaction_added(user_email)?;
    create_task_from_source_item::<SlackReaction, SlackService>(
        executor,
        slack_reaction.item.id(),
        ThirdPartyItemData::SlackReaction(slack_reaction),
        user_id,
        &integration_connection,
        slack_service,
        task_service,
        third_party_item_service,
        user_email,
    )
    .await?;

    Ok(integration_connection)
}

const SEED_SLACK_USER_ID: &str = "U05XYZ";

const SEED_SLACK_USERS: &[(&str, &str, &str)] = &[
    ("U01", "Alice Chen", "alice"),
    ("U02", "Bob Martinez", "bob"),
    ("U03", "Sam Kim", "sam"),
    ("U04", "Maya Rivera", "maya"),
    (SEED_SLACK_USER_ID, "Alex Morgan", "alex"),
    ("U05YYY", "John Doe", "john.doe"),
];

fn slack_seed_sender_profiles() -> HashMap<String, SlackMessageSenderDetails> {
    SEED_SLACK_USERS
        .iter()
        .map(|(id, real_name, display_name)| {
            let profile = SlackUserProfile {
                id: Some(SlackUserId(id.to_string())),
                display_name: Some(display_name.to_string()),
                real_name: Some(real_name.to_string()),
                real_name_normalized: Some(real_name.to_string()),
                avatar_hash: None,
                status_text: None,
                status_expiration: None,
                status_emoji: None,
                huddle_state: None,
                huddle_state_expiration_ts: None,
                display_name_normalized: Some(display_name.to_string()),
                email: None,
                icon: None,
                team: None,
                start_date: None,
                first_name: real_name.split_whitespace().next().map(|s| s.to_string()),
                last_name: real_name.split_whitespace().nth(1).map(|s| s.to_string()),
                phone: None,
                pronouns: None,
                title: None,
                fields: None,
            };
            (
                id.to_string(),
                SlackMessageSenderDetails::User(Box::new(profile)),
            )
        })
        .collect()
}

fn slack_seed_references() -> SlackReferences {
    SlackReferences {
        users: HashMap::from([
            (
                SlackUserId("U01".to_string()),
                Some("Alice Chen".to_string()),
            ),
            (
                SlackUserId("U02".to_string()),
                Some("Bob Martinez".to_string()),
            ),
            (SlackUserId("U03".to_string()), Some("Sam Kim".to_string())),
            (
                SlackUserId("U04".to_string()),
                Some("Maya Rivera".to_string()),
            ),
            (
                SlackUserId(SEED_SLACK_USER_ID.to_string()),
                Some("Alex Morgan".to_string()),
            ),
            (
                SlackUserId("U05YYY".to_string()),
                Some("John Doe".to_string()),
            ),
        ]),
        channels: HashMap::from([
            (
                SlackChannelId("C05XXX".to_string()),
                Some("universal-inbox".to_string()),
            ),
            (
                SlackChannelId("C06DESIGN".to_string()),
                Some("design-reviews".to_string()),
            ),
        ]),
        usergroups: HashMap::from([(
            SlackUserGroupId("S05ZZZ".to_string()),
            Some("v05-team".to_string()),
        )]),
        emojis: HashMap::from([
            (
                SlackEmojiName("unknown1".to_string()),
                Some(SlackEmojiRef::Alias(SlackEmojiName("rocket".to_string()))),
            ),
            (
                SlackEmojiName("unknown2".to_string()),
                Some(SlackEmojiRef::Alias(SlackEmojiName("sparkles".to_string()))),
            ),
        ]),
        user_id_to_highlight: Some(SlackUserId(SEED_SLACK_USER_ID.to_string())),
        usergroup_ids_to_highlight: Some(vec![SlackUserGroupId("S05ZZZ".to_string())]),
    }
}

pub fn slack_reaction_added(user_email: &str) -> Result<Box<SlackReaction>, UniversalInboxError> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_seed_fixture("slack_fetch_message_response.json", user_email)?;
    let channel_response: SlackApiConversationsInfoResponse =
        load_seed_fixture("slack_fetch_channel_response.json", user_email)?;
    let user_response: SlackApiUsersInfoResponse =
        load_seed_fixture("slack_fetch_user_response.json", user_email)?;
    let sender = SlackMessageSenderDetails::User(Box::new(user_response.user.profile.unwrap()));
    let team_response: SlackApiTeamInfoResponse =
        load_seed_fixture("slack_fetch_team_response.json", user_email)?;

    Ok(Box::new(SlackReaction {
        name: SlackReactionName("eyes".to_string()),
        state: SlackReactionState::ReactionAdded,
        created_at: Utc::now(),
        item: SlackReactionItem::SlackMessage(SlackMessageDetails {
            url: "https://universal-inbox.slack.com/archives/C05XXX/p1707686216825719"
                .parse()
                .unwrap(),
            message: message_response.messages[0].clone(),
            channel: channel_response.channel,
            sender,
            team: team_response.team,
            references: Some(slack_seed_references()),
        }),
    }))
}

pub fn slack_thread(user_email: &str) -> Result<Box<SlackThread>, UniversalInboxError> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_seed_fixture("slack_fetch_thread_verbose_response.json", user_email)?;
    let channel_response: SlackApiConversationsInfoResponse =
        load_seed_fixture("slack_fetch_channel_response.json", user_email)?;
    let team_response: SlackApiTeamInfoResponse =
        load_seed_fixture("slack_fetch_team_response.json", user_email)?;

    Ok(Box::new(SlackThread {
        url: "https://universal-inbox.slack.com/archives/C05XXX/p1732535291911209"
            .parse()
            .unwrap(),
        messages: message_response.messages.try_into().unwrap(),
        subscribed: true,
        last_read: None,
        channel: channel_response.channel.clone(),
        team: team_response.team.clone(),
        references: Some(slack_seed_references()),
        sender_profiles: slack_seed_sender_profiles(),
        user_slack_id: Some(SEED_SLACK_USER_ID.to_string()),
    }))
}

pub fn slack_design_thread(user_email: &str) -> Result<Box<SlackThread>, UniversalInboxError> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_seed_fixture("slack_fetch_thread_design_response.json", user_email)?;
    let channel_response: SlackApiConversationsInfoResponse =
        load_seed_fixture("slack_fetch_channel_design_response.json", user_email)?;
    let team_response: SlackApiTeamInfoResponse =
        load_seed_fixture("slack_fetch_team_response.json", user_email)?;

    Ok(Box::new(SlackThread {
        url: "https://universal-inbox.slack.com/archives/C06DESIGN/p1707690000000000"
            .parse()
            .unwrap(),
        messages: message_response.messages.try_into().unwrap(),
        subscribed: true,
        last_read: None,
        channel: channel_response.channel.clone(),
        team: team_response.team.clone(),
        references: Some(slack_seed_references()),
        sender_profiles: slack_seed_sender_profiles(),
        user_slack_id: Some(SEED_SLACK_USER_ID.to_string()),
    }))
}

pub fn slack_message_mention(user_email: &str) -> Result<Box<SlackThread>, UniversalInboxError> {
    let message_response: SlackApiConversationsHistoryResponse =
        load_seed_fixture("slack_fetch_mention_response.json", user_email)?;
    let channel_response: SlackApiConversationsInfoResponse =
        load_seed_fixture("slack_fetch_channel_response.json", user_email)?;
    let team_response: SlackApiTeamInfoResponse =
        load_seed_fixture("slack_fetch_team_response.json", user_email)?;

    Ok(Box::new(SlackThread {
        url: "https://universal-inbox.slack.com/archives/C05XXX/p1732600000000001"
            .parse()
            .unwrap(),
        messages: message_response.messages.try_into().unwrap(),
        subscribed: true,
        last_read: None,
        channel: channel_response.channel.clone(),
        team: team_response.team.clone(),
        references: Some(slack_seed_references()),
        sender_profiles: slack_seed_sender_profiles(),
        user_slack_id: Some(SEED_SLACK_USER_ID.to_string()),
    }))
}

async fn generate_google_mail_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
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

    let google_mail_thread = google_mail_thread(user_email)?;
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

fn google_mail_thread(user_email: &str) -> Result<GoogleMailThread, UniversalInboxError> {
    let raw_google_mail_thread: RawGoogleMailThread =
        load_seed_fixture("generate_google_mail_thread.json", user_email)?;
    let google_mail_user_profile: GoogleMailUserProfile =
        load_seed_fixture("google_mail_user_profile.json", user_email)?;
    let user_email_address = EmailAddress::from_str(&google_mail_user_profile.email_address)
        .context("Unable to parse email address from google mail user profile")?;

    Ok(raw_google_mail_thread.into_google_mail_thread(user_email_address))
}

#[allow(clippy::too_many_arguments)]
async fn generate_google_calendar_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
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

    let google_mail_thread = google_mail_thread(user_email)?;
    let google_calendar_event: GoogleCalendarEvent =
        load_seed_fixture("google_calendar_event.json", user_email)?;
    let google_calendar_design_event: GoogleCalendarEvent =
        load_seed_fixture("google_calendar_event_design_review.json", user_email)?;

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
            google_calendar_service.clone(),
            user_id,
        )
        .await?
        .unwrap();

    let gcal_design_third_party_item = ThirdPartyItem::new(
        google_calendar_design_event.id.to_string(),
        ThirdPartyItemData::GoogleCalendarEvent(Box::new(google_calendar_design_event.clone())),
        user_id,
        google_calendar_integration_connection.id,
    );
    let gcal_design_third_party_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(executor, Box::new(gcal_design_third_party_item))
        .await
        .unwrap()
        .value();

    notification_service
        .read()
        .await
        .create_notification_from_third_party_item(
            executor,
            *gcal_design_third_party_item,
            google_calendar_service,
            user_id,
        )
        .await?
        .unwrap();

    Ok(google_calendar_integration_connection)
}

#[allow(clippy::too_many_arguments)]
async fn generate_todoist_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
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

    let todoist_service = task_service.read().await.todoist_service.clone();

    for fixture_name in ["todoist_item.json", "todoist_item_review.json"] {
        let todoist_item: TodoistItem = load_seed_fixture(fixture_name, user_email)?;
        let notification = create_notification_from_source_item(
            executor,
            todoist_item.id.to_string(),
            ThirdPartyItemData::TodoistItem(Box::new(todoist_item.clone())),
            user_id,
            integration_connection.id,
            todoist_service.clone(),
            notification_service.clone(),
            third_party_item_service.clone(),
        )
        .await?;

        let third_party_item = notification.source_item;
        let task_request = TodoistService::build_task_with_project_name(
            &todoist_item,
            "Inbox".to_string(),
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
    }

    Ok(integration_connection)
}

#[allow(clippy::too_many_arguments)]
async fn generate_ticktick_notifications(
    executor: &mut Transaction<'_, Postgres>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: &Settings,
    user_id: UserId,
    user_email: &str,
) -> Result<IntegrationConnection, UniversalInboxError> {
    info!("Generating TickTick notifications");
    let integration_connection = create_integration_connection(
        executor,
        integration_connection_service,
        IntegrationProviderKind::TickTick,
        settings
            .integrations
            .get("ticktick")
            .unwrap()
            .required_oauth_scopes
            .clone(),
        user_id,
        None,
    )
    .await?;

    let ticktick_item: TickTickItem = load_seed_fixture("ticktick_item.json", user_email)?;
    let ticktick_service = task_service.read().await.ticktick_service.clone();
    let notification = create_notification_from_source_item(
        executor,
        ticktick_item.id.to_string(),
        ThirdPartyItemData::TickTickItem(Box::new(ticktick_item.clone())),
        user_id,
        integration_connection.id,
        ticktick_service,
        notification_service.clone(),
        third_party_item_service,
    )
    .await?;

    let third_party_item = notification.source_item;
    let task_request = TickTickService::build_task_with_project_name(
        &ticktick_item,
        "Inbox".to_string(),
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
    user_email: &str,
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
        load_seed_fixture("google_drive/google_drive_comment_123.json", user_email)?;
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
        .get_or_create_integration_connection(executor, integration_provider_kind, user_id)
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
    let short_id: String = id.to_string().chars().take(8).collect();
    let user = User {
        id: id.into(),
        first_name: Some("Alex".to_string()),
        last_name: Some("Morgan".to_string()),
        email: Some(
            format!("alex.morgan+{short_id}@universal-inbox.com")
                .parse()
                .unwrap(),
        ),
        email_validated_at: Some(Utc::now().with_nanosecond(0).unwrap()),
        email_validation_sent_at: Some(Utc::now().with_nanosecond(0).unwrap()),
        chat_support_email_signature: None,
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
    user_email: &str,
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
    let todoist_item: TodoistItem = load_seed_fixture("todoist_item.json", user_email)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_USER_EMAIL: &str = "alex.morgan+test1234@universal-inbox.com";

    #[test]
    fn template_resolves_known_tokens() {
        let input = "{{now}} {{ago_2h}} {{tomorrow_at_09}} {{date_in_3d}} {{ts_ago_5m}} \
                     {{epoch_ms_ago_1h}} {{rfc2822_ago_1d}} {{user_email}} {{unknown_token}}";
        let out = apply_seed_template(input, TEST_USER_EMAIL);
        assert!(out.contains(TEST_USER_EMAIL));
        assert!(
            out.contains("{{unknown_token}}"),
            "unknown tokens left as-is"
        );
        assert!(!out.contains("{{now}}"));
        assert!(!out.contains("{{ago_2h}}"));
        assert!(!out.contains("{{tomorrow_at_09}}"));
    }

    #[test]
    fn seed_fixtures_load_and_parse() {
        load_seed_fixture::<TodoistItem>("todoist_item.json", TEST_USER_EMAIL).unwrap();
        load_seed_fixture::<TodoistItem>("todoist_item_review.json", TEST_USER_EMAIL).unwrap();
        load_seed_fixture::<TickTickItem>("ticktick_item.json", TEST_USER_EMAIL).unwrap();
        load_seed_fixture::<GithubNotification>("github_notification.json", TEST_USER_EMAIL)
            .unwrap();
        load_seed_fixture::<Response<pull_request_query::ResponseData>>(
            "github_pull_request_123_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<Response<pull_request_query::ResponseData>>(
            "github_pull_request_review_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<Response<discussion_query::ResponseData>>(
            "github_discussion_123_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<Response<notifications_query::ResponseData>>(
            "sync_linear_notifications.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<Response<assigned_issues_query::ResponseData>>(
            "sync_linear_tasks.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiConversationsHistoryResponse>(
            "slack_fetch_thread_verbose_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiConversationsHistoryResponse>(
            "slack_fetch_thread_design_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiConversationsHistoryResponse>(
            "slack_fetch_mention_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiConversationsHistoryResponse>(
            "slack_fetch_message_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiConversationsInfoResponse>(
            "slack_fetch_channel_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiConversationsInfoResponse>(
            "slack_fetch_channel_design_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiUsersInfoResponse>(
            "slack_fetch_user_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<SlackApiTeamInfoResponse>(
            "slack_fetch_team_response.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<RawGoogleMailThread>(
            "generate_google_mail_thread.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<GoogleMailUserProfile>(
            "google_mail_user_profile.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<GoogleCalendarEvent>("google_calendar_event.json", TEST_USER_EMAIL)
            .unwrap();
        load_seed_fixture::<GoogleCalendarEvent>(
            "google_calendar_event_design_review.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
        load_seed_fixture::<GoogleDriveComment>(
            "google_drive/google_drive_comment_123.json",
            TEST_USER_EMAIL,
        )
        .unwrap();
    }
}
