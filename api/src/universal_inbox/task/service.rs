use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use chrono::{TimeDelta, Utc};
use slack_morphism::prelude::{SlackEventCallbackBody, SlackPushEventCallback};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use universal_inbox::{
    notification::{
        service::NotificationPatch, Notification, NotificationMetadata, NotificationSource,
        NotificationSourceKind, NotificationStatus,
    },
    task::{
        service::TaskPatch, ProjectSummary, Task, TaskCreation, TaskCreationResult, TaskId,
        TaskMetadata, TaskStatus, TaskSummary, TaskSyncSourceKind,
    },
    user::UserId,
    HasHtmlUrl,
};

use crate::{
    integrations::{
        notification::NotificationSourceService, slack::SlackService, task::TaskSourceService,
        todoist::TodoistService,
    },
    repository::{task::TaskRepository, Repository},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, user::service::UserService,
        UniversalInboxError, UpdateStatus, UpsertStatus,
    },
};

pub struct TaskService {
    repository: Arc<Repository>,
    todoist_service: TodoistService,
    notification_service: Weak<RwLock<NotificationService>>,
    slack_service: SlackService,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    user_service: Arc<RwLock<UserService>>,
    min_sync_tasks_interval_in_minutes: i64,
}

impl TaskService {
    pub fn new(
        repository: Arc<Repository>,
        todoist_service: TodoistService,
        notification_service: Weak<RwLock<NotificationService>>,
        slack_service: SlackService,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        user_service: Arc<RwLock<UserService>>,
        min_sync_tasks_interval_in_minutes: i64,
    ) -> TaskService {
        TaskService {
            repository,
            todoist_service,
            notification_service,
            slack_service,
            integration_connection_service,
            user_service,
            min_sync_tasks_interval_in_minutes,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, task_source_service, task), fields(task_id = task.id.to_string()), err)]
    pub async fn apply_updated_task_side_effect<'a, T>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_source_service: &(dyn TaskSourceService<T> + Send + Sync),
        patch: &TaskPatch,
        task: Box<Task>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        match patch.status {
            Some(TaskStatus::Deleted) => {
                task_source_service
                    .delete_task(executor, &task.source_id, user_id)
                    .await?;
            }
            Some(TaskStatus::Done) => {
                task_source_service
                    .complete_task(executor, &task.source_id, user_id)
                    .await?;
            }
            Some(TaskStatus::Active) => {
                task_source_service
                    .uncomplete_task(executor, &task.source_id, user_id)
                    .await?;
            }
            _ => (),
        }

        task_source_service
            .update_task(executor, &task.source_id, patch, user_id)
            .await?;

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, task, notifications),
        fields(
            task_id = task.id.to_string(),
            notification_ids = notifications.iter().map(|n| n.id.to_string()).collect::<Vec<String>>().join(", ")
        ),
        err
    )]
    pub async fn apply_synced_task_side_effect<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: &Task,
        notifications: Vec<Notification>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        for notification in notifications {
            debug!(
                "Applying side effect for task {} and notification {}",
                task.id, notification.id
            );
            match &notification.metadata {
                NotificationMetadata::Slack(box SlackPushEventCallback {
                    event: SlackEventCallbackBody::StarAdded(_),
                    ..
                })
                | NotificationMetadata::Slack(box SlackPushEventCallback {
                    event: SlackEventCallbackBody::StarRemoved(_),
                    ..
                }) => {
                    if task.status == TaskStatus::Done {
                        self.slack_service
                            .delete_notification_from_source(executor, &notification, user_id)
                            .await?;
                    } else if task.status == TaskStatus::Active {
                        self.slack_service
                            .undelete_notification_from_source(executor, &notification, user_id)
                            .await?;
                    }
                }

                _ => {}
            }
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
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

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
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

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
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

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn get_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        self.repository.get_tasks(executor, task_ids).await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, task), fields(task_id = task.id.to_string()), err)]
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

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()), err)]
    pub async fn create_task_from_notification<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_creation: &'b TaskCreation,
        notification: &'b Notification,
        for_user_id: UserId,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let source_task = self
            .todoist_service
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
                for_user_id,
            )
            .await?;
        let task = self
            .todoist_service
            .build_task(executor, &source_task, notification.user_id)
            .await?;
        let created_task = self.repository.create_task(executor, task).await?;

        Ok(created_task)
    }

    #[tracing::instrument(level = "debug", skip(self, executor, task_source_service), err)]
    async fn sync_source_tasks_and_notifications<'a, T: Debug, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_source_service: &U,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError>
    where
        U: TaskSourceService<T> + NotificationSource + Send + Sync,
    {
        let integration_provider_kind = task_source_service.get_integration_provider_kind();
        let result = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(
                executor,
                integration_provider_kind,
                Some(
                    Utc::now()
                        - TimeDelta::try_minutes(self.min_sync_tasks_interval_in_minutes)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Invalid `min_sync_tasks_interval_in_minutes` value: {}",
                                    self.min_sync_tasks_interval_in_minutes
                                )
                            }),
                ),
                user_id,
            )
            .await?;

        let Some((_, integration_connection)) = result else {
            debug!("No validated {integration_provider_kind} integration found for user {user_id}, skipping tasks sync.");
            return Ok(vec![]);
        };

        if !integration_connection.provider.is_sync_tasks_enabled() {
            debug!("{integration_provider_kind} integration for user {user_id} is disabled, skipping tasks sync.");
            return Ok(vec![]);
        }

        self.integration_connection_service
            .read()
            .await
            .update_integration_connection_sync_status(
                executor,
                user_id,
                integration_provider_kind,
                None,
                true,
            )
            .await?;
        match task_source_service.fetch_all_tasks(executor, user_id).await {
            Ok(source_tasks) => {
                let upsert_tasks = self
                    .save_tasks_from_source(
                        executor,
                        task_source_service,
                        &source_tasks,
                        true,
                        user_id,
                    )
                    .await?;

                let created_or_updated_tasks: Vec<Task> = upsert_tasks
                    .iter()
                    .filter_map(|upsert_task| match upsert_task {
                        UpsertStatus::Created(task) | UpsertStatus::Updated(task) => {
                            Some((**task).clone())
                        }
                        _ => None,
                    })
                    .collect();

                let mut notifications_by_task_id: HashMap<TaskId, Vec<Notification>> =
                    HashMap::new();
                for task in &created_or_updated_tasks {
                    let notifications = self
                        .notification_service
                        .upgrade()
                        .context("Unable to access notification_service from task_service")?
                        .read()
                        .await
                        .list_notifications(executor, vec![], true, Some(task.id), None, user_id)
                        .await?;

                    notifications_by_task_id.insert(
                        task.id,
                        notifications
                            .content
                            .into_iter()
                            .map(|notification_with_task| notification_with_task.into())
                            .collect(),
                    );
                }

                let (tasks_in_inbox, tasks_not_in_inbox): (Vec<Task>, Vec<Task>) =
                    created_or_updated_tasks
                        .into_iter()
                        .partition(|task| task.is_in_inbox());

                if integration_connection
                    .provider
                    .should_create_notification_from_inbox_task()
                {
                    let notification_source_kind =
                        task_source_service.get_notification_source_kind();
                    // Create notifications from tasks in the inbox if there is no existing notification
                    // for this task or if there is an existing notification with the same source kind
                    let notifications_from_tasks: Vec<Notification> = tasks_in_inbox
                        .into_iter()
                        .filter_map(|task| {
                            if let Some(existing_notifications) =
                                notifications_by_task_id.get(&task.id)
                            {
                                if !existing_notifications.is_empty()
                                    && existing_notifications
                                        .iter()
                                        .all(|n| n.get_source_kind() != notification_source_kind)
                                {
                                    return None;
                                }
                            }
                            Some(task.into_notification(user_id))
                        })
                        .collect();

                    if !notifications_from_tasks.is_empty() {
                        // Create/update notifications for tasks in the Inbox
                        let upsert_inbox_notifications = self
                            .notification_service
                            .upgrade()
                            .context("Unable to access notification_service from task_service")?
                            .read()
                            .await
                            .save_notifications_from_source(
                                executor,
                                notification_source_kind,
                                notifications_from_tasks,
                                true,
                                task_source_service.is_supporting_snoozed_notifications(),
                                user_id,
                            )
                            .await?;

                        for upsert_notification in upsert_inbox_notifications.into_iter() {
                            let notification = upsert_notification.value();
                            if let Some(task_id) = notification.task_id {
                                let notifications_for_task =
                                    notifications_by_task_id.entry(task_id).or_default();
                                if let Some(index) = notifications_for_task
                                    .iter()
                                    .position(|n| n.id == notification.id)
                                {
                                    notifications_for_task[index] = *notification;
                                } else {
                                    notifications_for_task.push(*notification);
                                }
                            }
                        }
                    }
                }

                // Update existing notifications for tasks that are not in the Inbox anymore
                for task in tasks_not_in_inbox {
                    let updated_notifications = self
                        .notification_service
                        .upgrade()
                        .context("Unable to access notification_service from task_service")?
                        .read()
                        .await
                        .patch_notifications_for_task(
                            executor,
                            task.id,
                            Some(task_source_service.get_notification_source_kind()),
                            &NotificationPatch {
                                status: Some(NotificationStatus::Deleted),
                                ..Default::default()
                            },
                        )
                        .await?;
                    for updated_notification in updated_notifications.into_iter() {
                        let notifications_for_task =
                            notifications_by_task_id.entry(task.id).or_default();
                        if updated_notification.updated {
                            if let Some(notification) = updated_notification.result {
                                if let Some(index) = notifications_for_task
                                    .iter()
                                    .position(|n| n.id == notification.id)
                                {
                                    notifications_for_task[index] = notification;
                                }
                            }
                        }
                    }
                }

                let mut tasks_creation_result = vec![];
                for upsert_task in upsert_tasks {
                    let task = upsert_task.value();
                    let notifications = notifications_by_task_id.remove(&task.id);

                    if let Some(notifications) = &notifications {
                        self.apply_synced_task_side_effect(
                            executor,
                            &task,
                            notifications.to_vec(),
                            user_id,
                        )
                        .await?;
                    }

                    tasks_creation_result.push(TaskCreationResult {
                        task: *task,
                        notifications: notifications.unwrap_or_default(),
                    });
                }

                Ok(tasks_creation_result)
            }
            Err(e) => {
                self.integration_connection_service
                    .read()
                    .await
                    .update_integration_connection_sync_status(
                        executor,
                        user_id,
                        integration_provider_kind,
                        Some(format!(
                            "Failed to fetch tasks from {integration_provider_kind}"
                        )),
                        false,
                    )
                    .await?;
                Err(UniversalInboxError::Recoverable(e.into()))
            }
        }
    }

    // To be used for tasks services without notifications from tasks
    // async fn sync_source_tasks<T>(
    //     &self,
    //     task_source_service: &dyn TaskSourceService<T>,
    // ) -> Result<Vec<Task>, UniversalInboxError> {
    //     let source_tasks = task_source_service.fetch_all_tasks().await?;
    //     self.save_tasks_from_source(task_source_service, &source_tasks)
    //         .await
    // }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, task_source_service, source_tasks)
    )]
    pub async fn save_tasks_from_source<'a, T: Debug>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_source_service: &(dyn TaskSourceService<T> + Send + Sync),
        source_tasks: &[T],
        is_incremental_update: bool,
        user_id: UserId,
    ) -> Result<Vec<UpsertStatus<Box<Task>>>, UniversalInboxError> {
        let mut upsert_tasks = vec![];
        for source_task in source_tasks {
            let task = task_source_service
                .build_task(executor, source_task, user_id)
                .await?;
            let upsert_task = self
                .repository
                .create_or_update_task(executor, task)
                .await?;
            upsert_tasks.push(upsert_task);
        }
        info!(
            "{} Todoist tasks successfully synced for user {user_id}.",
            upsert_tasks.len()
        );

        // For incremental synchronization tasks services, there is no need to update stale tasks
        // Not yet used as Todoist is incremental
        if !is_incremental_update {
            let source_task_ids = upsert_tasks
                .iter()
                .map(|upsert_task| upsert_task.value_ref().source_id.clone())
                .collect::<Vec<String>>();

            self.repository
                .update_stale_tasks_status_from_source_ids(
                    executor,
                    source_task_ids,
                    task_source_service.get_task_source_kind(),
                    TaskStatus::Done,
                    user_id,
                )
                .await?;
        }

        Ok(upsert_tasks)
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn sync_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source: TaskSyncSourceKind,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        match source {
            TaskSyncSourceKind::Todoist => {
                self.sync_source_tasks_and_notifications(executor, &self.todoist_service, user_id)
                    .await
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
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

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn sync_all_tasks<'a>(
        &self,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        let sync_result_from_todoist = self
            .sync_tasks_with_transaction(TaskSyncSourceKind::Todoist, user_id)
            .await?;
        Ok(sync_result_from_todoist)
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn sync_tasks_for_all_users<'a>(
        &self,
        source: Option<TaskSyncSourceKind>,
    ) -> Result<(), UniversalInboxError> {
        let service = self.user_service.read().await;
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

    #[tracing::instrument(level = "debug", skip(self), err)]
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

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
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
            } => match task.metadata {
                TaskMetadata::Todoist(_) => {
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
                        &self.todoist_service,
                        patch,
                        task.clone(),
                        for_user_id,
                    )
                    .await?;
                }
            },
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

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()), err)]
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

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
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

    #[tracing::instrument(level = "debug", skip(self), err)]
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
