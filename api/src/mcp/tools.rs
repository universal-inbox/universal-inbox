use std::sync::Arc;

use anyhow::{Context, anyhow};
use apalis_redis::RedisStorage;
use chrono::{DateTime, Utc};
use rmcp::{handler::server::tool::schema_for_output, model::JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::RwLock;

use universal_inbox::{
    Page, PageToken,
    notification::{
        Notification, NotificationId, NotificationListOrder, NotificationSourceKind,
        NotificationStatus, NotificationSyncSourceKind, NotificationWithTask,
        NotificationWithTaskSummary, service::NotificationPatch,
    },
    task::{
        Task, TaskCreation, TaskCreationResult, TaskId, TaskStatus, TaskSummary,
        TaskSummaryWithStatus, TaskSyncSourceKind, service::TaskPatch,
    },
    user::UserId,
};

use crate::{
    jobs::UniversalInboxJob,
    universal_inbox::{
        UpdateStatus, notification::service::NotificationService, task::service::TaskService,
    },
};

#[derive(Clone)]
pub struct McpServices {
    pub notification_service: Arc<RwLock<NotificationService>>,
    pub task_service: Arc<RwLock<TaskService>>,
    pub job_storage: RedisStorage<UniversalInboxJob>,
}

pub enum ToolCallError {
    UnknownTool(String),
    InvalidArguments(anyhow::Error),
    Execution(anyhow::Error),
}

impl ToolCallError {
    pub fn invalid_arguments(err: anyhow::Error) -> Self {
        Self::InvalidArguments(err)
    }

    pub fn execution<E>(err: E) -> Self
    where
        E: Into<anyhow::Error>,
    {
        Self::Execution(err.into())
    }
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NotificationAction {
    MarkRead,
    Delete,
    Unsubscribe,
    SnoozeUntil,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct ListNotificationsArgs {
    #[serde(default)]
    status: Vec<NotificationStatus>,
    #[serde(default)]
    sources: Vec<NotificationSourceKind>,
    #[serde(default)]
    include_snoozed_notifications: bool,
    order_by: Option<NotificationListOrder>,
    #[schemars(
        description = "Opaque pagination cursor — pass the previous_page_token or next_page_token returned by a prior list_notifications response."
    )]
    page_token: Option<PageToken>,
    task_id: Option<TaskId>,
    #[serde(default)]
    trigger_sync: bool,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct GetNotificationArgs {
    notification_id: NotificationId,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct ActOnNotificationArgs {
    notification_id: NotificationId,
    action: NotificationAction,
    #[schemars(description = "Required when `action` is `snooze_until`; ignored otherwise.")]
    snoozed_until: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct BulkActNotificationsArgs {
    #[serde(default)]
    statuses: Vec<NotificationStatus>,
    #[serde(default)]
    sources: Vec<NotificationSourceKind>,
    action: BulkNotificationAction,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BulkNotificationAction {
    MarkRead,
    Delete,
    Unsubscribe,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct CreateTaskFromNotificationArgs {
    notification_id: NotificationId,
    task_creation: Option<TaskCreation>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct SyncNotificationsArgs {
    source: Option<NotificationSyncSourceKind>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct ListTasksArgs {
    status: Option<TaskStatus>,
    #[serde(default = "default_true")]
    only_synced_tasks: bool,
    #[serde(default)]
    trigger_sync: bool,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct GetTaskArgs {
    task_id: TaskId,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct SearchTasksArgs {
    #[schemars(length(min = 1))]
    matches: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct UpdateTaskArgs {
    task_id: TaskId,
    patch: TaskPatch,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub(crate) struct SyncTasksArgs {
    source: Option<TaskSyncSourceKind>,
}

pub async fn execute_tool(
    name: &str,
    arguments: Option<Value>,
    services: &McpServices,
    user_id: UserId,
) -> Result<Value, ToolCallError> {
    match name {
        "list_notifications" => {
            let args: ListNotificationsArgs = parse_args(arguments)?;
            let service = services.notification_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let page: Page<NotificationWithTask> = service
                .list_notifications(
                    &mut transaction,
                    args.status,
                    args.include_snoozed_notifications,
                    args.task_id,
                    args.order_by
                        .unwrap_or(NotificationListOrder::UpdatedAtDesc),
                    args.sources,
                    args.page_token,
                    user_id,
                    args.trigger_sync.then(|| services.job_storage.clone()),
                )
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            let summary_page = page.map(NotificationWithTaskSummary::from);
            serde_json::to_value(summary_page)
                .context("Failed to serialize notifications page")
                .map_err(ToolCallError::execution)
        }
        "get_notification" => {
            let args: GetNotificationArgs = parse_args(arguments)?;
            let service = services.notification_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let notification = service
                .get_notification(&mut transaction, args.notification_id, user_id)
                .await
                .map_err(ToolCallError::execution)?
                .ok_or_else(|| anyhow!("Notification {} was not found", args.notification_id))
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serde_json::to_value(notification)
                .context("Failed to serialize notification")
                .map_err(ToolCallError::execution)
        }
        "act_on_notification" => {
            let args: ActOnNotificationArgs = parse_args(arguments)?;
            let patch = notification_patch_from_action(args.action, args.snoozed_until)?;
            let service = services.notification_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let updated = service
                .patch_notification(
                    &mut transaction,
                    args.notification_id,
                    &patch,
                    true,
                    true,
                    user_id,
                )
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serialize_update_status_notification(updated, args.notification_id)
        }
        "bulk_act_notifications" => {
            let args: BulkActNotificationsArgs = parse_args(arguments)?;
            let patch = bulk_notification_patch(args.action);
            let status_filters = if args.statuses.is_empty() {
                all_notification_statuses()
            } else {
                args.statuses
            };
            let source_filters = if args.sources.is_empty() {
                all_notification_sources()
            } else {
                args.sources
            };
            let service = services.notification_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let mut storage = services.job_storage.clone();
            let notifications = service
                .patch_notifications_bulk(
                    &mut transaction,
                    status_filters,
                    source_filters,
                    &patch,
                    user_id,
                    &mut storage,
                )
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            let notifications: Vec<NotificationWithTaskSummary> = notifications
                .into_iter()
                .map(NotificationWithTaskSummary::from)
                .collect();
            serialize_result(BulkActResult {
                count: notifications.len(),
                notifications,
            })
        }
        "create_task_from_notification" => {
            let args: CreateTaskFromNotificationArgs = parse_args(arguments)?;
            let service = services.notification_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let notification = service
                .create_task_from_notification(
                    &mut transaction,
                    args.notification_id,
                    args.task_creation,
                    true,
                    user_id,
                )
                .await
                .map_err(ToolCallError::execution)?
                .ok_or_else(|| anyhow!("Notification {} was not updated", args.notification_id))
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serialize_result(CreateTaskFromNotificationResult { notification })
        }
        "sync_notifications" => {
            let args: SyncNotificationsArgs = parse_args(arguments)?;
            let service = services.notification_service.read().await;
            let notifications: Vec<Notification> = if let Some(source) = args.source {
                service
                    .sync_notifications_with_transaction(source, user_id, false)
                    .await
                    .map_err(ToolCallError::execution)?
            } else {
                service
                    .sync_all_notifications(user_id, false)
                    .await
                    .map_err(ToolCallError::execution)?
            };
            let notifications: Vec<NotificationWithTaskSummary> = notifications
                .into_iter()
                .map(NotificationWithTaskSummary::from)
                .collect();
            serialize_result(SyncNotificationsResult {
                count: notifications.len(),
                notifications,
            })
        }
        "list_tasks" => {
            let args: ListTasksArgs = parse_args(arguments)?;
            let service = services.task_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let page: Page<Task> = service
                .list_tasks(
                    &mut transaction,
                    args.status.unwrap_or(TaskStatus::Active),
                    args.only_synced_tasks,
                    user_id,
                    args.trigger_sync.then(|| services.job_storage.clone()),
                )
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            let summary_page = page.map(TaskSummaryWithStatus::from);
            serde_json::to_value(summary_page)
                .context("Failed to serialize tasks page")
                .map_err(ToolCallError::execution)
        }
        "get_task" => {
            let args: GetTaskArgs = parse_args(arguments)?;
            let service = services.task_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let task = service
                .get_task(&mut transaction, args.task_id, user_id)
                .await
                .map_err(ToolCallError::execution)?
                .ok_or_else(|| anyhow!("Task {} was not found", args.task_id))
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serde_json::to_value(task)
                .context("Failed to serialize task")
                .map_err(ToolCallError::execution)
        }
        "search_tasks" => {
            let args: SearchTasksArgs = parse_args(arguments)?;
            let service = services.task_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let tasks: Vec<TaskSummary> = service
                .search_tasks(&mut transaction, &args.matches, None, user_id)
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serialize_result(SearchTasksResult { tasks })
        }
        "update_task" => {
            let args: UpdateTaskArgs = parse_args(arguments)?;
            let service = services.task_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let updated = service
                .patch_task(&mut transaction, args.task_id, &args.patch, user_id)
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serialize_update_status_task(updated, args.task_id)
        }
        "sync_tasks" => {
            let args: SyncTasksArgs = parse_args(arguments)?;
            let service = services.task_service.read().await;
            let results: Vec<TaskCreationResult> = if let Some(source) = args.source {
                service
                    .sync_tasks_with_transaction(source, user_id, false)
                    .await
                    .map_err(ToolCallError::execution)?
            } else {
                service
                    .sync_all_tasks(user_id, false)
                    .await
                    .map_err(ToolCallError::execution)?
            };
            let tasks: Vec<TaskSummaryWithStatus> = results
                .into_iter()
                .map(|r| TaskSummaryWithStatus::from(r.task))
                .collect();
            serialize_result(SyncTasksResult {
                count: tasks.len(),
                tasks,
            })
        }
        _ => Err(ToolCallError::UnknownTool(name.to_string())),
    }
}

fn parse_args<T>(arguments: Option<Value>) -> Result<T, ToolCallError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.unwrap_or_else(|| json!({})))
        .context("Invalid tool arguments")
        .map_err(ToolCallError::invalid_arguments)
}

fn notification_patch_from_action(
    action: NotificationAction,
    snoozed_until: Option<DateTime<Utc>>,
) -> Result<NotificationPatch, ToolCallError> {
    Ok(match action {
        NotificationAction::MarkRead => NotificationPatch {
            status: Some(NotificationStatus::Read),
            ..Default::default()
        },
        NotificationAction::Delete => NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
        NotificationAction::Unsubscribe => NotificationPatch {
            status: Some(NotificationStatus::Unsubscribed),
            ..Default::default()
        },
        NotificationAction::SnoozeUntil => NotificationPatch {
            snoozed_until: Some(
                snoozed_until
                    .ok_or_else(|| anyhow!("`snoozed_until` is required for `snooze_until`"))
                    .map_err(ToolCallError::invalid_arguments)?,
            ),
            ..Default::default()
        },
    })
}

fn bulk_notification_patch(action: BulkNotificationAction) -> NotificationPatch {
    match action {
        BulkNotificationAction::MarkRead => NotificationPatch {
            status: Some(NotificationStatus::Read),
            ..Default::default()
        },
        BulkNotificationAction::Delete => NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
        BulkNotificationAction::Unsubscribe => NotificationPatch {
            status: Some(NotificationStatus::Unsubscribed),
            ..Default::default()
        },
    }
}

fn serialize_update_status_notification(
    update_status: UpdateStatus<Box<Notification>>,
    notification_id: NotificationId,
) -> Result<Value, ToolCallError> {
    match update_status {
        UpdateStatus {
            updated: _,
            result: Some(notification),
        } => serde_json::to_value(notification)
            .context("Failed to serialize notification")
            .map_err(ToolCallError::execution),
        UpdateStatus {
            updated: _,
            result: None,
        } => Err(ToolCallError::execution(anyhow!(
            "Notification {} was not updated",
            notification_id
        ))),
    }
}

fn serialize_update_status_task(
    update_status: UpdateStatus<Box<Task>>,
    task_id: TaskId,
) -> Result<Value, ToolCallError> {
    match update_status {
        UpdateStatus {
            updated: _,
            result: Some(task),
        } => serde_json::to_value(task)
            .context("Failed to serialize task")
            .map_err(ToolCallError::execution),
        UpdateStatus {
            updated: _,
            result: None,
        } => Err(ToolCallError::execution(anyhow!(
            "Task {} was not updated",
            task_id
        ))),
    }
}

fn all_notification_statuses() -> Vec<NotificationStatus> {
    vec![
        NotificationStatus::Unread,
        NotificationStatus::Read,
        NotificationStatus::Deleted,
        NotificationStatus::Unsubscribed,
    ]
}

fn all_notification_sources() -> Vec<NotificationSourceKind> {
    vec![
        NotificationSourceKind::Github,
        NotificationSourceKind::Todoist,
        NotificationSourceKind::Linear,
        NotificationSourceKind::GoogleMail,
        NotificationSourceKind::GoogleCalendar,
        NotificationSourceKind::GoogleDrive,
        NotificationSourceKind::Slack,
        NotificationSourceKind::API,
    ]
}

/// Result wrappers for tools whose response is a small object with named
/// fields. They are typed so each tool has a single `T` to point
/// `schema_for_output::<T>()` at, and so the executor constructs them
/// statically instead of building ad-hoc `json!` literals.
#[derive(Serialize, JsonSchema)]
pub struct BulkActResult {
    pub count: usize,
    pub notifications: Vec<NotificationWithTaskSummary>,
}

#[derive(Serialize, JsonSchema)]
pub struct CreateTaskFromNotificationResult {
    pub notification: NotificationWithTask,
}

#[derive(Serialize, JsonSchema)]
pub struct SyncNotificationsResult {
    pub count: usize,
    pub notifications: Vec<NotificationWithTaskSummary>,
}

#[derive(Serialize, JsonSchema)]
pub struct SearchTasksResult {
    pub tasks: Vec<TaskSummary>,
}

#[derive(Serialize, JsonSchema)]
pub struct SyncTasksResult {
    pub count: usize,
    pub tasks: Vec<TaskSummaryWithStatus>,
}

fn serialize_result<T: Serialize>(value: T) -> Result<Value, ToolCallError> {
    serde_json::to_value(value)
        .context("Failed to serialize tool result")
        .map_err(ToolCallError::execution)
}

fn output_schema_for<T: JsonSchema + 'static>(tool: &'static str) -> Arc<JsonObject> {
    schema_for_output::<T>()
        .unwrap_or_else(|err| panic!("`{tool}` outputSchema does not have an object root: {err}"))
}

pub(crate) fn list_notifications_output_schema() -> Arc<JsonObject> {
    output_schema_for::<Page<NotificationWithTaskSummary>>("list_notifications")
}

pub(crate) fn get_notification_output_schema() -> Arc<JsonObject> {
    output_schema_for::<Notification>("get_notification")
}

pub(crate) fn act_on_notification_output_schema() -> Arc<JsonObject> {
    output_schema_for::<Notification>("act_on_notification")
}

pub(crate) fn bulk_act_notifications_output_schema() -> Arc<JsonObject> {
    output_schema_for::<BulkActResult>("bulk_act_notifications")
}

pub(crate) fn create_task_from_notification_output_schema() -> Arc<JsonObject> {
    output_schema_for::<CreateTaskFromNotificationResult>("create_task_from_notification")
}

pub(crate) fn sync_notifications_output_schema() -> Arc<JsonObject> {
    output_schema_for::<SyncNotificationsResult>("sync_notifications")
}

pub(crate) fn list_tasks_output_schema() -> Arc<JsonObject> {
    output_schema_for::<Page<TaskSummaryWithStatus>>("list_tasks")
}

pub(crate) fn get_task_output_schema() -> Arc<JsonObject> {
    output_schema_for::<Task>("get_task")
}

pub(crate) fn search_tasks_output_schema() -> Arc<JsonObject> {
    output_schema_for::<SearchTasksResult>("search_tasks")
}

pub(crate) fn update_task_output_schema() -> Arc<JsonObject> {
    output_schema_for::<Task>("update_task")
}

pub(crate) fn sync_tasks_output_schema() -> Arc<JsonObject> {
    output_schema_for::<SyncTasksResult>("sync_tasks")
}

#[cfg(test)]
mod output_schema_tests {
    use super::*;

    fn assert_object_with_keys(schema: &JsonObject, required: &[&str]) {
        assert_eq!(
            schema.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "schema must declare type=object"
        );
        let actual_required: Vec<&str> = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        for key in required {
            assert!(
                actual_required.contains(key),
                "expected required key `{key}` in schema, got {actual_required:?}"
            );
        }
    }

    #[test]
    fn list_notifications_output_schema_shape() {
        let schema = list_notifications_output_schema();
        assert_object_with_keys(&schema, &["per_page", "pages_count", "total", "content"]);
    }

    #[test]
    fn get_notification_output_schema_shape() {
        let schema = get_notification_output_schema();
        assert_object_with_keys(&schema, &["id", "title", "status", "kind", "source_item"]);
    }

    #[test]
    fn act_on_notification_output_schema_shape() {
        let schema = act_on_notification_output_schema();
        assert_object_with_keys(&schema, &["id", "status", "source_item"]);
    }

    #[test]
    fn bulk_act_notifications_output_schema_shape() {
        let schema = bulk_act_notifications_output_schema();
        assert_object_with_keys(&schema, &["count", "notifications"]);
    }

    #[test]
    fn create_task_from_notification_output_schema_shape() {
        let schema = create_task_from_notification_output_schema();
        assert_object_with_keys(&schema, &["notification"]);
    }

    #[test]
    fn sync_notifications_output_schema_shape() {
        let schema = sync_notifications_output_schema();
        assert_object_with_keys(&schema, &["count", "notifications"]);
    }

    #[test]
    fn list_tasks_output_schema_shape() {
        let schema = list_tasks_output_schema();
        assert_object_with_keys(&schema, &["per_page", "pages_count", "total", "content"]);
    }

    #[test]
    fn get_task_output_schema_shape() {
        let schema = get_task_output_schema();
        assert_object_with_keys(&schema, &["id", "title", "status", "kind", "source_item"]);
    }

    #[test]
    fn search_tasks_output_schema_shape() {
        let schema = search_tasks_output_schema();
        assert_object_with_keys(&schema, &["tasks"]);
    }

    #[test]
    fn update_task_output_schema_shape() {
        let schema = update_task_output_schema();
        assert_object_with_keys(&schema, &["id", "title", "status", "source_item"]);
    }

    #[test]
    fn sync_tasks_output_schema_shape() {
        let schema = sync_tasks_output_schema();
        assert_object_with_keys(&schema, &["count", "tasks"]);
    }

    #[test]
    #[ignore = "manual visual inspection only"]
    fn dump_notification_schema() {
        let schema = get_notification_output_schema();
        println!(
            "{}",
            serde_json::to_string_pretty(&*schema).expect("serialize")
        );
    }

    /// Sanity: the opaque-data annotation on `ThirdPartyItem` must keep the
    /// 11-variant provider union out of `Notification`'s schema.
    #[test]
    fn third_party_item_data_is_opaque() {
        let schema = get_notification_output_schema();
        let serialized = serde_json::to_string(&schema).expect("schema must serialize to JSON");
        for variant in [
            "TodoistItem",
            "TickTickItem",
            "SlackReaction",
            "SlackThread",
            "LinearIssue",
            "LinearNotification",
            "GithubNotification",
            "GoogleMailThread",
            "GoogleCalendarEvent",
            "GoogleDriveComment",
            "WebPage",
        ] {
            assert!(
                !serialized.contains(variant),
                "`{variant}` leaked into the Notification outputSchema — the opaque-`data` annotation regressed"
            );
        }
    }
}
