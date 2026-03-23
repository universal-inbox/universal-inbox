use std::sync::Arc;

use anyhow::{Context, anyhow};
use apalis_redis::RedisStorage;
use chrono::{DateTime, Utc};
use rmcp::model::{JsonObject, object};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::RwLock;

use universal_inbox::{
    Page, PageToken,
    notification::{
        Notification, NotificationId, NotificationListOrder, NotificationSourceKind,
        NotificationStatus, NotificationSyncSourceKind, NotificationWithTask,
        service::NotificationPatch,
    },
    task::{
        Task, TaskCreation, TaskCreationResult, TaskId, TaskStatus, TaskSummary,
        TaskSyncSourceKind, service::TaskPatch,
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NotificationAction {
    MarkRead,
    Delete,
    Unsubscribe,
    SnoozeUntil,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ListNotificationsArgs {
    #[serde(default)]
    status: Vec<NotificationStatus>,
    #[serde(default)]
    sources: Vec<NotificationSourceKind>,
    include_snoozed_notifications: Option<bool>,
    order_by: Option<NotificationListOrder>,
    page_token: Option<PageToken>,
    task_id: Option<TaskId>,
    trigger_sync: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct GetNotificationArgs {
    notification_id: NotificationId,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ActOnNotificationArgs {
    notification_id: NotificationId,
    action: NotificationAction,
    snoozed_until: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct BulkActNotificationsArgs {
    #[serde(default)]
    statuses: Vec<NotificationStatus>,
    #[serde(default)]
    sources: Vec<NotificationSourceKind>,
    action: NotificationAction,
    snoozed_until: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct CreateTaskFromNotificationArgs {
    notification_id: NotificationId,
    task_creation: Option<TaskCreation>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct SyncNotificationsArgs {
    source: Option<NotificationSyncSourceKind>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ListTasksArgs {
    status: Option<TaskStatus>,
    only_synced_tasks: Option<bool>,
    trigger_sync: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct GetTaskArgs {
    task_id: TaskId,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct SearchTasksArgs {
    matches: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct UpdateTaskArgs {
    task_id: TaskId,
    patch: TaskPatch,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct SyncTasksArgs {
    source: Option<TaskSyncSourceKind>,
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_notifications",
            "title": "List notifications",
            "description": "List Universal Inbox notifications without implicitly triggering synchronization.",
            "annotations": {
                "readOnlyHint": true,
                "idempotentHint": true
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": notification_status_array_schema(),
                    "sources": notification_source_array_schema(),
                    "include_snoozed_notifications": { "type": "boolean", "default": false },
                    "order_by": notification_order_schema(),
                    "page_token": page_token_schema(),
                    "task_id": { "type": "string", "format": "uuid" },
                    "trigger_sync": { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "get_notification",
            "title": "Get notification",
            "description": "Fetch a single Universal Inbox notification.",
            "annotations": {
                "readOnlyHint": true,
                "idempotentHint": true
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notification_id": { "type": "string", "format": "uuid" }
                },
                "required": ["notification_id"]
            }
        }),
        json!({
            "name": "act_on_notification",
            "title": "Act on notification",
            "description": "Apply a single notification action. Write operations execute immediately.",
            "annotations": {
                "destructiveHint": true,
                "idempotentHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notification_id": { "type": "string", "format": "uuid" },
                    "action": {
                        "type": "string",
                        "enum": ["mark_read", "delete", "unsubscribe", "snooze_until"]
                    },
                    "snoozed_until": { "type": "string", "format": "date-time" }
                },
                "required": ["notification_id", "action"]
            }
        }),
        json!({
            "name": "bulk_act_notifications",
            "title": "Bulk act on notifications",
            "description": "Apply the same action to all matching notifications. Empty status/source filters match all notifications.",
            "annotations": {
                "destructiveHint": true,
                "idempotentHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "statuses": notification_status_array_schema(),
                    "sources": notification_source_array_schema(),
                    "action": {
                        "type": "string",
                        "enum": ["mark_read", "delete", "unsubscribe", "snooze_until"]
                    },
                    "snoozed_until": { "type": "string", "format": "date-time" }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "create_task_from_notification",
            "title": "Create task from notification",
            "description": "Create a task from a notification and link the two together.",
            "annotations": {
                "destructiveHint": true,
                "idempotentHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "notification_id": { "type": "string", "format": "uuid" },
                    "task_creation": task_creation_schema()
                },
                "required": ["notification_id"]
            }
        }),
        json!({
            "name": "sync_notifications",
            "title": "Synchronize notifications",
            "description": "Synchronize notification sources immediately and return the resulting notifications.",
            "annotations": {
                "destructiveHint": true,
                "idempotentHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": notification_sync_source_schema()
                }
            }
        }),
        json!({
            "name": "list_tasks",
            "title": "List tasks",
            "description": "List tasks synchronized through Universal Inbox (not all tasks from underlying providers like Todoist). Does not trigger synchronization unless trigger_sync is true.",
            "annotations": {
                "readOnlyHint": true,
                "idempotentHint": true
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": task_status_schema(),
                    "only_synced_tasks": { "type": "boolean", "default": true },
                    "trigger_sync": { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "get_task",
            "title": "Get task",
            "description": "Fetch a single task synchronized through Universal Inbox.",
            "annotations": {
                "readOnlyHint": true,
                "idempotentHint": true
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "format": "uuid" }
                },
                "required": ["task_id"]
            }
        }),
        json!({
            "name": "search_tasks",
            "title": "Search tasks",
            "description": "Search tasks synchronized through Universal Inbox by text.",
            "annotations": {
                "readOnlyHint": true,
                "idempotentHint": true
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "matches": { "type": "string", "minLength": 1 }
                },
                "required": ["matches"]
            }
        }),
        json!({
            "name": "update_task",
            "title": "Update task",
            "description": "Patch an existing task synchronized through Universal Inbox. Write operations execute immediately.",
            "annotations": {
                "destructiveHint": true,
                "idempotentHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "format": "uuid" },
                    "patch": task_patch_schema()
                },
                "required": ["task_id", "patch"]
            }
        }),
        json!({
            "name": "sync_tasks",
            "title": "Synchronize tasks",
            "description": "Synchronize task sources immediately and return the resulting tasks. Only synchronizes tasks tracked by Universal Inbox, not all tasks from the provider.",
            "annotations": {
                "destructiveHint": true,
                "idempotentHint": false
            },
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": task_sync_source_schema()
                }
            }
        }),
    ]
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
                    args.include_snoozed_notifications.unwrap_or(false),
                    args.task_id,
                    args.order_by
                        .unwrap_or(NotificationListOrder::UpdatedAtDesc),
                    args.sources,
                    args.page_token,
                    user_id,
                    args.trigger_sync
                        .unwrap_or(false)
                        .then(|| services.job_storage.clone()),
                )
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serde_json::to_value(page)
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
            let patch = notification_patch_from_action(args.action, args.snoozed_until)?;
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
            Ok(json!({
                "count": notifications.len(),
                "notifications": notifications
            }))
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
            Ok(json!({ "notification": notification }))
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
            Ok(json!({
                "count": notifications.len(),
                "notifications": notifications
            }))
        }
        "list_tasks" => {
            let args: ListTasksArgs = parse_args(arguments)?;
            let service = services.task_service.read().await;
            let mut transaction = service.begin().await.map_err(ToolCallError::execution)?;
            let page: Page<Task> = service
                .list_tasks(
                    &mut transaction,
                    args.status.unwrap_or(TaskStatus::Active),
                    args.only_synced_tasks.unwrap_or(true),
                    user_id,
                    args.trigger_sync
                        .unwrap_or(false)
                        .then(|| services.job_storage.clone()),
                )
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            serde_json::to_value(page)
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
                .search_tasks(&mut transaction, &args.matches, user_id)
                .await
                .map_err(ToolCallError::execution)?;
            transaction
                .commit()
                .await
                .map_err(ToolCallError::execution)?;
            Ok(json!({ "tasks": tasks }))
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
            Ok(json!({
                "count": results.len(),
                "results": results
            }))
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

fn enum_string_schema(values: &[&str]) -> Value {
    json!({
        "type": "string",
        "enum": values
    })
}

pub(crate) fn notification_status_array_schema() -> Value {
    json!({
        "type": "array",
        "items": enum_string_schema(&["Unread", "Read", "Deleted", "Unsubscribed"])
    })
}

pub(crate) fn notification_source_array_schema() -> Value {
    json!({
        "type": "array",
        "items": enum_string_schema(&[
            "Github",
            "Todoist",
            "Linear",
            "GoogleMail",
            "GoogleCalendar",
            "GoogleDrive",
            "Slack",
            "API"
        ])
    })
}

pub(crate) fn notification_order_schema() -> Value {
    enum_string_schema(&["UpdatedAtAsc", "UpdatedAtDesc"])
}

pub(crate) fn notification_sync_source_schema() -> Value {
    enum_string_schema(&["Github", "Linear", "GoogleMail", "GoogleDrive"])
}

pub(crate) fn task_status_schema() -> Value {
    enum_string_schema(&["Active", "Done", "Deleted"])
}

pub(crate) fn task_sync_source_schema() -> Value {
    enum_string_schema(&["Todoist", "Linear"])
}

fn task_priority_schema() -> Value {
    json!({
        "type": "integer",
        "enum": [1, 2, 3, 4]
    })
}

pub(crate) fn page_token_schema() -> Value {
    json!({
        "type": "object",
        "description": "Use the previous_page_token or next_page_token returned by list_notifications.",
        "additionalProperties": true
    })
}

pub(crate) fn task_creation_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "body": { "type": ["string", "null"] },
            "project_name": { "type": ["string", "null"] },
            "due_at": { "type": ["object", "string", "null"] },
            "priority": task_priority_schema()
        },
        "required": ["title", "priority"]
    })
}

pub(crate) fn task_patch_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "status": task_status_schema(),
            "project_name": { "type": ["string", "null"] },
            "due_at": { "type": ["object", "string", "null"] },
            "priority": task_priority_schema(),
            "body": { "type": ["string", "null"] },
            "title": { "type": ["string", "null"] },
            "sink_item_id": { "type": ["string", "null"], "format": "uuid" }
        }
    })
}

pub(crate) fn list_notifications_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "status": notification_status_array_schema(),
            "sources": notification_source_array_schema(),
            "include_snoozed_notifications": { "type": "boolean", "default": false },
            "order_by": notification_order_schema(),
            "page_token": page_token_schema(),
            "task_id": { "type": "string", "format": "uuid" },
            "trigger_sync": { "type": "boolean", "default": false }
        }
    }))
}

pub(crate) fn get_notification_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "notification_id": { "type": "string", "format": "uuid" }
        },
        "required": ["notification_id"]
    }))
}

pub(crate) fn act_on_notification_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "notification_id": { "type": "string", "format": "uuid" },
            "action": {
                "type": "string",
                "enum": ["mark_read", "delete", "unsubscribe", "snooze_until"]
            },
            "snoozed_until": { "type": "string", "format": "date-time" }
        },
        "required": ["notification_id", "action"]
    }))
}

pub(crate) fn bulk_act_notifications_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "statuses": notification_status_array_schema(),
            "sources": notification_source_array_schema(),
            "action": {
                "type": "string",
                "enum": ["mark_read", "delete", "unsubscribe", "snooze_until"]
            },
            "snoozed_until": { "type": "string", "format": "date-time" }
        },
        "required": ["action"]
    }))
}

pub(crate) fn create_task_from_notification_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "notification_id": { "type": "string", "format": "uuid" },
            "task_creation": task_creation_schema()
        },
        "required": ["notification_id"]
    }))
}

pub(crate) fn sync_notifications_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "source": notification_sync_source_schema()
        }
    }))
}

pub(crate) fn list_tasks_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "status": task_status_schema(),
            "only_synced_tasks": { "type": "boolean", "default": true },
            "trigger_sync": { "type": "boolean", "default": false }
        }
    }))
}

pub(crate) fn get_task_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "task_id": { "type": "string", "format": "uuid" }
        },
        "required": ["task_id"]
    }))
}

pub(crate) fn search_tasks_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "matches": { "type": "string", "minLength": 1 }
        },
        "required": ["matches"]
    }))
}

pub(crate) fn update_task_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "task_id": { "type": "string", "format": "uuid" },
            "patch": task_patch_schema()
        },
        "required": ["task_id", "patch"]
    }))
}

pub(crate) fn sync_tasks_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": {
            "source": task_sync_source_schema()
        }
    }))
}
