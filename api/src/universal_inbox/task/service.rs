use std::sync::Arc;

use anyhow::anyhow;
use duplicate::duplicate_item;
use futures::stream::{self, StreamExt};
use uuid::Uuid;

use crate::{
    integrations::{
        task::{SourceTask, TaskSourceService},
        todoist::TodoistService,
    },
    repository::task::{ConnectedTaskRepository, TaskRepository, TransactionalTaskRepository},
    universal_inbox::{UniversalInboxError, UpdateStatus},
};
use universal_inbox::task::{Task, TaskPatch, TaskStatus};

use super::source::TaskSourceKind;

pub struct TaskService {
    repository: Box<TaskRepository>,
    #[allow(dead_code)]
    todoist_service: TodoistService,
}

impl TaskService {
    pub fn new(
        repository: Box<TaskRepository>,
        todoist_service: TodoistService,
    ) -> Result<TaskService, UniversalInboxError> {
        Ok(TaskService {
            repository,
            todoist_service,
        })
    }

    pub async fn connect(&self) -> Result<Box<ConnectedTaskService>, UniversalInboxError> {
        Ok(Box::new(ConnectedTaskService {
            repository: self.repository.connect().await?,
            service: self,
        }))
    }

    pub async fn begin(&self) -> Result<Box<TransactionalTaskService>, UniversalInboxError> {
        Ok(Box::new(TransactionalTaskService {
            repository: self.repository.begin().await?,
            service: self,
        }))
    }
}

pub struct ConnectedTaskService<'a> {
    repository: Arc<ConnectedTaskRepository>,
    #[allow(dead_code)]
    service: &'a TaskService,
}

pub struct TransactionalTaskService<'a> {
    repository: Arc<TransactionalTaskRepository<'a>>,
    #[allow(dead_code)]
    service: &'a TaskService,
}

impl<'a> TransactionalTaskService<'a> {
    pub async fn commit(self) -> Result<(), UniversalInboxError> {
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
        self.repository.fetch_all(status).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_task(&self, task_id: Uuid) -> Result<Option<Task>, UniversalInboxError> {
        self.repository.get_one(task_id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create_task(&self, task: Box<Task>) -> Result<Box<Task>, UniversalInboxError> {
        self.repository.create(task).await
    }

    async fn sync_source_tasks<T: SourceTask>(
        &self,
        task_source_service: &dyn TaskSourceService<T>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let all_source_tasks = task_source_service.fetch_all_tasks().await?;

        let tasks = stream::iter(&all_source_tasks)
            .then(|source_task| async {
                let task = task_source_service.build_task(source_task).await?;
                self.repository.create_or_update(task).await
            })
            .collect::<Vec<Result<Task, UniversalInboxError>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<Task>, UniversalInboxError>>()?;

        let all_source_task_ids = all_source_tasks
            .into_iter()
            .map(|source_notif| source_notif.get_id())
            .collect::<Vec<String>>();

        self.repository
            .update_stale_tasks_status_from_source_ids(
                all_source_task_ids,
                task_source_service.get_task_source_kind(),
                TaskStatus::Done,
            )
            .await?;

        Ok(tasks)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_tasks(
        &self,
        source: &Option<TaskSourceKind>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        match source {
            Some(TaskSourceKind::Todoist) => {
                self.sync_source_tasks(&self.service.todoist_service).await
            }
            None => {
                let tasks_from_todoist = self
                    .sync_source_tasks(&self.service.todoist_service)
                    .await?;
                Ok(tasks_from_todoist)
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn patch_task(
        &self,
        task_id: Uuid,
        patch: &TaskPatch,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        let updated_task = self.repository.update(task_id, patch).await?;

        Ok(updated_task)
    }
}
