use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::debug;

use universal_inbox::{
    integration_connection::provider::IntegrationProviderSource,
    task::{service::TaskPatch, Task, TaskCreation},
    third_party::{
        integrations::slack::{SlackReaction, SlackStar},
        item::{
            ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemFromSource,
            ThirdPartyItemSource, ThirdPartyItemSourceKind,
        },
    },
    user::UserId,
};

use crate::{
    integrations::{
        linear::LinearService, slack::SlackService, task::ThirdPartyTaskSourceService,
        third_party::ThirdPartyItemSourceService, todoist::TodoistService,
    },
    repository::{third_party::ThirdPartyItemRepository, Repository},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        UniversalInboxError, UpsertStatus,
    },
};

pub struct ThirdPartyItemService {
    repository: Arc<Repository>,
    task_service: Weak<RwLock<TaskService>>,
    notification_service: Weak<RwLock<NotificationService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    todoist_service: Arc<TodoistService>,
    slack_service: Arc<SlackService>,
    linear_service: Arc<LinearService>,
}

impl ThirdPartyItemService {
    pub fn new(
        repository: Arc<Repository>,
        task_service: Weak<RwLock<TaskService>>,
        notification_service: Weak<RwLock<NotificationService>>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        todoist_service: Arc<TodoistService>,
        slack_service: Arc<SlackService>,
        linear_service: Arc<LinearService>,
    ) -> Self {
        Self {
            repository,
            task_service,
            notification_service,
            integration_connection_service,
            todoist_service,
            slack_service,
            linear_service,
        }
    }

    pub fn set_task_service(&mut self, task_service: Weak<RwLock<TaskService>>) {
        self.task_service = task_service;
    }

    pub fn set_notification_service(
        &mut self,
        notification_service: Weak<RwLock<NotificationService>>,
    ) {
        self.notification_service = notification_service;
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
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
    pub async fn create_task_item<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Option<ThirdPartyItemCreationResult>, UniversalInboxError> {
        let upserted_third_party_item = self
            .save_third_party_item(executor, third_party_item)
            .await?;
        let third_party_item = upserted_third_party_item.value();
        let task_creation = match third_party_item.get_third_party_item_source_kind() {
            ThirdPartyItemSourceKind::Todoist => {
                self.task_service
                    .upgrade()
                    .context("Unable to access task_service from third_party_service")?
                    .read()
                    .await
                    .create_task_from_third_party_item(
                        executor,
                        *third_party_item.clone(),
                        self.todoist_service.clone(),
                        user_id,
                    )
                    .await?
            }
            ThirdPartyItemSourceKind::SlackStar => {
                self.task_service
                    .upgrade()
                    .context("Unable to access task_service from third_party_service")?
                    .read()
                    .await
                    .create_task_from_third_party_item::<SlackStar, SlackService>(
                        executor,
                        *third_party_item.clone(),
                        self.slack_service.clone(),
                        user_id,
                    )
                    .await?
            }
            ThirdPartyItemSourceKind::SlackReaction => {
                self.task_service
                    .upgrade()
                    .context("Unable to access task_service from third_party_service")?
                    .read()
                    .await
                    .create_task_from_third_party_item::<SlackReaction, SlackService>(
                        executor,
                        *third_party_item.clone(),
                        self.slack_service.clone(),
                        user_id,
                    )
                    .await?
            }
            ThirdPartyItemSourceKind::LinearIssue => {
                self.task_service
                    .upgrade()
                    .context("Unable to access task_service from third_party_service")?
                    .read()
                    .await
                    .create_task_from_third_party_item(
                        executor,
                        *third_party_item.clone(),
                        self.linear_service.clone(),
                        user_id,
                    )
                    .await?
            }
            kind => {
                return Err(anyhow!(
                    "Cannot create a task item from a third party item of kind {kind}",
                )
                .into());
            }
        };

        Ok(task_creation.map(|creation| ThirdPartyItemCreationResult {
            third_party_item: *third_party_item,
            task: Some(creation.task),
            notification: creation.notifications.first().cloned(),
        }))
    }

    #[tracing::instrument(level = "debug", skip(self, executor, third_party_service), err)]
    pub async fn sync_items<'a, T, U>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_service: Arc<U>,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyItemSourceService<T> + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let kind = third_party_service.get_third_party_item_source_kind();
        let items = third_party_service.fetch_items(executor, user_id).await?;
        let mut upserted_third_party_items = vec![];

        debug!("Syncing {kind} third party items for user {user_id}");
        for item in items.into_iter() {
            let upsert_result = self.save_third_party_item(executor, item).await?;

            upserted_third_party_items.push(*upsert_result.value());
        }
        debug!(
            "Successfully synced {} third party items for user {user_id}",
            upserted_third_party_items.len()
        );

        if !third_party_service.is_sync_incremental() {
            let active_source_third_party_item_ids = upserted_third_party_items
                .iter()
                .map(|tpi| tpi.id)
                .collect();

            if let Ok(task_source_kind) = kind.try_into() {
                let third_party_items_to_mark_as_done = self
                    .repository
                    .get_stale_task_source_third_party_items(
                        executor,
                        active_source_third_party_item_ids,
                        task_source_kind,
                        user_id,
                    )
                    .await?;
                let third_party_items_to_mark_as_done_count =
                    third_party_items_to_mark_as_done.len();
                debug!("Marking {third_party_items_to_mark_as_done_count} stale third party items as done",);

                for item in third_party_items_to_mark_as_done.into_iter() {
                    let upsert_result = self
                        .save_third_party_item(executor, item.marked_as_done())
                        .await?;

                    if let Some(upserted_third_party_item) = upsert_result.modified_value() {
                        upserted_third_party_items.push(*upserted_third_party_item);
                    }
                }
                debug!(
                    "Marked {third_party_items_to_mark_as_done_count} {kind} stale third party items as done"
                );
            } else if let Ok(notification_source_kind) = kind.try_into() {
                self.notification_service
                    .upgrade()
                    .context("Unable to access task_service from third_party_service")?
                    .read()
                    .await
                    .delete_stale_notifications_status_from_source_ids(
                        executor,
                        active_source_third_party_item_ids,
                        notification_source_kind,
                        user_id,
                    )
                    .await?;
            } else {
                return Err(anyhow!(
                    "Cannot mark stale third party items as done for {kind}, neither a TaskSource nor a NotificationSource"
                )
                .into());
            };
        }

        Ok(upserted_third_party_items)
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
        overwrite_existing_sink_item: bool,
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
            if !overwrite_existing_sink_item {
                debug!(
                    "Task {} already has a {} sink item for {}, returning it",
                    task.id,
                    sink_item.kind(),
                    sink_item.source_id
                );
                return Ok(Box::new(sink_item.clone()));
            }
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
