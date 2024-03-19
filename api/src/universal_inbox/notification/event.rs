use anyhow::{anyhow, Context};
use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tracing::info;

use universal_inbox::{
    integration_connection::{
        integrations::slack::{SlackSyncTaskConfig, SlackSyncType},
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    notification::{
        integrations::slack::SlackPushEventCallbackExt, Notification, NotificationStatus,
    },
    task::{
        integrations::todoist::TODOIST_INBOX_PROJECT, service::TaskPatch, TaskCreation, TaskStatus,
    },
};

use crate::universal_inbox::{
    notification::{service::NotificationService, NotificationEventService},
    UniversalInboxError,
};

#[async_trait]
impl NotificationEventService<SlackPushEventCallback> for NotificationService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, event), err)]
    async fn save_notification_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: SlackPushEventCallback,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let (provider_user_id, task_status) = match &event {
            SlackPushEventCallback {
                event: SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { user, .. }),
                ..
            } => (user.to_string(), TaskStatus::Active),
            SlackPushEventCallback {
                event: SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { user, .. }),
                ..
            } => (user.to_string(), TaskStatus::Done),
            _ => {
                return Err(UniversalInboxError::UnsupportedAction(format!(
                    "Unsupported Slack event {event:?}"
                )))
            }
        };

        let integration_connection = self
            .integration_connection_service
            .read()
            .await
            .get_integration_connection_per_provider_user_id(
                executor,
                IntegrationProviderKind::Slack,
                provider_user_id.clone(),
            )
            .await?
            .ok_or_else(|| {
                UniversalInboxError::UnsupportedAction(format!(
                    "Integration connection not found for Slack user id {provider_user_id}"
                ))
            })?;

        let IntegrationProvider::Slack {
            config: slack_config,
        } = &integration_connection.provider
        else {
            return Ok(vec![]);
        };

        if !slack_config.sync_enabled {
            return Ok(vec![]);
        }

        let mut notification = event.into_notification(integration_connection.user_id)?;
        if let SlackSyncType::AsTasks(_) = &slack_config.sync_type {
            // When syncing notifications as tasks, we keep the notification status as deleted.
            notification.status = NotificationStatus::Deleted;
        }

        let saved_notifications = self
            .save_notifications_and_sync_details(
                executor,
                &self.slack_service,
                vec![notification],
                integration_connection.user_id,
            )
            .await?;
        let [saved_notification] = saved_notifications.as_slice() else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Unexpected empty saved notifications list"
            )));
        };

        if let SlackSyncType::AsTasks(SlackSyncTaskConfig {
            target_project,
            default_due_at,
            default_priority,
        }) = &slack_config.sync_type
        {
            if let Some(task_id) = saved_notification.task_id {
                info!(
                    "Updating task to status {task_status} from Slack notification {} for user {}",
                    saved_notification.id, integration_connection.user_id
                );
                let task_patch = TaskPatch {
                    status: Some(task_status),
                    ..Default::default()
                };
                self.task_service
                    .upgrade()
                    .context("Unable to access task_service from notification_service")?
                    .read()
                    .await
                    .patch_task(
                        executor,
                        task_id,
                        &task_patch,
                        integration_connection.user_id,
                    )
                    .await?;
                return Ok(saved_notifications);
            }

            info!(
                "Creating task from Slack notification {} for user {}",
                saved_notification.id, integration_connection.user_id
            );
            let target_project = match target_project {
                Some(target_project) => target_project.clone(),
                None => {
                    self.task_service
                        .upgrade()
                        .context("Unable to access task_service from notification_service")?
                        .read()
                        .await
                        .get_or_create_project(
                            executor,
                            TODOIST_INBOX_PROJECT,
                            integration_connection.user_id,
                        )
                        .await?
                }
            };
            let task_creation = TaskCreation {
                title: saved_notification.title.clone(),
                body: None,
                project: target_project,
                due_at: default_due_at.as_ref().map(|due_at| due_at.clone().into()),
                priority: *default_priority,
            };

            self.create_task_from_notification(
                executor,
                saved_notification.id,
                &task_creation,
                false,
                integration_connection.user_id,
            )
            .await?;
        }

        Ok(saved_notifications)
    }
}
