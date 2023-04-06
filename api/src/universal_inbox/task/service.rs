use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    notification::{Notification, NotificationPatch, NotificationStatus},
    task::{Task, TaskCreation, TaskId, TaskMetadata, TaskPatch, TaskStatus, TaskSummary},
    user::UserId,
};

use crate::{
    integrations::{
        notification::{NotificationSource, NotificationSourceKind},
        task::{TaskSourceService, TaskSyncSourceKind},
        todoist::TodoistService,
    },
    repository::{task::TaskRepository, Repository},
    universal_inbox::{
        notification::service::NotificationService, UniversalInboxError, UpdateStatus,
    },
};

use super::TaskCreationResult;

#[derive(Debug)]
pub struct TaskService {
    repository: Arc<Repository>,
    todoist_service: TodoistService,
    notification_service: Weak<RwLock<NotificationService>>,
}

impl TaskService {
    pub fn new(
        repository: Arc<Repository>,
        todoist_service: TodoistService,
        notification_service: Weak<RwLock<NotificationService>>,
    ) -> TaskService {
        TaskService {
            repository,
            todoist_service,
            notification_service,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(task_source_service))]
    pub async fn apply_updated_task_side_effect<T>(
        task_source_service: &dyn TaskSourceService<T>,
        patch: &TaskPatch,
        task: Box<Task>,
    ) -> Result<(), UniversalInboxError> {
        match patch.status {
            Some(TaskStatus::Deleted) => {
                task_source_service.delete_task(&task.source_id).await?;
            }
            Some(TaskStatus::Done) => {
                task_source_service.complete_task(&task.source_id).await?;
            }
            _ => (),
        }

        task_source_service
            .update_task(&task.source_id, patch)
            .await?;

        Ok(())
    }
}

impl TaskService {
    #[tracing::instrument(level = "debug", skip(self))]
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

    #[tracing::instrument(level = "debug", skip(self))]
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

    #[tracing::instrument(level = "debug", skip(self))]
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

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        self.repository.get_tasks(executor, task_ids).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
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
            notification: notification.map(|n| *n),
        }))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_task_from_notification<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_creation: &'b TaskCreation,
        notification: &'b Notification,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let source_task = self
            .todoist_service
            .create_task(&TaskCreation {
                body: Some(format!(
                    "- [{}]({})",
                    notification.title,
                    notification
                        .source_html_url
                        .as_ref()
                        .map(|url| url.to_string())
                        .unwrap_or_default()
                )),
                ..(*task_creation).clone()
            })
            .await?;
        let task = self
            .todoist_service
            .build_task(&source_task, notification.user_id)
            .await?;
        let created_task = self.repository.create_task(executor, task).await?;

        Ok(created_task)
    }

    async fn sync_source_tasks_and_notifications<'a, T: Debug, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_source_service: &U,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError>
    where
        U: TaskSourceService<T> + NotificationSource,
    {
        let source_tasks = task_source_service.fetch_all_tasks().await?;
        let tasks = self
            .save_tasks_from_source(executor, task_source_service, &source_tasks, user_id)
            .await?;

        let tasks_in_inbox: Vec<Task> = tasks
            .iter()
            .filter(|task| task.is_in_inbox())
            .cloned()
            .collect();

        let notifications = self
            .notification_service
            .upgrade()
            .context("Unable to access notification_service from task_service")?
            .read()
            .await
            .save_notifications_from_source(
                executor,
                task_source_service.get_notification_source_kind(),
                tasks_in_inbox,
                user_id,
            )
            .await?;

        let mut notifications_by_task_id: HashMap<Option<TaskId>, Notification> = notifications
            .into_iter()
            .map(|notification| (notification.task_id, notification))
            .collect();

        Ok(tasks
            .into_iter()
            .map(move |task| {
                let task_id = task.id;
                TaskCreationResult {
                    task,
                    notification: notifications_by_task_id.remove(&Some(task_id)),
                }
            })
            .collect())
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

    #[tracing::instrument(level = "debug", skip(self, task_source_service))]
    pub async fn save_tasks_from_source<'a, T: Debug>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_source_service: &dyn TaskSourceService<T>,
        source_tasks: &[T],
        user_id: UserId,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let mut tasks = vec![];
        for source_task in source_tasks {
            let task = task_source_service.build_task(source_task, user_id).await?;
            let uptodate_task = self
                .repository
                .create_or_update_task(executor, task)
                .await?;
            tasks.push(uptodate_task);
        }

        let source_task_ids = tasks
            .iter()
            .map(|task| task.source_id.clone())
            .collect::<Vec<String>>();

        self.repository
            .update_stale_tasks_status_from_source_ids(
                executor,
                source_task_ids,
                task_source_service.get_task_source_kind(),
                TaskStatus::Done,
            )
            .await?;

        Ok(tasks)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source: &Option<TaskSyncSourceKind>,
        user_id: UserId,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        match source {
            Some(TaskSyncSourceKind::Todoist) => {
                self.sync_source_tasks_and_notifications(executor, &self.todoist_service, user_id)
                    .await
            }
            None => {
                let sync_result_from_todoist = self
                    .sync_source_tasks_and_notifications(executor, &self.todoist_service, user_id)
                    .await?;
                // merge result with other integrations here
                Ok(sync_result_from_todoist)
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
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

                    TaskService::apply_updated_task_side_effect(
                        &self.todoist_service,
                        patch,
                        task.clone(),
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

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn associate_notification_with_task<'a, 'b>(
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
                    "Cannot associate notification {} with unknown task {task_id}",
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
                        notification
                            .source_html_url
                            .as_ref()
                            .map(|url| url.to_string())
                            .unwrap_or_default()
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
                "Cannot update task {task_id} body while associating notification {} to it",
                notification.id
            )))
        }
    }
}
