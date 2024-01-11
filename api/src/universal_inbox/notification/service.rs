use std::sync::{Arc, Weak};

use anyhow::{anyhow, Context};
use chrono::{Duration, Utc};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use universal_inbox::{
    integration_connection::IntegrationConnection,
    notification::{
        service::NotificationPatch, Notification, NotificationDetails, NotificationId,
        NotificationMetadata, NotificationSourceKind, NotificationStatus,
        NotificationSyncSourceKind, NotificationWithTask,
    },
    task::{service::TaskPatch, TaskCreation, TaskId, TaskStatus},
    user::UserId,
};

use crate::{
    integrations::{
        github::GithubService, google_mail::GoogleMailService, linear::LinearService,
        notification::NotificationSourceService, oauth2::AccessToken,
    },
    repository::{notification::NotificationRepository, Repository},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, task::service::TaskService,
        user::service::UserService, UniversalInboxError, UpdateStatus, UpsertStatus,
    },
};

// tag: New notification integration
pub struct NotificationService {
    repository: Arc<Repository>,
    github_service: GithubService,
    linear_service: LinearService,
    google_mail_service: Arc<RwLock<GoogleMailService>>,
    task_service: Weak<RwLock<TaskService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    user_service: Arc<RwLock<UserService>>,
    min_sync_notifications_interval_in_minutes: i64,
}

impl NotificationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: Arc<Repository>,
        github_service: GithubService,
        linear_service: LinearService,
        google_mail_service: Arc<RwLock<GoogleMailService>>,
        task_service: Weak<RwLock<TaskService>>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        user_service: Arc<RwLock<UserService>>,
        min_sync_notifications_interval_in_minutes: i64,
    ) -> NotificationService {
        NotificationService {
            repository,
            github_service,
            linear_service,
            google_mail_service,
            task_service,
            integration_connection_service,
            user_service,
            min_sync_notifications_interval_in_minutes,
        }
    }

    pub fn set_task_service(&mut self, task_service: Weak<RwLock<TaskService>>) {
        self.task_service = task_service;
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, notification_source_service, notification), fields(notification_id = notification.id.to_string()), err)]
    pub async fn apply_updated_notification_side_effect<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_service: &dyn NotificationSourceService,
        patch: &NotificationPatch,
        notification: Box<Notification>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        match patch.status {
            Some(NotificationStatus::Deleted) => {
                notification_source_service
                    .delete_notification_from_source(executor, &notification.source_id, user_id)
                    .await
            }
            Some(NotificationStatus::Unsubscribed) => {
                notification_source_service
                    .unsubscribe_notification_from_source(
                        executor,
                        &notification.source_id,
                        user_id,
                    )
                    .await
            }
            _ => {
                if let Some(snoozed_until) = patch.snoozed_until {
                    notification_source_service
                        .snooze_notification_from_source(
                            executor,
                            &notification.source_id,
                            snoozed_until,
                            user_id,
                        )
                        .await
                } else {
                    Ok(())
                }
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn list_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: Vec<NotificationStatus>,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
        user_id: UserId,
    ) -> Result<Vec<NotificationWithTask>, UniversalInboxError> {
        self.repository
            .fetch_all_notifications(
                executor,
                status,
                include_snoozed_notifications,
                task_id,
                user_id,
            )
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn get_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        for_user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let notification = self
            .repository
            .get_one_notification(executor, notification_id)
            .await?;

        if let Some(ref notif) = notification {
            if notif.user_id != for_user_id {
                return Err(UniversalInboxError::Forbidden(format!(
                    "Only the owner of the notification {notification_id} can access it"
                )));
            }
        }

        Ok(notification)
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn get_notification_for_source_id<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        for_user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        self.repository
            .get_notification_for_source_id(executor, source_id, for_user_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()), err)]
    pub async fn create_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
        for_user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        if notification.user_id != for_user_id {
            return Err(UniversalInboxError::Forbidden(format!(
                "A notification can only be created for {for_user_id}"
            )));
        }

        self.repository
            .create_notification(executor, notification)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.to_string()), err)]
    pub async fn create_or_update_notification<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: Box<Notification>,
        notification_source_kind: NotificationSourceKind,
        update_snoozed_until: bool,
    ) -> Result<UpsertStatus<Box<Notification>>, UniversalInboxError> {
        self.repository
            .create_or_update_notification(
                executor,
                notification,
                notification_source_kind,
                update_snoozed_until,
            )
            .await
    }

    async fn sync_source_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_service: &(dyn NotificationSourceService + Send + Sync),
        notification: &Notification,
        user_id: UserId,
    ) -> Result<Option<NotificationDetails>, UniversalInboxError> {
        let notification_details = notification_source_service
            .fetch_notification_details(executor, notification, user_id)
            .await?;
        let Some(details) = notification_details else {
            return Ok(None);
        };
        let details_upsert = self
            .repository
            .create_or_update_notification_details(executor, notification.id, details)
            .await?;
        Ok(Some(details_upsert.value()))
    }

    async fn sync_source_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_service: &(dyn NotificationSourceService + Send + Sync),
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let integration_provider_kind = notification_source_service.get_integration_provider_kind();
        let result: Option<(AccessToken, IntegrationConnection)> = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(
                executor,
                integration_provider_kind,
                Some(
                    Utc::now() - Duration::minutes(self.min_sync_notifications_interval_in_minutes),
                ),
                user_id,
            )
            .await?;

        if let Some((_, integration_connection)) = result {
            if !integration_connection
                .provider
                .is_sync_notifications_enabled()
            {
                debug!("{integration_provider_kind} integration for user {user_id} is disabled, skipping notifications sync.");
                return Ok(vec![]);
            }
        } else {
            debug!("No validated {integration_provider_kind} integration found for user {user_id}, skipping notifications sync.");
            return Ok(vec![]);
        }

        info!("Syncing {integration_provider_kind} integration for user {user_id}.");
        self.integration_connection_service
            .read()
            .await
            .update_integration_connection_sync_status(
                executor,
                user_id,
                integration_provider_kind,
                None,
            )
            .await?;
        match notification_source_service
            .fetch_all_notifications(executor, user_id)
            .await
        {
            Ok(source_notifications) => {
                let notification_source_kind =
                    notification_source_service.get_notification_source_kind();
                let notification_upserts = self
                    .save_notifications_from_source(
                        executor,
                        notification_source_kind,
                        source_notifications,
                        false,
                        notification_source_service.is_supporting_snoozed_notifications(),
                        user_id,
                    )
                    .await?;

                let mut notifications = vec![];
                let mut notification_details_synced = 0;
                for notification_upsert in notification_upserts {
                    let notification = match notification_upsert {
                        UpsertStatus::Created(notification)
                        | UpsertStatus::Updated(notification) => {
                            let notification_details = self
                                .sync_source_notification_details(
                                    executor,
                                    notification_source_service,
                                    &notification,
                                    user_id,
                                )
                                .await;
                            match notification_details {
                                Ok(notification_details) => {
                                    notification_details_synced += 1;

                                    Notification {
                                        details: notification_details,
                                        ..*notification
                                    }
                                }
                                Err(error @ UniversalInboxError::Recoverable(_)) => {
                                    // A recoverable error is considered not transient and we should mark the integration connection as failed.
                                    self.integration_connection_service
                                        .read()
                                        .await
                                        .update_integration_connection_sync_status(
                                            executor,
                                            user_id,
                                            integration_provider_kind,
                                            Some(format!(
                                                "Failed to fetch notification details from {integration_provider_kind}"
                                            )),
                                        )
                                        .await?;
                                    return Err(error);
                                }
                                // Making any other errors recoverable so that we can continue syncing other notifications.
                                Err(error) => {
                                    return Err(UniversalInboxError::Recoverable(error.into()))
                                }
                            }
                        }
                        UpsertStatus::Untouched(notification) => *notification,
                    };
                    notifications.push(notification);
                }

                info!(
                    "{} {notification_source_kind} notification details successfully synced for user {user_id}.",
                    notification_details_synced
                );
                Ok(notifications)
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
                            "Failed to fetch notifications from {integration_provider_kind}"
                        )),
                    )
                    .await?;
                Err(UniversalInboxError::Recoverable(e.into()))
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self, executor, source_notifications), err)]
    pub async fn save_notifications_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_source_kind: NotificationSourceKind,
        source_notifications: Vec<Notification>,
        is_incremental_update: bool,
        update_snoozed_until: bool,
        user_id: UserId,
    ) -> Result<Vec<UpsertStatus<Box<Notification>>>, UniversalInboxError> {
        let mut upsert_notifications: Vec<UpsertStatus<Box<Notification>>> = vec![];
        for notification in source_notifications {
            let upsert_notification = self
                .repository
                .create_or_update_notification(
                    executor,
                    Box::new(notification),
                    notification_source_kind,
                    update_snoozed_until,
                )
                .await?;
            upsert_notifications.push(upsert_notification);
        }
        info!(
            "{} {notification_source_kind} notifications successfully synced for user {user_id}.",
            upsert_notifications.len()
        );

        // For incremental synchronization, there is no need to update stale notifications
        if !is_incremental_update {
            let source_notification_ids = upsert_notifications
                .iter()
                .map(|upsert_notification| upsert_notification.value_ref().source_id.clone())
                .collect::<Vec<String>>();

            let deleted_notifications = self
                .repository
                .update_stale_notifications_status_from_source_ids(
                    executor,
                    source_notification_ids,
                    notification_source_kind,
                    NotificationStatus::Deleted,
                    user_id,
                )
                .await?;
            info!(
                "{} {notification_source_kind} notifications marked as deleted for user {user_id}.",
                deleted_notifications.len()
            );
        }

        Ok(upsert_notifications)
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn sync_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source: NotificationSyncSourceKind,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        // tag: New notification integration
        match source {
            NotificationSyncSourceKind::Github => {
                self.sync_source_notifications(executor, &self.github_service, user_id)
                    .await
            }
            NotificationSyncSourceKind::Linear => {
                self.sync_source_notifications(executor, &self.linear_service, user_id)
                    .await
            }
            NotificationSyncSourceKind::GoogleMail => {
                self.sync_source_notifications(
                    executor,
                    &*self.google_mail_service.read().await,
                    user_id,
                )
                .await
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn sync_notifications_with_transaction<'a>(
        &self,
        source: NotificationSyncSourceKind,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let mut transaction = self.begin().await.context(format!(
            "Failed to create new transaction while syncing {source:?}"
        ))?;

        match self
            .sync_notifications(&mut transaction, source, user_id)
            .await
        {
            Ok(notifications) => {
                transaction
                    .commit()
                    .await
                    .context(format!("Failed to commit while syncing {source:?}"))?;
                Ok(notifications)
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
    pub async fn sync_all_notifications<'a>(
        &self,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        // tag: New notification integration
        let notifications_from_github = self
            .sync_notifications_with_transaction(NotificationSyncSourceKind::Github, user_id)
            .await?;
        let notifications_from_linear = self
            .sync_notifications_with_transaction(NotificationSyncSourceKind::Linear, user_id)
            .await?;
        let notifications_from_google_mail = self
            .sync_notifications_with_transaction(NotificationSyncSourceKind::GoogleMail, user_id)
            .await?;
        Ok(notifications_from_github
            .into_iter()
            .chain(notifications_from_linear.into_iter())
            .chain(notifications_from_google_mail.into_iter())
            .collect())
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn sync_notifications_for_all_users<'a>(
        &self,
        source: Option<NotificationSyncSourceKind>,
    ) -> Result<(), UniversalInboxError> {
        let service = self.user_service.read().await;
        let mut transaction = service.begin().await.context(
            "Failed to create new transaction while syncing notifications for all users",
        )?;
        let users = service.fetch_all_users(&mut transaction).await?;
        for user in users {
            let user_id = user.id;
            info!("Syncing notifications for user {user_id}");
            let sync_result = if let Some(source) = source {
                self.sync_notifications_with_transaction(source, user_id)
                    .await
            } else {
                self.sync_all_notifications(user_id).await
            };
            match sync_result {
                Ok(notifications) => info!(
                    "{} notifications successfully synced for user {user_id}",
                    notifications.len()
                ),
                Err(err) => error!("Failed to sync notifications for user {user_id}: {err:?}"),
            };
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn patch_notification<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        patch: &'b NotificationPatch,
        apply_task_side_effects: bool,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        let updated_notification = self
            .repository
            .update_notification(executor, notification_id, patch, for_user_id)
            .await?;

        match updated_notification {
            UpdateStatus {
                updated: true,
                result: Some(ref notification),
            } => {
                // tag: New notification integration
                match notification.metadata {
                    NotificationMetadata::Github(_) => {
                        self.apply_updated_notification_side_effect(
                            executor,
                            &self.github_service,
                            patch,
                            notification.clone(),
                            for_user_id,
                        )
                        .await?;
                    }
                    NotificationMetadata::Linear(_) => {
                        self.apply_updated_notification_side_effect(
                            executor,
                            &self.linear_service,
                            patch,
                            notification.clone(),
                            for_user_id,
                        )
                        .await?
                    }
                    NotificationMetadata::GoogleMail(_) => {
                        self.apply_updated_notification_side_effect(
                            executor,
                            &*self.google_mail_service.read().await,
                            patch,
                            notification.clone(),
                            for_user_id,
                        )
                        .await?
                    }
                    NotificationMetadata::Todoist => {
                        if let Some(NotificationStatus::Deleted) = patch.status {
                            if let Some(task_id) = notification.task_id {
                                if apply_task_side_effects {
                                    self.task_service
                                        .upgrade()
                                        .context(
                                            "Unable to access task_service from notification_service",
                                    )?
                                    .read()
                                    .await
                                    .patch_task(
                                        executor,
                                        task_id,
                                        &TaskPatch {
                                            status: Some(TaskStatus::Deleted),
                                            ..Default::default()
                                        },
                                        for_user_id,
                                    )
                                    .await?;
                                }
                            } else {
                                return Err(UniversalInboxError::Unexpected(anyhow!(
                                    "Todoist notification {notification_id} is expected to be linked to a task"
                                )));
                            }
                            // Other actions than delete or snoozing is not supported
                        } else if patch.snoozed_until.is_none() {
                            return Err(UniversalInboxError::UnsupportedAction(format!(
                                "Cannot update the status of Todoist notification {notification_id}, update task's project"
                            )));
                        }
                    }
                };

                if let Some(task_id) = patch.task_id {
                    if apply_task_side_effects {
                        self.task_service
                            .upgrade()
                            .context("Unable to access task_service from notification_service")?
                            .read()
                            .await
                            .link_notification_with_task(
                                executor,
                                notification,
                                task_id,
                                for_user_id,
                            )
                            .await?;
                    }
                }
            }
            UpdateStatus {
                updated: false,
                result: None,
            } => {
                if self
                    .repository
                    .does_notification_exist(executor, notification_id)
                    .await?
                {
                    return Err(UniversalInboxError::Forbidden(format!(
                        "Only the owner of the notification {notification_id} can patch it"
                    )));
                }
            }
            _ => {}
        }

        Ok(updated_notification)
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn patch_notifications_for_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &'b NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError> {
        self.repository
            .update_notifications_for_task(executor, task_id, notification_kind, patch)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn create_task_from_notification<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification_id: NotificationId,
        task_creation: &'b TaskCreation,
        for_user_id: UserId,
    ) -> Result<Option<NotificationWithTask>, UniversalInboxError> {
        let notification = self
            .get_notification(executor, notification_id, for_user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot create task from unknown notification {notification_id}")
            })?;
        let task = self
            .task_service
            .upgrade()
            .context("Unable to access task_service from notification_service")?
            .read()
            .await
            .create_task_from_notification(executor, task_creation, &notification, for_user_id)
            .await?;

        let delete_patch = NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            task_id: Some(task.id),
            ..Default::default()
        };
        let updated_notification = self
            .patch_notification(executor, notification_id, &delete_patch, false, for_user_id)
            .await?;

        if let UpdateStatus {
            updated: true,
            result: Some(ref notification),
        } = updated_notification
        {
            return Ok(Some(NotificationWithTask::build(notification, Some(*task))));
        }

        Ok(None)
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn delete_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source: NotificationSourceKind,
    ) -> Result<u64, UniversalInboxError> {
        self.repository
            .delete_notification_details(executor, source)
            .await
    }
}
