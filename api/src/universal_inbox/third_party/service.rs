use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::debug;

use universal_inbox::{
    integration_connection::provider::{IntegrationProvider, IntegrationProviderSource},
    notification::NotificationSource,
    task::{service::TaskPatch, Task, TaskCreation, TaskSource},
    third_party::item::{
        ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemFromSource,
        ThirdPartyItemSource, ThirdPartyItemSourceKind,
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
    repository::{third_party::ThirdPartyItemRepository, Repository},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, task::service::TaskService,
        UniversalInboxError, UpsertStatus,
    },
};

pub struct ThirdPartyItemService {
    repository: Arc<Repository>,
    task_service: Weak<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    todoist_service: Arc<TodoistService>,
    slack_service: Arc<SlackService>,
    linear_service: Arc<LinearService>,
}

impl ThirdPartyItemService {
    pub fn new(
        repository: Arc<Repository>,
        task_service: Weak<RwLock<TaskService>>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        todoist_service: Arc<TodoistService>,
        slack_service: Arc<SlackService>,
        linear_service: Arc<LinearService>,
    ) -> Self {
        Self {
            repository,
            task_service,
            integration_connection_service,
            todoist_service,
            slack_service,
            linear_service,
        }
    }

    pub fn set_task_service(&mut self, task_service: Weak<RwLock<TaskService>>) {
        self.task_service = task_service;
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(
        level = "debug",
        skip(
            self,
            executor,
            third_party_task_service,
            integration_connection_provider
        ),
        err
    )]
    pub async fn sync_items<'a, T, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_task_service: Arc<U>,
        integration_connection_provider: &IntegrationProvider,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItemCreationResult>, UniversalInboxError>
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
        let kind = third_party_task_service.get_third_party_item_source_kind();
        let items = third_party_task_service
            .fetch_items(executor, user_id)
            .await?;
        let mut creations_result = vec![];

        debug!("Syncing {kind} third party items for user {user_id}");
        for item in items.into_iter() {
            let task_creation = integration_connection_provider.get_task_creation_default_values();

            let creation_result = self
                .sync_item(
                    executor,
                    third_party_task_service.clone(),
                    item,
                    integration_connection_provider,
                    task_creation,
                    user_id,
                )
                .await?;

            if let Some(creation_result) = creation_result {
                creations_result.push(creation_result);
            }
        }
        debug!(
            "Successfully synced {} third party items for user {user_id}",
            creations_result.len()
        );

        if !third_party_task_service.is_sync_incremental() {
            let active_task_source_third_party_item_ids = creations_result
                .iter()
                .map(|r| r.third_party_item.id)
                .collect();

            let third_party_items_to_mark_as_done = self
                .repository
                .get_stale_task_source_third_party_items(
                    executor,
                    active_task_source_third_party_item_ids,
                    third_party_task_service.get_task_source_kind(),
                    user_id,
                )
                .await?;

            let third_party_items_to_mark_as_done_count = third_party_items_to_mark_as_done.len();
            for item in third_party_items_to_mark_as_done.into_iter() {
                let task_creation =
                    integration_connection_provider.get_task_creation_default_values();

                let creation_result = self
                    .sync_item(
                        executor,
                        third_party_task_service.clone(),
                        item.marked_as_done(),
                        integration_connection_provider,
                        task_creation,
                        user_id,
                    )
                    .await?;

                if let Some(creation_result) = creation_result {
                    creations_result.push(creation_result);
                }
            }
            debug!(
                "Marked {third_party_items_to_mark_as_done_count} {kind} stale third party items as done"
            )
        }

        Ok(creations_result)
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    pub async fn create_item<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Option<ThirdPartyItemCreationResult>, UniversalInboxError> {
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
            debug!("No validated {integration_provider_kind} integration found for user {user_id}, cannot create third party item");
            return Ok(None);
        };

        let task_creation = integration_connection
            .provider
            .get_task_creation_default_values();
        match third_party_item.get_third_party_item_source_kind() {
            ThirdPartyItemSourceKind::Todoist => {
                self.sync_item(
                    executor,
                    self.todoist_service.clone(),
                    third_party_item,
                    &integration_connection.provider,
                    task_creation,
                    user_id,
                )
                .await
            }
            ThirdPartyItemSourceKind::Slack => {
                self.sync_item(
                    executor,
                    self.slack_service.clone(),
                    third_party_item,
                    &integration_connection.provider,
                    task_creation,
                    user_id,
                )
                .await
            }
            ThirdPartyItemSourceKind::Linear => {
                self.sync_item(
                    executor,
                    self.linear_service.clone(),
                    third_party_item,
                    &integration_connection.provider,
                    task_creation,
                    user_id,
                )
                .await
            }
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_task_service, third_party_item, integration_connection_provider),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    pub async fn sync_item<'a, T, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_task_service: Arc<U>,
        third_party_item: ThirdPartyItem,
        integration_connection_provider: &IntegrationProvider,
        task_creation: Option<TaskCreation>,
        user_id: UserId,
    ) -> Result<Option<ThirdPartyItemCreationResult>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyTaskService<T>
            + ThirdPartyItemSource
            + NotificationSource
            + TaskSource
            + Send
            + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let upsert_item = self
            .save_third_party_item(executor, third_party_item)
            .await?;

        let third_party_item_id = upsert_item.value_ref().id;
        let Some(third_party_item) = upsert_item.modified_value() else {
            debug!("Third party item {third_party_item_id} is already up to date");
            return Ok(None);
        };

        let upsert_task = self
            .task_service
            .upgrade()
            .context("Unable to access task_service from third_party_item_service")?
            .read()
            .await
            .sync_third_party_item_as_task(
                executor,
                third_party_task_service.clone(),
                &third_party_item,
                task_creation,
                user_id,
            )
            .await?;

        let task_id = upsert_task.value_ref().id;
        let Some(task) = upsert_task.modified_value() else {
            debug!(
                "Task {task_id} for third party item {third_party_item_id} is already up to date"
            );
            return Ok(Some(ThirdPartyItemCreationResult {
                third_party_item: *third_party_item,
                task: None,
                notification: None,
            }));
        };

        let upsert_notification = self
            .task_service
            .upgrade()
            .context("Unable to access task_service from third_party_item_service")?
            .read()
            .await
            .save_task_as_notification(
                executor,
                third_party_task_service,
                &task,
                integration_connection_provider,
                true, // Force incremental here to avoid deleting all other notification for this third party item kind
                user_id,
            )
            .await?;

        let Some(upsert_notification) = upsert_notification else {
            return Ok(Some(ThirdPartyItemCreationResult {
                third_party_item: *third_party_item,
                task: Some(*task),
                notification: None,
            }));
        };

        let notification_id = upsert_notification.value_ref().id;
        let notification_modified_value = upsert_notification.modified_value();
        if notification_modified_value.is_none() {
            debug!("Notification {notification_id} for task {task_id} is already up to date");
        }

        Ok(Some(ThirdPartyItemCreationResult {
            third_party_item: *third_party_item,
            task: Some(*task),
            notification: notification_modified_value.map(|n| *n),
        }))
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    pub async fn save_third_party_item(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: ThirdPartyItem,
    ) -> Result<UpsertStatus<Box<ThirdPartyItem>>, UniversalInboxError> {
        self.repository
            .create_or_update_third_party_item(executor, Box::new(third_party_item))
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, executor, task),
        fields(task_id = task.id.to_string()),
        err
    )]
    pub async fn create_sink_item_from_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: &'b mut Task,
    ) -> Result<Box<ThirdPartyItem>, UniversalInboxError> {
        let user_id = task.user_id;
        let third_party_task_service = self.todoist_service.clone(); // Shortcut as only Todoist is supported for now as a sink
        let integration_provider_kind = third_party_task_service.get_integration_provider_kind();
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, integration_provider_kind, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot create a sink item without an access token for {integration_provider_kind}"))?;

        let project = third_party_task_service
            .get_or_create_project(executor, &task.project, user_id, Some(&access_token))
            .await?;
        let task_creation = TaskCreation {
            title: task.title.clone(),
            body: Some(task.body.clone()),
            project,
            due_at: task.due_at.clone(),
            priority: task.priority,
        };

        if let Some(sink_item) = &task.sink_item {
            debug!(
                "Task {} already has a {} sink item for {}, returning it",
                task.id,
                sink_item.kind(),
                sink_item.source_id
            );
            return Ok(Box::new(sink_item.clone()));
        };

        let third_party_task = third_party_task_service
            .create_task(executor, &task_creation, user_id)
            .await?;

        let sink_third_party_item =
            third_party_task.into_third_party_item(user_id, integration_connection.id);
        let upsert_item = self
            .save_third_party_item(executor, sink_third_party_item)
            .await?;
        let uptodate_sink_party_item = upsert_item.value();
        debug!(
            "Created a new {integration_provider_kind} sink item {} for task {}",
            uptodate_sink_party_item.id, task.id
        );

        task.sink_item = Some(*uptodate_sink_party_item.clone());
        self.task_service
            .upgrade()
            .context("Unable to access task_service from third_party_service")?
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

        Ok(Box::new(*uptodate_sink_party_item))
    }
}
