use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::Context;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    notification::{Notification, NotificationPatch, NotificationStatus},
    task::{Task, TaskId, TaskMetadata, TaskPatch, TaskStatus},
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
                task_source_service
                    .delete_task_from_source(&task.source_id)
                    .await
            }
            Some(TaskStatus::Done) => {
                task_source_service
                    .complete_task_from_source(&task.source_id)
                    .await
            }
            _ => Ok(()),
        }
    }
}

impl TaskService {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn list_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: TaskStatus,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        self.repository.fetch_all_tasks(executor, status).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        self.repository.get_one_task(executor, task_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: Box<Task>,
    ) -> Result<Box<TaskCreationResult>, UniversalInboxError> {
        let task = self.repository.create_task(executor, task).await?;
        let notification = if task.is_in_inbox() {
            Some(
                self.notification_service
                    .upgrade()
                    .context("Unable to access notification_service from task_service")?
                    .read()
                    .await
                    .create_notification(executor, Box::new(task.as_ref().into()))
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

    async fn sync_source_tasks_and_notifications<'a, T: Debug, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_source_service: &U,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError>
    where
        U: TaskSourceService<T> + NotificationSource,
    {
        let source_tasks = task_source_service.fetch_all_tasks().await?;
        let tasks = self
            .save_tasks_from_source(executor, task_source_service, &source_tasks)
            .await?;

        let tasks_in_inbox = tasks.iter().filter(|task| task.is_in_inbox()).collect();

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
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let mut tasks = vec![];
        for source_task in source_tasks {
            let task = task_source_service.build_task(source_task).await?;
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
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        match source {
            Some(TaskSyncSourceKind::Todoist) => {
                self.sync_source_tasks_and_notifications(executor, &self.todoist_service)
                    .await
            }
            None => {
                let sync_result_from_todoist = self
                    .sync_source_tasks_and_notifications(executor, &self.todoist_service)
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
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        let updated_task = self
            .repository
            .update_task(executor, task_id, patch)
            .await?;

        if let UpdateStatus {
            updated: true,
            result: Some(ref task),
        } = updated_task
        {
            match task.metadata {
                TaskMetadata::Todoist(_) => {
                    if patch.status == Some(TaskStatus::Deleted)
                        || patch.status == Some(TaskStatus::Done)
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
            }
        }

        Ok(updated_task)
    }
}
