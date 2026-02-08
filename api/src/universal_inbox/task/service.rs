use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{Context, anyhow};
use apalis_redis::RedisStorage;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use universal_inbox::{
    HasHtmlUrl, Page,
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{
        Notification, NotificationSource, NotificationStatus, service::NotificationPatch,
    },
    task::{
        CreateOrUpdateTaskRequest, ProjectSummary, Task, TaskCreation, TaskCreationConfig,
        TaskCreationResult, TaskId, TaskSource, TaskSourceKind, TaskStatus, TaskSummary,
        TaskSyncSourceKind, service::TaskPatch,
    },
    third_party::{
        integrations::slack::{SlackReaction, SlackStar},
        item::{
            ThirdPartyItem, ThirdPartyItemFromSource, ThirdPartyItemSource,
            ThirdPartyItemSourceKind,
        },
    },
    user::UserId,
};

use crate::{
    integrations::{
        linear::LinearService,
        slack::SlackService,
        task::{ThirdPartyTaskService, ThirdPartyTaskSourceService},
        third_party::ThirdPartyItemSourceService,
        todoist::TodoistService,
    },
    jobs::UniversalInboxJob,
    repository::{Repository, task::TaskRepository},
    universal_inbox::{
        UniversalInboxError, UpdateStatus, UpsertStatus,
        integration_connection::service::{
            IntegrationConnectionService, IntegrationConnectionSyncType,
        },
        notification::service::NotificationService,
        third_party::service::ThirdPartyItemService,
        user::service::UserService,
    },
};

pub struct TaskService {
    repository: Arc<Repository>,
    pub todoist_service: Arc<TodoistService>,
    pub linear_service: Arc<LinearService>,
    notification_service: Weak<RwLock<NotificationService>>,
    pub slack_service: Arc<SlackService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    user_service: Arc<UserService>,
    pub(super) third_party_item_service: Weak<RwLock<ThirdPartyItemService>>,
    min_sync_tasks_interval_in_minutes: i64,
}

impl TaskService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: Arc<Repository>,
        todoist_service: Arc<TodoistService>,
        linear_service: Arc<LinearService>,
        notification_service: Weak<RwLock<NotificationService>>,
        slack_service: Arc<SlackService>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        user_service: Arc<UserService>,
        third_party_item_service: Weak<RwLock<ThirdPartyItemService>>,
        min_sync_tasks_interval_in_minutes: i64,
    ) -> TaskService {
        TaskService {
            repository,
            todoist_service,
            linear_service,
            notification_service,
            slack_service,
            integration_connection_service,
            user_service,
            third_party_item_service,
            min_sync_tasks_interval_in_minutes,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            patch,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn apply_updated_task_side_effect<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_task_service: Arc<U>,
        patch: &TaskPatch,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError>
    where
        U: ThirdPartyTaskService<T> + Send + Sync,
    {
        match patch.status {
            Some(TaskStatus::Deleted) => {
                debug!(
                    "Deleting {} task {}",
                    third_party_item.get_third_party_item_source_kind(),
                    third_party_item.source_id
                );
                match third_party_task_service
                    .delete_task(executor, third_party_item, user_id)
                    .await
                {
                    Err(UniversalInboxError::ItemNotFound(_)) => {}
                    Err(e) => return Err(e),
                    _ => {}
                }
            }
            Some(TaskStatus::Done) => {
                debug!(
                    "Completing {} task {}",
                    third_party_item.get_third_party_item_source_kind(),
                    third_party_item.source_id
                );
                third_party_task_service
                    .complete_task(executor, third_party_item, user_id)
                    .await?;
            }
            Some(TaskStatus::Active) => {
                debug!(
                    "Uncompleting {} task {}",
                    third_party_item.get_third_party_item_source_kind(),
                    third_party_item.source_id
                );
                third_party_task_service
                    .uncomplete_task(executor, third_party_item, user_id)
                    .await?;
            }
            _ => (),
        }

        debug!(
            "Updating {} task {}",
            third_party_item.get_third_party_item_source_kind(),
            third_party_item.source_id
        );
        third_party_task_service
            .update_task(executor, &third_party_item.source_id, patch, user_id)
            .await?;

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = synced_third_party_item.id.to_string(),
            third_party_item_source_id = synced_third_party_item.source_id,
            task_id = upsert_task.value_ref().id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn apply_synced_task_side_effect(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        synced_third_party_item: &ThirdPartyItem,
        upsert_task: &mut UpsertStatus<Box<Task>>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        match upsert_task {
            UpsertStatus::Created(task) => {
                if task.kind == TaskSourceKind::Todoist {
                    debug!(
                        "No side effect to apply for newly created {} task {}",
                        task.kind, task.id
                    );
                    return Ok(());
                }

                debug!(
                    "Applying side effect for newly created {} task {} and {} third party item {}",
                    task.kind,
                    task.id,
                    synced_third_party_item.kind(),
                    synced_third_party_item.id
                );

                self.third_party_item_service
                    .upgrade()
                    .context("Unable to access third_party_item_service from task_service")?
                    .read()
                    .await
                    .create_sink_item_from_task(executor, task, false)
                    .await?;
            }
            UpsertStatus::Updated {
                new: new_task,
                old: old_task,
            } => {
                let task_source_item = &new_task.source_item;

                if new_task.sink_item.is_none() {
                    if new_task.kind == TaskSourceKind::Todoist {
                        debug!(
                            "No side effect to apply for {} task {} with no sink item",
                            new_task.kind, new_task.id
                        );
                        return Ok(());
                    }

                    debug!(
                        "Creating new sink item for {} task {} as it's missing",
                        new_task.kind, new_task.id
                    );

                    self.third_party_item_service
                        .upgrade()
                        .context("Unable to access third_party_item_service from task_service")?
                        .read()
                        .await
                        .create_sink_item_from_task(executor, new_task, false)
                        .await?;

                    return Ok(());
                }

                let task_sink_item = new_task.sink_item.as_ref().ok_or_else(|| {
                    UniversalInboxError::Unexpected(anyhow!(
                        "Task {} has no sink item, cannot apply side effect",
                        new_task.id
                    ))
                })?;
                if task_source_item.id == task_sink_item.id {
                    debug!(
                        "No side effect to apply for {} task {}",
                        new_task.kind, new_task.id
                    );
                    return Ok(());
                }

                let third_party_item_to_be_updated = if task_source_item.id
                    == synced_third_party_item.id
                {
                    task_sink_item
                } else if task_sink_item.id == synced_third_party_item.id {
                    task_source_item
                } else {
                    return Err(UniversalInboxError::Unexpected(anyhow!(
                        "Task {} has no source or sink item matching the synced third party item {}",
                        new_task.id,
                        synced_third_party_item.id
                    )));
                };

                debug!(
                    "Applying side effect for updated {} task {} and {} third party item {}",
                    new_task.kind,
                    new_task.id,
                    synced_third_party_item.kind(),
                    synced_third_party_item.id
                );

                let task_patch = TaskPatch {
                    status: (new_task.status != old_task.status).then_some(new_task.status),
                    project_name: (new_task.project != old_task.project)
                        .then(|| new_task.project.clone()),
                    due_at: (new_task.due_at != old_task.due_at).then(|| new_task.due_at.clone()),
                    priority: (new_task.priority != old_task.priority).then_some(new_task.priority),
                    body: (new_task.body != old_task.body).then(|| new_task.body.clone()),
                    title: (new_task.title != old_task.title).then(|| new_task.title.clone()),
                    sink_item_id: None,
                };

                let side_effect_result = self
                    .apply_task_third_party_item_side_effect(
                        executor,
                        &task_patch,
                        third_party_item_to_be_updated,
                        user_id,
                    )
                    .await;

                let Err(UniversalInboxError::ItemNotFound(_)) = side_effect_result else {
                    return side_effect_result;
                };

                if new_task.kind == TaskSourceKind::Todoist {
                    // Return the error as there is no fallback in that case
                    return side_effect_result;
                }

                debug!(
                    "Creating new sink item from {} task {} as update failed because current {} sink item {} was not found upstream",
                    new_task.kind,
                    new_task.id,
                    task_sink_item.kind(),
                    task_sink_item.source_id
                );

                self.third_party_item_service
                    .upgrade()
                    .context("Unable to access third_party_item_service from task_service")?
                    .read()
                    .await
                    .create_sink_item_from_task(executor, new_task, true)
                    .await?;
            }
            _ => {}
        }

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            status = status.to_string(),
            only_synced_tasks,
            user.id = user_id.to_string(),
            trigger_sync = job_storage.is_none()
        ),
        err
    )]
    pub async fn list_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: TaskStatus,
        only_synced_tasks: bool,
        user_id: UserId,
        job_storage: Option<RedisStorage<UniversalInboxJob>>,
    ) -> Result<Page<Task>, UniversalInboxError> {
        let tasks_page = self
            .repository
            .fetch_all_tasks(executor, status, only_synced_tasks, user_id)
            .await?;

        if let Some(job_storage) = job_storage {
            self.integration_connection_service
                .read()
                .await
                .trigger_sync_for_integration_connections(executor, user_id, job_storage)
                .await?;
        }

        Ok(tasks_page)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            matches,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn search_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        matches: &str,
        user_id: UserId,
    ) -> Result<Vec<TaskSummary>, UniversalInboxError> {
        self.repository
            .search_tasks(executor, matches, user_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_id = task_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn get_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        for_user_id: UserId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        let task = self.repository.get_one_task(executor, task_id).await?;

        if let Some(ref task) = task
            && task.user_id != for_user_id
        {
            return Err(UniversalInboxError::Forbidden(format!(
                "Only the owner of the task {task_id} can access it"
            )));
        }

        Ok(task)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(task_ids = task_ids.iter().map(|id| id.to_string()).collect::<Vec<String>>().join(", ")),
        err
    )]
    pub async fn get_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        self.repository.get_tasks(executor, task_ids).await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_id = task.id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn create_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task: Box<Task>,
        for_user_id: UserId,
    ) -> Result<Box<TaskCreationResult>, UniversalInboxError> {
        if task.user_id != for_user_id {
            return Err(UniversalInboxError::Forbidden(format!(
                "A task can only be created for {for_user_id}"
            )));
        }

        let task = self.repository.create_task(executor, task).await?;
        Ok(Box::new(TaskCreationResult {
            task: *task,
            notifications: vec![], // notification.into_iter().map(|n| *n).collect(),
        }))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(notification_id = notification.id.to_string()),
        err
    )]
    pub async fn create_task_from_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_creation: &TaskCreation,
        notification: &Notification,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let user_id = notification.user_id;
        let third_party_task_service = self.todoist_service.clone();
        let integration_provider_kind = third_party_task_service.get_integration_provider_kind();
        let Some(integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .get_integration_connection_to_sync(
                executor,
                integration_provider_kind,
                0,
                IntegrationConnectionSyncType::Tasks,
                user_id,
            )
            .await?
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "No validated {integration_provider_kind} integration found for user {user_id}, cannot create a task from the notification {}",
                notification.id
            )));
        };

        let third_party_task = third_party_task_service
            .create_task(
                executor,
                &TaskCreation {
                    body: Some(format!(
                        "- [{}]({})",
                        notification.title,
                        notification.get_html_url()
                    )),
                    ..(*task_creation).clone()
                },
                user_id,
            )
            .await?;

        let third_party_item =
            third_party_task.into_third_party_item(user_id, integration_connection.id);

        let upsert_item = self
            .third_party_item_service
            .upgrade()
            .context("Unable to access third_party_item_service from task_service")?
            .read()
            .await
            .create_or_update_third_party_item(executor, Box::new(third_party_item))
            .await?;

        let updated_third_party_item = upsert_item.value();
        let source_id = updated_third_party_item.source_id.clone();

        if let Some(TaskCreationResult { task, .. }) = self
            .create_task_from_third_party_item(
                executor,
                *updated_third_party_item,
                third_party_task_service,
                user_id,
            )
            .await?
        {
            Ok(Box::new(task))
        } else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "A task should have been created from the {integration_provider_kind} task {source_id}",
            )));
        }
    }

    /// Create a task from a third party item and apply side effects
    /// Then create a notification from the task if needed and return
    /// the task and the notification if any
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn create_task_from_third_party_item<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: ThirdPartyItem,
        third_party_task_service: Arc<U>,
        user_id: UserId,
    ) -> Result<Option<TaskCreationResult>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T> + NotificationSource + TaskSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let integration_provider_kind = third_party_item.get_integration_provider_kind();
        let Some(integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .get_validated_integration_connection_per_kind(
                executor,
                integration_provider_kind,
                user_id,
            )
            .await?
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "No validated {integration_provider_kind} integration found for user {user_id}, cannot create third party item"
            )));
        };

        let task_creation = integration_connection
            .provider
            .get_task_creation_default_values(&third_party_item);

        let upsert_task = self
            .sync_third_party_item_as_task(
                executor,
                third_party_task_service.clone(),
                &third_party_item,
                task_creation,
                user_id,
            )
            .await?;

        let task_is_modified = upsert_task.is_modified();
        let task = upsert_task.value();
        if !task_is_modified {
            debug!(
                "Task {} for third party item {} is already up to date",
                task.id, third_party_item.id
            );
            return Ok(Some(TaskCreationResult {
                task: *task,
                notifications: vec![],
            }));
        };

        let upsert_notification = if integration_provider_kind == IntegrationProviderKind::Todoist {
            self.notification_service
                .upgrade()
                .context("Unable to access notification_service from task_service")?
                .read()
                .await
                .save_task_as_notification(
                    executor,
                    self.todoist_service.clone(),
                    &task,
                    &third_party_item,
                    &integration_connection.provider,
                    true, // Force incremental here to avoid deleting all other notification for this third party item kind
                    user_id,
                )
                .await?
        } else {
            None
        };

        let Some(upsert_notification) = upsert_notification else {
            return Ok(Some(TaskCreationResult {
                task: *task,
                notifications: vec![],
            }));
        };

        let notification_is_modified = upsert_notification.is_modified();
        let notification = upsert_notification.value();
        if !notification_is_modified {
            debug!(
                "Notification {} for task {} is already up to date",
                notification.id, task.id
            );
        }

        Ok(Some(TaskCreationResult {
            task: *task,
            notifications: vec![*notification],
        }))
    }

    async fn sync_third_party_tasks<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_task_service: Arc<U>,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T>
            + ThirdPartyItemSourceService<T>
            + NotificationSource
            + TaskSource
            + Send
            + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        async fn sync_third_party_tasks<T, U>(
            task_service: &TaskService,
            executor: &mut Transaction<'_, Postgres>,
            third_party_task_service: Arc<U>,
            user_id: UserId,
            last_tasks_sync_completed_at: Option<DateTime<Utc>>,
        ) -> Result<Vec<TaskCreationResult>, UniversalInboxError>
        where
            T: TryFrom<ThirdPartyItem> + Debug,
            U: ThirdPartyTaskService<T>
                + ThirdPartyItemSourceService<T>
                + NotificationSource
                + TaskSource
                + Send
                + Sync,
            <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
        {
            let third_party_items = task_service
                .third_party_item_service
                .upgrade()
                .context("Unable to access third_party_item_service from task_service")?
                .read()
                .await
                .sync_items(
                    executor,
                    third_party_task_service.clone(),
                    user_id,
                    last_tasks_sync_completed_at,
                )
                .await?;

            let mut task_creation_results = vec![];
            for third_party_item in third_party_items {
                if let Some(task_creation_result) = task_service
                    .create_task_from_third_party_item(
                        executor,
                        third_party_item,
                        third_party_task_service.clone(),
                        user_id,
                    )
                    .await?
                {
                    task_creation_results.push(task_creation_result);
                }
            }
            Ok(task_creation_results)
        }

        let integration_provider_kind = third_party_task_service.get_integration_provider_kind();
        let integration_connection_service = self.integration_connection_service.read().await;
        let min_sync_interval_in_minutes = if !force_sync {
            self.min_sync_tasks_interval_in_minutes
        } else {
            Default::default()
        };
        let Some(integration_connection) = integration_connection_service
            .get_integration_connection_to_sync(
                executor,
                integration_provider_kind,
                min_sync_interval_in_minutes,
                IntegrationConnectionSyncType::Tasks,
                user_id,
            )
            .await?
        else {
            debug!(
                "No validated {integration_provider_kind} integration found for user {user_id}, skipping tasks sync"
            );
            return Ok(vec![]);
        };

        if !integration_connection.provider.is_sync_tasks_enabled() {
            debug!(
                "{integration_provider_kind} integration for user {user_id} is disabled, skipping tasks sync"
            );
            return Ok(vec![]);
        }

        info!("Syncing {integration_provider_kind} tasks for user {user_id}");
        integration_connection_service
            .start_tasks_sync_status(executor, integration_provider_kind, user_id)
            .await?;

        let task_creation_results = match sync_third_party_tasks(
            self,
            executor,
            third_party_task_service,
            user_id,
            integration_connection.last_tasks_sync_completed_at,
        )
        .await
        {
            Err(e) => {
                integration_connection_service
                    .error_tasks_sync_status(
                        executor,
                        integration_provider_kind,
                        format!("Failed to fetch tasks from {integration_provider_kind}"),
                        user_id,
                    )
                    .await?;
                return Err(UniversalInboxError::Recoverable(e.into()));
            }
            Ok(task_creation_results) => {
                integration_connection_service
                    .complete_tasks_sync_status(executor, integration_provider_kind, user_id)
                    .await?;
                task_creation_results
            }
        };

        info!(
            "Successfully synced {} {integration_provider_kind} tasks for user {user_id}",
            task_creation_results.len()
        );

        Ok(task_creation_results)
    }

    /// Save a third party item as a task and apply side effects
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn sync_third_party_item_as_task<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_task_service: Arc<U>,
        third_party_item: &ThirdPartyItem,
        task_creation_config: Option<TaskCreationConfig>,
        user_id: UserId,
    ) -> Result<UpsertStatus<Box<Task>>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T> + TaskSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let mut upsert_task = self
            .save_third_party_item_as_task(
                executor,
                third_party_task_service,
                third_party_item,
                task_creation_config,
                user_id,
            )
            .await?;

        if upsert_task.modified_value_ref().is_some() {
            self.apply_synced_task_side_effect(
                executor,
                third_party_item,
                &mut upsert_task,
                user_id,
            )
            .await?;
        }

        Ok(upsert_task)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn save_third_party_item_as_task<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_task_service: Arc<U>,
        third_party_item: &ThirdPartyItem,
        task_creation_config: Option<TaskCreationConfig>,
        user_id: UserId,
    ) -> Result<UpsertStatus<Box<Task>>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T> + TaskSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let data: T = third_party_item.clone().try_into().map_err(|_| {
            anyhow!(
                "Unexpected third party item kind {} for {}",
                third_party_item.kind(),
                third_party_task_service.get_integration_provider_kind()
            )
        })?;

        let task_request = third_party_task_service
            .third_party_item_into_task(
                executor,
                &data,
                third_party_item,
                task_creation_config,
                user_id,
            )
            .await?;
        self.repository
            .create_or_update_task(executor, task_request)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_item_id = task_request.source_item.id.to_string(),
            source_item_source_id = task_request.source_item.source_id,
            user.id = task_request.user_id.to_string()
        ),
        err
    )]
    pub async fn create_or_update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_request: Box<CreateOrUpdateTaskRequest>,
    ) -> Result<UpsertStatus<Box<Task>>, UniversalInboxError> {
        self.repository
            .create_or_update_task(executor, task_request)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source = source.to_string(),
            user.id = user_id.to_string(),
            force_sync
        ),
        err
    )]
    pub async fn sync_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source: TaskSyncSourceKind,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        match source {
            TaskSyncSourceKind::Todoist => {
                self.sync_third_party_tasks(
                    executor,
                    self.todoist_service.clone(),
                    user_id,
                    force_sync,
                )
                .await
            }
            TaskSyncSourceKind::Linear => {
                self.sync_third_party_tasks(
                    executor,
                    self.linear_service.clone(),
                    user_id,
                    force_sync,
                )
                .await
            }
            TaskSyncSourceKind::TickTick => {
                // TickTick sync will be implemented in Phase 3 (TICK-3.4)
                Ok(vec![])
            }
        }
    }

    pub async fn sync_tasks_with_transaction(
        &self,
        source: TaskSyncSourceKind,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        let mut transaction = self.begin().await.context(format!(
            "Failed to create new transaction while syncing {source:?}"
        ))?;

        match self
            .sync_tasks(&mut transaction, source, user_id, force_sync)
            .await
        {
            Ok(tasks) => {
                transaction
                    .commit()
                    .await
                    .context(format!("Failed to commit while syncing {source:?}"))?;
                Ok(tasks)
            }
            Err(error @ UniversalInboxError::Recoverable(_)) => {
                transaction
                    .commit()
                    .await
                    .context(format!("Failed to commit while syncing {source:?}"))?;
                Err(error)
            }
            Err(error) => {
                transaction
                    .rollback()
                    .await
                    .context(format!("Failed to rollback while syncing {source:?}"))?;
                Err(error)
            }
        }
    }

    pub async fn sync_all_tasks(
        &self,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        let sync_result_from_todoist = self
            .sync_tasks_with_transaction(TaskSyncSourceKind::Todoist, user_id, force_sync)
            .await?;
        let sync_result_from_linear = self
            .sync_tasks_with_transaction(TaskSyncSourceKind::Linear, user_id, force_sync)
            .await?;
        Ok(sync_result_from_todoist
            .into_iter()
            .chain(sync_result_from_linear.into_iter())
            .collect())
    }

    pub async fn sync_tasks_for_all_users(
        &self,
        source: Option<TaskSyncSourceKind>,
        force_sync: bool,
    ) -> Result<(), UniversalInboxError> {
        let service = self.user_service.clone();
        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while syncing tasks for all users")?;
        let users = service.fetch_all_users(&mut transaction).await?;

        for user in users {
            let _ = self.sync_tasks_for_user(source, user.id, force_sync).await;
        }

        Ok(())
    }

    pub async fn sync_tasks_for_user(
        &self,
        source: Option<TaskSyncSourceKind>,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<(), UniversalInboxError> {
        info!("Syncing tasks for user {user_id}");

        let sync_result = if let Some(source) = source {
            self.sync_tasks_with_transaction(source, user_id, force_sync)
                .await
        } else {
            self.sync_all_tasks(user_id, force_sync).await
        };
        match sync_result {
            Ok(tasks) => info!(
                "{} tasks successfully synced for user {user_id}",
                tasks.len()
            ),
            Err(err) => error!("Failed to sync tasks for user {user_id}: {err:?}"),
        };

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_id = task_id.to_string(),
            patch,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn patch_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        patch: &TaskPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        let updated_task = self
            .repository
            .update_task(executor, task_id, patch, for_user_id)
            .await?;

        if let UpdateStatus {
            updated: _,
            result: Some(task),
        } = &updated_task
            && task.kind == TaskSourceKind::Todoist
            && patch.status.is_some()
        {
            let notification_patch = NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            };

            self.notification_service
                .upgrade()
                .context("Unable to access notification_service from task_service")?
                .read()
                .await
                .patch_notifications_for_task(executor, task.id, None, &notification_patch)
                .await?;
        }

        match updated_task {
            UpdateStatus {
                updated: true,
                result: Some(ref task),
            } => {
                self.apply_task_third_party_item_side_effect(
                    executor,
                    patch,
                    &task.source_item,
                    for_user_id,
                )
                .await?;

                if let Some(sink_item) = &task.sink_item
                    && task.source_item.id != sink_item.id
                {
                    self.apply_task_third_party_item_side_effect(
                        executor,
                        patch,
                        sink_item,
                        for_user_id,
                    )
                    .await?;
                }
            }
            UpdateStatus {
                updated: false,
                result: None,
            } => {
                if self.repository.does_task_exist(executor, task_id).await? {
                    return Err(UniversalInboxError::Forbidden(format!(
                        "Only the owner of the task {task_id} can patch it"
                    )));
                }
            }
            _ => {}
        }

        Ok(updated_task)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification.id.to_string(),
            task_id = task_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn link_notification_with_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification: &Notification,
        task_id: TaskId,
        for_user_id: UserId,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let task = self
            .get_task(executor, task_id, for_user_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Cannot link notification {} with unknown task {task_id}",
                    notification.id
                ))
            })?;

        let updated_task = self
            .patch_task(
                executor,
                task_id,
                &TaskPatch {
                    body: Some(format!(
                        "{}\n- [{}]({})",
                        task.body,
                        notification.title,
                        notification.get_html_url()
                    )),
                    ..Default::default()
                },
                for_user_id,
            )
            .await?;

        if let Some(task) = updated_task.result {
            Ok(task)
        } else {
            Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot update task {task_id} body while linking notification {} to it",
                notification.id
            )))
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            matches,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn search_projects(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        matches: &str,
        user_id: UserId,
    ) -> Result<Vec<ProjectSummary>, UniversalInboxError> {
        self.todoist_service
            .search_projects(executor, matches, user_id)
            .await
    }

    pub async fn get_or_create_project(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        project_name: &str,
        user_id: UserId,
    ) -> Result<ProjectSummary, UniversalInboxError> {
        self.todoist_service
            .get_or_create_project(executor, project_name, user_id, None)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            patch,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn apply_task_third_party_item_side_effect(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        patch: &TaskPatch,
        third_party_item: &ThirdPartyItem,
        for_user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Skip side effects for test accounts
        let user = self.user_service.get_user(executor, for_user_id).await?;
        if let Some(user) = user
            && user.is_testing
        {
            debug!("Skipping task side effects for test account {for_user_id}");
            return Ok(());
        }

        match third_party_item.get_third_party_item_source_kind() {
            ThirdPartyItemSourceKind::Todoist => {
                self.apply_updated_task_side_effect(
                    executor,
                    self.todoist_service.clone(),
                    patch,
                    third_party_item,
                    for_user_id,
                )
                .await
            }
            ThirdPartyItemSourceKind::LinearIssue => {
                self.apply_updated_task_side_effect(
                    executor,
                    self.linear_service.clone(),
                    patch,
                    third_party_item,
                    for_user_id,
                )
                .await
            }
            ThirdPartyItemSourceKind::SlackReaction => {
                self.apply_updated_task_side_effect::<SlackReaction, SlackService>(
                    executor,
                    self.slack_service.clone(),
                    patch,
                    third_party_item,
                    for_user_id,
                )
                .await
            }
            ThirdPartyItemSourceKind::SlackStar => {
                self.apply_updated_task_side_effect::<SlackStar, SlackService>(
                    executor,
                    self.slack_service.clone(),
                    patch,
                    third_party_item,
                    for_user_id,
                )
                .await
            }
            kind => Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot apply task side effect for third party item source kind {kind}"
            ))),
        }
    }
}
