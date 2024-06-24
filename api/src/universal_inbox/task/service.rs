use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use universal_inbox::{
    integration_connection::provider::{IntegrationProvider, IntegrationProviderSource},
    notification::{
        service::NotificationPatch, Notification, NotificationSource, NotificationSourceKind,
        NotificationStatus,
    },
    task::{
        service::TaskPatch, ProjectSummary, Task, TaskCreation, TaskCreationResult, TaskId,
        TaskSource, TaskSourceKind, TaskStatus, TaskSummary, TaskSyncSourceKind,
    },
    third_party::item::{
        ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemFromSource,
        ThirdPartyItemSource, ThirdPartyItemSourceKind,
    },
    user::UserId,
    HasHtmlUrl,
};

use crate::{
    integrations::{
        linear::LinearService,
        slack::SlackService,
        task::{ThirdPartyTaskService, ThirdPartyTaskSourceService},
        third_party::ThirdPartyItemSourceService,
        todoist::TodoistService,
    },
    repository::{task::TaskRepository, Repository},
    universal_inbox::{
        integration_connection::service::{
            IntegrationConnectionService, IntegrationConnectionSyncType,
        },
        notification::service::NotificationService,
        third_party::service::ThirdPartyItemService,
        user::service::UserService,
        UniversalInboxError, UpdateStatus, UpsertStatus,
    },
};

pub struct TaskService {
    repository: Arc<Repository>,
    todoist_service: Arc<TodoistService>,
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

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_task_service, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    pub async fn apply_updated_task_side_effect<'a, T, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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
                third_party_task_service
                    .delete_task(executor, third_party_item, user_id)
                    .await?;
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
        skip(self, executor, synced_third_party_item, upsert_task),
        fields(
            third_party_item_id = synced_third_party_item.id.to_string(),
            third_party_item_source_id = synced_third_party_item.source_id,
            task_id = upsert_task.value_ref().id.to_string()
        ),
        err
    )]
    pub async fn apply_synced_task_side_effect<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
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
                    .create_sink_item_from_task(executor, task)
                    .await?;
            }
            UpsertStatus::Updated {
                new: new_task,
                old: old_task,
            } => {
                let task_source_item = &new_task.source_item;
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
                    project: (new_task.project != old_task.project)
                        .then(|| new_task.project.clone()),
                    due_at: (new_task.due_at != old_task.due_at).then(|| new_task.due_at.clone()),
                    priority: (new_task.priority != old_task.priority).then_some(new_task.priority),
                    body: (new_task.body != old_task.body).then(|| new_task.body.clone()),
                    sink_item_id: None,
                };

                match third_party_item_to_be_updated.get_third_party_item_source_kind() {
                    ThirdPartyItemSourceKind::Todoist => {
                        self.apply_updated_task_side_effect(
                            executor,
                            self.todoist_service.clone(),
                            &task_patch,
                            third_party_item_to_be_updated,
                            user_id,
                        )
                        .await?
                    }
                    ThirdPartyItemSourceKind::Slack => {
                        self.apply_updated_task_side_effect(
                            executor,
                            self.slack_service.clone(),
                            &task_patch,
                            third_party_item_to_be_updated,
                            user_id,
                        )
                        .await?
                    }
                    ThirdPartyItemSourceKind::Linear => {
                        self.apply_updated_task_side_effect(
                            executor,
                            self.linear_service.clone(),
                            &task_patch,
                            third_party_item_to_be_updated,
                            user_id,
                        )
                        .await?
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn list_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: TaskStatus,
        user_id: UserId,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        self.repository
            .fetch_all_tasks(executor, status, user_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn search_tasks<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        matches: &'b str,
        user_id: UserId,
    ) -> Result<Vec<TaskSummary>, UniversalInboxError> {
        self.repository
            .search_tasks(executor, matches, user_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn get_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        for_user_id: UserId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        let task = self.repository.get_one_task(executor, task_id).await?;

        if let Some(ref task) = task {
            if task.user_id != for_user_id {
                return Err(UniversalInboxError::Forbidden(format!(
                    "Only the owner of the task {task_id} can access it"
                )));
            }
        }

        Ok(task)
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn get_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        self.repository.get_tasks(executor, task_ids).await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, task), fields(task_id = task.id.to_string()))]
    pub async fn create_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: Box<Task>,
        for_user_id: UserId,
    ) -> Result<Box<TaskCreationResult>, UniversalInboxError> {
        if task.user_id != for_user_id {
            return Err(UniversalInboxError::Forbidden(format!(
                "A task can only be created for {for_user_id}"
            )));
        }

        let task = self.repository.create_task(executor, task).await?;
        let notification = if task.is_in_inbox() {
            Some(
                self.notification_service
                    .upgrade()
                    .context("Unable to access notification_service from task_service")?
                    .read()
                    .await
                    .create_notification(executor, Box::new((*task).clone().into()), for_user_id)
                    .await?,
            )
        } else {
            None
        };

        Ok(Box::new(TaskCreationResult {
            task: *task,
            notifications: notification.into_iter().map(|n| *n).collect(),
        }))
    }

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()))]
    pub async fn create_task_from_notification<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_creation: &'b TaskCreation,
        notification: &'b Notification,
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
        let source_id = third_party_item.source_id.clone();
        let created_third_party_item = self
            .third_party_item_service
            .upgrade()
            .context("Unable to access third_party_item_service from task_service")?
            .read()
            .await
            .create_item(executor, third_party_item, user_id)
            .await?;

        let Some(ThirdPartyItemCreationResult {
            task: Some(created_task),
            ..
        }) = created_third_party_item
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "A task should have been created from the {integration_provider_kind} task {source_id}"
            )));
        };

        Ok(Box::new(created_task))
    }

    #[tracing::instrument(level = "debug", skip(self, executor, third_party_task_service))]
    async fn sync_third_party_tasks<'a, T, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_task_service: Arc<U>,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T>
            + ThirdPartyItemSourceService
            + ThirdPartyItemSource
            + NotificationSource
            + TaskSource
            + Send
            + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let integration_provider_kind = third_party_task_service.get_integration_provider_kind();
        let integration_connection_service = self.integration_connection_service.read().await;
        let Some(integration_connection) = integration_connection_service
            .get_integration_connection_to_sync(
                executor,
                integration_provider_kind,
                self.min_sync_tasks_interval_in_minutes,
                IntegrationConnectionSyncType::Tasks,
                user_id,
            )
            .await?
        else {
            debug!("No validated {integration_provider_kind} integration found for user {user_id}, skipping tasks sync");
            return Ok(vec![]);
        };

        if !integration_connection.provider.is_sync_tasks_enabled() {
            debug!("{integration_provider_kind} integration for user {user_id} is disabled, skipping tasks sync");
            return Ok(vec![]);
        }

        info!("Syncing {integration_provider_kind} tasks for user {user_id}");
        integration_connection_service
            .start_tasks_sync_status(executor, integration_provider_kind, user_id)
            .await?;

        let third_party_item_creation_results = match self
            .third_party_item_service
            .upgrade()
            .context("Unable to access third_party_item_service from task_service")?
            .read()
            .await
            .sync_items(
                executor,
                third_party_task_service,
                &integration_connection.provider,
                user_id,
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
            Ok(third_party_item_creation_results) => {
                integration_connection_service
                    .complete_tasks_sync_status(executor, integration_provider_kind, user_id)
                    .await?;
                third_party_item_creation_results
            }
        };

        let mut tasks_creation_result = vec![];
        for third_party_item_creation_result in third_party_item_creation_results {
            if let Some(task) = third_party_item_creation_result.task {
                tasks_creation_result.push(TaskCreationResult {
                    task,
                    notifications: third_party_item_creation_result
                        .notification
                        .into_iter()
                        .collect(),
                });
            }
        }
        info!(
            "Successfully synced {} {integration_provider_kind} tasks for user {user_id}",
            tasks_creation_result.len()
        );

        Ok(tasks_creation_result)
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_task_service, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
    )]
    pub async fn sync_third_party_item_as_task<'a, T, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_task_service: Arc<U>,
        third_party_item: &ThirdPartyItem,
        task_creation: Option<TaskCreation>,
        user_id: UserId,
    ) -> Result<UpsertStatus<Box<Task>>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T> + ThirdPartyItemSource + TaskSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let mut upsert_task = self
            .save_third_party_item_as_task(
                executor,
                third_party_task_service,
                third_party_item,
                task_creation,
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
        skip(self, executor, third_party_task_service, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
    )]
    pub async fn save_third_party_item_as_task<'a, T, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_task_service: Arc<U>,
        third_party_item: &ThirdPartyItem,
        task_creation: Option<TaskCreation>,
        user_id: UserId,
    ) -> Result<UpsertStatus<Box<Task>>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T> + ThirdPartyItemSource + TaskSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let data: T = third_party_item.clone().try_into().map_err(|_| {
            anyhow!(
                "Unexpected third party item kind {}, expecting {}",
                third_party_item.kind(),
                third_party_task_service.get_third_party_item_source_kind()
            )
        })?;

        let task = third_party_task_service
            .third_party_item_into_task(executor, &data, third_party_item, task_creation, user_id)
            .await?;
        self.repository.create_or_update_task(executor, task).await
    }

    #[tracing::instrument(
        level = "debug",
        skip(
            self,
            executor,
            third_party_task_service,
            task,
            integration_connection_provider
        ),
        fields(task_id = task.id.to_string()),
    )]
    pub async fn save_task_as_notification<'a, T: Debug, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_task_service: Arc<U>,
        task: &Task,
        integration_connection_provider: &IntegrationProvider,
        is_incremental_update: bool,
        user_id: UserId,
    ) -> Result<Option<UpsertStatus<Box<Notification>>>, UniversalInboxError>
    where
        U: ThirdPartyTaskService<T> + NotificationSource + Send + Sync,
    {
        let existing_notifications = self
            .notification_service
            .upgrade()
            .context("Unable to access notification_service from task_service")?
            .read()
            .await
            .list_notifications(executor, vec![], true, Some(task.id), None, user_id)
            .await?
            // Considering the list of notifications for a task is small enough to fit in a single page
            .content;

        let notification_source_kind = third_party_task_service.get_notification_source_kind();

        if task.is_in_inbox() {
            if !integration_connection_provider.should_create_notification_from_inbox_task() {
                return Ok(None);
            }

            // Create notifications from tasks in the inbox if there is no existing notification
            // for this task or if there is an existing notification for the task with the same
            // source kind
            let task_has_a_notification_from_the_same_source = existing_notifications
                .iter()
                .any(|n| n.get_source_kind() == notification_source_kind);
            if !existing_notifications.is_empty() && !task_has_a_notification_from_the_same_source {
                return Ok(None);
            }

            debug!(
                "Creating notification from {} task {}",
                notification_source_kind, task.id
            );
            let notification_from_task = task.clone().into();
            // Create/update notifications for tasks in the Inbox
            return self
                .notification_service
                .upgrade()
                .context("Unable to access notification_service from task_service")?
                .read()
                .await
                .save_notifications_from_source(
                    executor,
                    notification_source_kind,
                    vec![notification_from_task],
                    is_incremental_update,
                    third_party_task_service.is_supporting_snoozed_notifications(),
                    user_id,
                )
                .await
                .map(|mut notifications| notifications.pop());
        }

        // Update existing notifications for a task that is not in the Inbox anymore
        let mut updated_notifications = self
            .notification_service
            .upgrade()
            .context("Unable to access notification_service from task_service")?
            .read()
            .await
            .patch_notifications_for_task(
                executor,
                task.id,
                Some(notification_source_kind),
                &NotificationPatch {
                    status: Some(NotificationStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?;
        debug!(
            "{} {} notifications deleted for task {}",
            updated_notifications.len(),
            notification_source_kind,
            task.id
        );

        updated_notifications
            .pop()
            .map(|update_status| {
                Ok::<UpsertStatus<Box<Notification>>, UniversalInboxError>({
                    let notification =
                        Box::new(update_status.result.clone().ok_or_else(|| {
                            anyhow!("Expected a notification from the UpdateStatus")
                        })?);
                    if update_status.updated {
                        // the `old` value is wrong here, but we don't need it
                        UpsertStatus::Updated {
                            new: notification.clone(),
                            old: notification,
                        }
                    } else {
                        UpsertStatus::Untouched(notification)
                    }
                })
            })
            .transpose()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn sync_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source: TaskSyncSourceKind,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        match source {
            TaskSyncSourceKind::Todoist => {
                self.sync_third_party_tasks(executor, self.todoist_service.clone(), user_id)
                    .await
            }
            TaskSyncSourceKind::Linear => {
                self.sync_third_party_tasks(executor, self.linear_service.clone(), user_id)
                    .await
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_tasks_with_transaction<'a>(
        &self,
        source: TaskSyncSourceKind,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        let mut transaction = self.begin().await.context(format!(
            "Failed to create new transaction while syncing {source:?}"
        ))?;

        match self.sync_tasks(&mut transaction, source, user_id).await {
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

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_all_tasks<'a>(
        &self,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        let sync_result_from_todoist = self
            .sync_tasks_with_transaction(TaskSyncSourceKind::Todoist, user_id)
            .await?;
        let sync_result_from_linear = self
            .sync_tasks_with_transaction(TaskSyncSourceKind::Linear, user_id)
            .await?;
        Ok(sync_result_from_todoist
            .into_iter()
            .chain(sync_result_from_linear.into_iter())
            .collect())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_tasks_for_all_users<'a>(
        &self,
        source: Option<TaskSyncSourceKind>,
    ) -> Result<(), UniversalInboxError> {
        let service = self.user_service.clone();
        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while syncing tasks for all users")?;
        let users = service.fetch_all_users(&mut transaction).await?;

        for user in users {
            let _ = self.sync_tasks_for_user(source, user.id).await;
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_tasks_for_user<'a>(
        &self,
        source: Option<TaskSyncSourceKind>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        info!("Syncing tasks for user {user_id}");

        let sync_result = if let Some(source) = source {
            self.sync_tasks_with_transaction(source, user_id).await
        } else {
            self.sync_all_tasks(user_id).await
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn patch_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        patch: &TaskPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        let updated_task = self
            .repository
            .update_task(executor, task_id, patch, for_user_id)
            .await?;

        match updated_task {
            UpdateStatus {
                updated: true,
                result: Some(ref task),
            } =>
            {
                #[allow(clippy::single_match)]
                match task.kind {
                    TaskSourceKind::Todoist => {
                        if patch.status == Some(TaskStatus::Deleted)
                            || patch.status == Some(TaskStatus::Done)
                            || (patch.project.is_some() && !task.is_in_inbox())
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
                                .patch_notifications_for_task(
                                    executor,
                                    task.id,
                                    Some(NotificationSourceKind::Todoist),
                                    &notification_patch,
                                )
                                .await?;
                        }

                        self.apply_updated_task_side_effect(
                            executor,
                            self.todoist_service.clone(),
                            patch,
                            &task.source_item,
                            for_user_id,
                        )
                        .await?;
                    }
                    _ => {}
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

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()))]
    pub async fn link_notification_with_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &'b Notification,
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
    pub async fn search_projects<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        matches: &'b str,
        user_id: UserId,
    ) -> Result<Vec<ProjectSummary>, UniversalInboxError> {
        self.todoist_service
            .search_projects(executor, matches, user_id)
            .await
    }

    pub async fn get_or_create_project<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        project_name: &'b str,
        user_id: UserId,
    ) -> Result<ProjectSummary, UniversalInboxError> {
        self.todoist_service
            .get_or_create_project(executor, project_name, user_id, None)
            .await
    }
}
