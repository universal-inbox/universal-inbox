use std::{collections::HashMap, fmt::Debug, sync::Arc};

use anyhow::anyhow;
use duplicate::duplicate_item;
use futures::stream::{self, StreamExt};
use uuid::Uuid;

use universal_inbox::{
    notification::Notification,
    task::{Task, TaskPatch, TaskStatus},
};

use crate::{
    integrations::{
        notification::NotificationSource,
        task::{TaskSourceService, TaskSyncSourceKind},
        todoist::TodoistService,
    },
    repository::{task::TaskRepository, ConnectedRepository, Repository, TransactionalRepository},
    universal_inbox::{
        notification::service::{
            ConnectedNotificationService, NotificationService, TransactionalNotificationService,
        },
        UniversalInboxError, UpdateStatus,
    },
};

use super::TaskCreationResult;

pub struct TaskService {
    repository: Arc<Repository>,
    #[allow(dead_code)]
    todoist_service: TodoistService,
    notification_service: Arc<NotificationService>,
}

impl TaskService {
    pub fn new(
        repository: Arc<Repository>,
        todoist_service: TodoistService,
        notification_service: Arc<NotificationService>,
    ) -> Result<TaskService, UniversalInboxError> {
        Ok(TaskService {
            repository,
            todoist_service,
            notification_service,
        })
    }

    pub async fn connect(&self) -> Result<Box<ConnectedTaskService>, UniversalInboxError> {
        let connected_repository = self.repository.connect().await?;
        let connected_notification_service = self
            .notification_service
            .connected_with(connected_repository.clone());
        Ok(Box::new(ConnectedTaskService {
            repository: connected_repository,
            notification_service: *connected_notification_service,
            service: self,
        }))
    }

    pub async fn begin(&self) -> Result<Box<TransactionalTaskService>, UniversalInboxError> {
        let transactional_repository = self.repository.begin().await?;
        let transactional_notification_service = self
            .notification_service
            .transactional_with(transactional_repository.clone());
        Ok(Box::new(TransactionalTaskService {
            repository: transactional_repository,
            notification_service: *transactional_notification_service,
            service: self,
        }))
    }
}

pub struct ConnectedTaskService<'a> {
    repository: Arc<ConnectedRepository>,
    notification_service: ConnectedNotificationService<'a>,
    #[allow(dead_code)]
    service: &'a TaskService,
}

pub struct TransactionalTaskService<'a> {
    pub repository: Arc<TransactionalRepository<'a>>,
    notification_service: TransactionalNotificationService<'a>,
    #[allow(dead_code)]
    service: &'a TaskService,
}

impl<'a> TransactionalTaskService<'a> {
    pub async fn commit(self) -> Result<(), UniversalInboxError> {
        drop(self.notification_service.repository);
        let repository = Arc::try_unwrap(self.repository)
            .map_err(|_| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Cannot extract repository to commit transaction it as it has other references using it"
                ))
            })?;

        repository.commit().await
    }
}

#[duplicate_item(task_service; [ConnectedTaskService]; [TransactionalTaskService];)]
impl<'a> task_service<'a> {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn list_tasks(&self, status: TaskStatus) -> Result<Vec<Task>, UniversalInboxError> {
        self.repository.fetch_all_tasks(status).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_task(&self, task_id: Uuid) -> Result<Option<Task>, UniversalInboxError> {
        self.repository.get_one_task(task_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_task(
        &self,
        task: Box<Task>,
    ) -> Result<Box<TaskCreationResult>, UniversalInboxError> {
        let task = self.repository.create_task(task).await?;
        let notification = if task.is_in_inbox() {
            Some(
                self.notification_service
                    .create_notification(Box::new(task.as_ref().into()))
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

    async fn sync_source_tasks_and_notifications<T: Debug, U>(
        &self,
        task_source_service: &U,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError>
    where
        U: TaskSourceService<T> + NotificationSource,
    {
        let source_tasks = task_source_service.fetch_all_tasks().await?;
        let tasks = self
            .save_tasks_from_source(task_source_service, &source_tasks)
            .await?;

        let tasks_in_inbox = tasks.iter().filter(|task| task.is_in_inbox()).collect();

        let notifications = self
            .notification_service
            .save_notifications_from_source(
                task_source_service.get_notification_source_kind(),
                tasks_in_inbox,
            )
            .await?;

        let mut notifications_by_task_id: HashMap<Option<Uuid>, Notification> = notifications
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
    pub async fn save_tasks_from_source<T: Debug>(
        &self,
        task_source_service: &dyn TaskSourceService<T>,
        source_tasks: &[T],
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let tasks = stream::iter(source_tasks)
            .then(|source_task| async {
                let task = task_source_service.build_task(source_task).await?;
                self.repository.create_or_update_task(task).await
            })
            .collect::<Vec<Result<Task, UniversalInboxError>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<Task>, UniversalInboxError>>()?;

        let source_task_ids = tasks
            .iter()
            .map(|task| task.source_id.clone())
            .collect::<Vec<String>>();

        self.repository
            .update_stale_tasks_status_from_source_ids(
                source_task_ids,
                task_source_service.get_task_source_kind(),
                TaskStatus::Done,
            )
            .await?;

        Ok(tasks)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_tasks(
        &self,
        source: &Option<TaskSyncSourceKind>,
    ) -> Result<Vec<TaskCreationResult>, UniversalInboxError> {
        match source {
            Some(TaskSyncSourceKind::Todoist) => {
                self.sync_source_tasks_and_notifications(&self.service.todoist_service)
                    .await
            }
            None => {
                let sync_result_from_todoist = self
                    .sync_source_tasks_and_notifications(&self.service.todoist_service)
                    .await?;
                // merge result with other integrations here
                Ok(sync_result_from_todoist)
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn patch_task(
        &self,
        task_id: Uuid,
        patch: &TaskPatch,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        let updated_task = self.repository.update_task(task_id, patch).await?;

        Ok(updated_task)
    }
}
