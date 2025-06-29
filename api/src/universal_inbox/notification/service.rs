use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use anyhow::{anyhow, Context};
use apalis_redis::RedisStorage;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use universal_inbox::{
    integration_connection::provider::IntegrationProvider,
    notification::{
        service::{InvitationPatch, NotificationPatch},
        Notification, NotificationId, NotificationListOrder, NotificationSource,
        NotificationSourceKind, NotificationStatus, NotificationSyncSourceKind,
        NotificationWithTask,
    },
    task::{service::TaskPatch, Task, TaskCreation, TaskId, TaskStatus},
    third_party::{
        integrations::slack::{SlackReaction, SlackStar, SlackThread},
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemId, ThirdPartyItemKind},
    },
    user::UserId,
    Page, PageToken,
};

use crate::{
    integrations::{
        github::GithubService, google_calendar::GoogleCalendarService,
        google_mail::GoogleMailService, linear::LinearService,
        notification::ThirdPartyNotificationSourceService, slack::SlackService,
        third_party::ThirdPartyItemSourceService,
    },
    jobs::UniversalInboxJob,
    repository::{notification::NotificationRepository, Repository},
    universal_inbox::{
        integration_connection::service::{
            IntegrationConnectionService, IntegrationConnectionSyncType,
        },
        task::service::TaskService,
        third_party::service::ThirdPartyItemService,
        user::service::UserService,
        UniversalInboxError, UpdateStatus, UpsertStatus,
    },
};

// tag: New notification integration
pub struct NotificationService {
    pub(super) repository: Arc<Repository>,
    pub github_service: Arc<GithubService>,
    pub linear_service: Arc<LinearService>,
    pub google_calendar_service: Arc<GoogleCalendarService>,
    pub google_mail_service: Arc<RwLock<GoogleMailService>>,
    pub slack_service: Arc<SlackService>,
    pub(super) task_service: Weak<RwLock<TaskService>>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    pub(super) third_party_item_service: Weak<RwLock<ThirdPartyItemService>>,
    user_service: Arc<UserService>,
    min_sync_notifications_interval_in_minutes: i64,
}

impl NotificationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: Arc<Repository>,
        github_service: Arc<GithubService>,
        linear_service: Arc<LinearService>,
        google_calendar_service: Arc<GoogleCalendarService>,
        google_mail_service: Arc<RwLock<GoogleMailService>>,
        slack_service: Arc<SlackService>,
        task_service: Weak<RwLock<TaskService>>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        third_party_item_service: Weak<RwLock<ThirdPartyItemService>>,
        user_service: Arc<UserService>,
        min_sync_notifications_interval_in_minutes: i64,
    ) -> NotificationService {
        NotificationService {
            repository,
            github_service,
            linear_service,
            google_calendar_service,
            google_mail_service,
            slack_service,
            task_service,
            integration_connection_service,
            third_party_item_service,
            user_service,
            min_sync_notifications_interval_in_minutes,
        }
    }

    pub fn set_task_service(&mut self, task_service: Weak<RwLock<TaskService>>) {
        self.task_service = task_service;
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            patch,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn apply_updated_notification_side_effect<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification_source_service: Arc<U>,
        patch: &NotificationPatch,
        source_item: &mut ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyNotificationSourceService<T> + NotificationSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        debug!(
            "Applying {} side effects for updated notification from third party item {}",
            notification_source_service.get_notification_source_kind(),
            source_item.id
        );
        match patch.status {
            Some(NotificationStatus::Deleted) => {
                if source_item.kind() == ThirdPartyItemKind::SlackThread {
                    let ThirdPartyItemData::SlackThread(ref mut slack_thread) = source_item.data
                    else {
                        return Err(UniversalInboxError::Unexpected(anyhow!(
                            "Unexpected third party item data type {} for {}, expected SlackThread",
                            source_item.kind(),
                            source_item.id
                        )));
                    };
                    slack_thread.last_read = Some(slack_thread.messages.last().origin.ts.clone());

                    self.third_party_item_service
                        .upgrade()
                        .context(
                            "Unable to access third_party_item_service from notification_service",
                        )?
                        .read()
                        .await
                        .create_or_update_third_party_item(executor, Box::new(source_item.clone()))
                        .await?;
                }

                notification_source_service
                    .delete_notification_from_source(executor, source_item, user_id)
                    .await
            }
            Some(NotificationStatus::Unsubscribed) => {
                if source_item.kind() == ThirdPartyItemKind::SlackThread {
                    let ThirdPartyItemData::SlackThread(ref mut slack_thread) = source_item.data
                    else {
                        return Err(UniversalInboxError::Unexpected(anyhow!(
                            "Unexpected third party item data type {} for {}, expected SlackThread",
                            source_item.kind(),
                            source_item.id
                        )));
                    };
                    slack_thread.subscribed = false;

                    self.third_party_item_service
                        .upgrade()
                        .context(
                            "Unable to access third_party_item_service from notification_service",
                        )?
                        .read()
                        .await
                        .create_or_update_third_party_item(executor, Box::new(source_item.clone()))
                        .await?;
                }

                notification_source_service
                    .unsubscribe_notification_from_source(executor, source_item, user_id)
                    .await
            }
            _ => {
                if let Some(snoozed_until) = patch.snoozed_until {
                    notification_source_service
                        .snooze_notification_from_source(
                            executor,
                            source_item,
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

    #[tracing::instrument(
        level = "debug", 
        skip_all,
        fields(
            trigger_sync = job_storage.is_some(),
            status = status.iter().map(|s| s.to_string()).collect::<Vec<String>>().join(","),
            include_snoozed_notifications,
            task_id = task_id.map(|id| id.to_string()),
            order_by,
            from_sources,
            page_token,
            user.id = user_id.to_string()
        ),
        err
    )]
    #[allow(clippy::too_many_arguments)]
    pub async fn list_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: Vec<NotificationStatus>,
        include_snoozed_notifications: bool,
        task_id: Option<TaskId>,
        order_by: NotificationListOrder,
        from_sources: Vec<NotificationSourceKind>,
        page_token: Option<PageToken>,
        user_id: UserId,
        job_storage: Option<RedisStorage<UniversalInboxJob>>,
    ) -> Result<Page<NotificationWithTask>, UniversalInboxError> {
        let notifications_page = self
            .repository
            .fetch_all_notifications(
                executor,
                status,
                include_snoozed_notifications,
                task_id,
                order_by,
                from_sources,
                page_token,
                user_id,
            )
            .await?;

        if let Some(job_storage) = job_storage {
            self.integration_connection_service
                .read()
                .await
                .trigger_sync_for_integration_connections(executor, user_id, job_storage)
                .await?;
        }

        Ok(notifications_page)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification_id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn get_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn get_notification_for_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_id: &str,
        for_user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        self.repository
            .get_notification_for_source_id(executor, source_id, for_user_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification.id.to_string(),
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn create_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification.id.to_string(),
            notification_source_kind = notification_source_kind.to_string(),
            update_snoozed_until
        ),
        err
    )]
    pub async fn create_or_update_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_source_kind = notification_source_kind.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn delete_stale_notifications_status_from_source_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        active_source_third_party_item_ids: Vec<ThirdPartyItemId>,
        notification_source_kind: NotificationSourceKind,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let deleted_notifications = self
            .repository
            .update_stale_notifications_status_from_source_ids(
                executor,
                active_source_third_party_item_ids,
                notification_source_kind,
                NotificationStatus::Deleted,
                user_id,
            )
            .await?;
        info!(
            "{} {notification_source_kind} notifications marked as deleted for user {user_id}.",
            deleted_notifications.len()
        );

        Ok(deleted_notifications)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            synced_source = source.to_string(),
            user.id = user_id.to_string(),
            force_sync
        ),
        err
    )]
    pub async fn sync_notifications(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source: NotificationSyncSourceKind,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        // tag: New notification integration
        match source {
            NotificationSyncSourceKind::Github => {
                self.sync_third_party_notifications(
                    executor,
                    self.github_service.clone(),
                    user_id,
                    force_sync,
                )
                .await
            }
            NotificationSyncSourceKind::Linear => {
                self.sync_third_party_notifications(
                    executor,
                    self.linear_service.clone(),
                    user_id,
                    force_sync,
                )
                .await
            }
            NotificationSyncSourceKind::GoogleMail => {
                self.sync_third_party_notifications(
                    executor,
                    (*self.google_mail_service.read().await).clone().into(),
                    user_id,
                    force_sync,
                )
                .await
            }
        }
    }

    pub async fn sync_notifications_with_transaction(
        &self,
        source: NotificationSyncSourceKind,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let mut transaction = self.begin().await.context(format!(
            "Failed to create new transaction while syncing {source:?}"
        ))?;

        match self
            .sync_notifications(&mut transaction, source, user_id, force_sync)
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

    pub async fn sync_all_notifications(
        &self,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        // tag: New notification integration
        let notifications_from_github = self
            .sync_notifications_with_transaction(
                NotificationSyncSourceKind::Github,
                user_id,
                force_sync,
            )
            .await?;
        let notifications_from_linear = self
            .sync_notifications_with_transaction(
                NotificationSyncSourceKind::Linear,
                user_id,
                force_sync,
            )
            .await?;
        let notifications_from_google_mail = self
            .sync_notifications_with_transaction(
                NotificationSyncSourceKind::GoogleMail,
                user_id,
                force_sync,
            )
            .await?;
        Ok(notifications_from_github
            .into_iter()
            .chain(notifications_from_linear.into_iter())
            .chain(notifications_from_google_mail.into_iter())
            .collect())
    }

    pub async fn sync_notifications_for_all_users(
        &self,
        source: Option<NotificationSyncSourceKind>,
        force_sync: bool,
    ) -> Result<(), UniversalInboxError> {
        let service = self.user_service.clone();
        let mut transaction = service.begin().await.context(
            "Failed to create new transaction while syncing notifications for all users",
        )?;
        let users = service.fetch_all_users(&mut transaction).await?;

        for user in users {
            let _ = self
                .sync_notifications_for_user(source, user.id, force_sync)
                .await;
        }

        Ok(())
    }

    pub async fn sync_notifications_for_user(
        &self,
        source: Option<NotificationSyncSourceKind>,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<(), UniversalInboxError> {
        info!("Syncing notifications for user {user_id}");

        let sync_result = if let Some(source) = source {
            self.sync_notifications_with_transaction(source, user_id, force_sync)
                .await
        } else {
            self.sync_all_notifications(user_id, force_sync).await
        };
        match sync_result {
            Ok(notifications) => info!(
                "{} notifications successfully synced for user {user_id}",
                notifications.len()
            ),
            Err(err) => error!("Failed to sync notifications for user {user_id}: {err:?}"),
        };

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification_id.to_string(),
            patch,
            apply_task_side_effects,
            apply_notification_side_effects,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn patch_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification_id: NotificationId,
        patch: &NotificationPatch,
        apply_task_side_effects: bool,
        apply_notification_side_effects: bool,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        let mut updated_notification = self
            .repository
            .update_notification(executor, notification_id, patch, for_user_id)
            .await?;

        if !apply_notification_side_effects {
            return Ok(updated_notification);
        }

        match updated_notification {
            UpdateStatus {
                updated: true,
                result: Some(ref mut notification),
            } => {
                // tag: New notification integration
                match notification.kind {
                    NotificationSourceKind::Github => {
                        self.apply_updated_notification_side_effect(
                            executor,
                            self.github_service.clone(),
                            patch,
                            &mut notification.source_item,
                            for_user_id,
                        )
                        .await?;
                    }
                    NotificationSourceKind::Linear => {
                        self.apply_updated_notification_side_effect(
                            executor,
                            self.linear_service.clone(),
                            patch,
                            &mut notification.source_item,
                            for_user_id,
                        )
                        .await?
                    }
                    NotificationSourceKind::GoogleCalendar => {
                        // Google Calendar events are derived from Google Mail threads
                        if let Some(ref mut source_item) = notification.source_item.source_item {
                            if source_item.kind() == ThirdPartyItemKind::GoogleMailThread {
                                self.apply_updated_notification_side_effect(
                                    executor,
                                    (*self.google_mail_service.read().await).clone().into(),
                                    patch,
                                    source_item,
                                    for_user_id,
                                )
                                .await?
                            }
                        }

                        self.apply_updated_notification_side_effect(
                            executor,
                            self.google_calendar_service.clone(),
                            patch,
                            &mut notification.source_item,
                            for_user_id,
                        )
                        .await?
                    }
                    NotificationSourceKind::GoogleMail => {
                        self.apply_updated_notification_side_effect(
                            executor,
                            (*self.google_mail_service.read().await).clone().into(),
                            patch,
                            &mut notification.source_item,
                            for_user_id,
                        )
                        .await?
                    }
                    NotificationSourceKind::Slack => match notification.source_item.data {
                        ThirdPartyItemData::SlackReaction(_) => self
                            .apply_updated_notification_side_effect::<SlackReaction, SlackService>(
                                executor,
                                self.slack_service.clone(),
                                patch,
                                &mut notification.source_item,
                                for_user_id,
                            )
                            .await?,
                        ThirdPartyItemData::SlackStar(_) => {
                            self.apply_updated_notification_side_effect::<SlackStar, SlackService>(
                                executor,
                                self.slack_service.clone(),
                                patch,
                                &mut notification.source_item,
                                for_user_id,
                            )
                            .await?
                        }
                        ThirdPartyItemData::SlackThread(_) => self
                            .apply_updated_notification_side_effect::<SlackThread, SlackService>(
                                executor,
                                self.slack_service.clone(),
                                patch,
                                &mut notification.source_item,
                                for_user_id,
                            )
                            .await?,
                        _ => {
                            return Err(UniversalInboxError::Unexpected(anyhow!(
                                "Unsupported Slack notification data type for third party item {}",
                                notification.source_item.id
                            )))
                        }
                    },
                    NotificationSourceKind::Todoist => {
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
                    NotificationSourceKind::API => {
                        // API notifications do not have side effects
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_id = task_id.to_string(),
            notification_kind = notification_kind.map(|k| k.to_string()),
            patch
        ),
        err
    )]
    pub async fn patch_notifications_for_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        notification_kind: Option<NotificationSourceKind>,
        patch: &NotificationPatch,
    ) -> Result<Vec<UpdateStatus<Notification>>, UniversalInboxError> {
        self.repository
            .update_notifications_for_task(executor, task_id, notification_kind, patch)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification_id.to_string(),
            task_creation,
            apply_notification_side_effects,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn create_task_from_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification_id: NotificationId,
        task_creation: &TaskCreation,
        apply_notification_side_effects: bool,
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
            .create_task_from_notification(executor, task_creation, &notification)
            .await?;

        let delete_patch = NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            task_id: Some(task.id),
            ..Default::default()
        };
        let updated_notification = self
            .patch_notification(
                executor,
                notification_id,
                &delete_patch,
                false,
                apply_notification_side_effects,
                for_user_id,
            )
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn create_notification_from_third_party_item<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: ThirdPartyItem,
        third_party_notification_service: Arc<U>,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyNotificationSourceService<T> + NotificationSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let upsert_notification = self
            .save_third_party_item_as_notification(
                executor,
                &third_party_item,
                third_party_notification_service.clone(),
                None,
                user_id,
            )
            .await?;

        Ok(Some(*upsert_notification.value()))
    }

    async fn sync_third_party_notifications<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_notification_service: Arc<U>,
        user_id: UserId,
        force_sync: bool,
    ) -> Result<Vec<Notification>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyNotificationSourceService<T>
            + ThirdPartyItemSourceService<T>
            + NotificationSource
            + Send
            + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        async fn sync_third_party_notifications<T, U>(
            notification_service: &NotificationService,
            executor: &mut Transaction<'_, Postgres>,
            third_party_notification_service: Arc<U>,
            user_id: UserId,
        ) -> Result<Vec<Notification>, UniversalInboxError>
        where
            T: TryFrom<ThirdPartyItem> + Debug,
            U: ThirdPartyNotificationSourceService<T>
                + ThirdPartyItemSourceService<T>
                + NotificationSource
                + Send
                + Sync,
            <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
        {
            let third_party_items = notification_service
                .third_party_item_service
                .upgrade()
                .context("Unable to access third_party_item_service from notification_service")?
                .read()
                .await
                .sync_items(executor, third_party_notification_service.clone(), user_id)
                .await?;

            let mut notification_creation_results = vec![];
            for third_party_item in third_party_items {
                // tag: New notification integration
                if let Some(notification_creation_result) = match third_party_item.kind() {
                    // For now, only Google Calendar events are derived from another third party item
                    // and thus need an updated third party notification service
                    ThirdPartyItemKind::GoogleCalendarEvent => {
                        notification_service
                            .create_notification_from_third_party_item(
                                executor,
                                third_party_item,
                                notification_service.google_calendar_service.clone(),
                                user_id,
                            )
                            .await?
                    }
                    _ => {
                        notification_service
                            .create_notification_from_third_party_item(
                                executor,
                                third_party_item,
                                third_party_notification_service.clone(),
                                user_id,
                            )
                            .await?
                    }
                } {
                    notification_creation_results.push(notification_creation_result);
                }
            }
            Ok(notification_creation_results)
        }

        let integration_provider_kind =
            third_party_notification_service.get_integration_provider_kind();
        let integration_connection_service = self.integration_connection_service.read().await;
        let min_sync_interval_in_minutes = (!force_sync)
            .then_some(self.min_sync_notifications_interval_in_minutes)
            .unwrap_or_default();
        let Some(integration_connection) = integration_connection_service
            .get_integration_connection_to_sync(
                executor,
                integration_provider_kind,
                min_sync_interval_in_minutes,
                IntegrationConnectionSyncType::Notifications,
                user_id,
            )
            .await?
        else {
            debug!("No validated {integration_provider_kind} integration found for user {user_id}, skipping notifications sync");
            return Ok(vec![]);
        };

        if !integration_connection
            .provider
            .is_sync_notifications_enabled()
        {
            debug!("{integration_provider_kind} integration for user {user_id} is disabled, skipping notifications sync");
            return Ok(vec![]);
        }

        info!("Syncing {integration_provider_kind} notifications for user {user_id}");
        integration_connection_service
            .start_notifications_sync_status(executor, integration_provider_kind, user_id)
            .await?;

        let notification_creation_results = match sync_third_party_notifications(
            self,
            executor,
            third_party_notification_service,
            user_id,
        )
        .await
        {
            Err(e) => {
                integration_connection_service
                    .error_notifications_sync_status(
                        executor,
                        integration_provider_kind,
                        format!("Failed to fetch notifications from {integration_provider_kind}"),
                        user_id,
                    )
                    .await?;
                return Err(UniversalInboxError::Recoverable(e.into()));
            }
            Ok(notification_creation_results) => {
                integration_connection_service
                    .complete_notifications_sync_status(
                        executor,
                        integration_provider_kind,
                        user_id,
                    )
                    .await?;
                notification_creation_results
            }
        };

        info!(
            "Successfully synced {} {integration_provider_kind} notifications for user {user_id}",
            notification_creation_results.len()
        );

        Ok(notification_creation_results)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            task_id = task_id.map(|id| id.to_string()),
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn save_third_party_item_as_notification<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        third_party_notification_service: Arc<U>,
        task_id: Option<TaskId>,
        user_id: UserId,
    ) -> Result<UpsertStatus<Box<Notification>>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyNotificationSourceService<T> + NotificationSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let data: T = third_party_item.clone().try_into().map_err(|_| {
            anyhow!(
                "Unexpected third party item kind {} for {}",
                third_party_item.kind(),
                third_party_notification_service.get_integration_provider_kind()
            )
        })?;

        let mut notification = third_party_notification_service
            .third_party_item_into_notification(&data, third_party_item, user_id)
            .await?;
        notification.task_id = task_id;
        self.repository
            .create_or_update_notification(
                executor,
                notification,
                third_party_notification_service.get_notification_source_kind(),
                third_party_notification_service.is_supporting_snoozed_notifications(),
            )
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_id = task.id.to_string(),
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            integration_connection_provider = integration_connection_provider.kind().to_string(),
            is_incremental_update = _is_incremental_update,
            user.id = user_id.to_string()
        ),
        err
    )]
    #[allow(clippy::too_many_arguments)]
    pub async fn save_task_as_notification<T, U>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_task_service: Arc<U>,
        task: &Task,
        third_party_item: &ThirdPartyItem,
        integration_connection_provider: &IntegrationProvider,
        _is_incremental_update: bool,
        user_id: UserId,
    ) -> Result<Option<UpsertStatus<Box<Notification>>>, UniversalInboxError>
    where
        T: TryFrom<ThirdPartyItem> + Debug,
        U: ThirdPartyNotificationSourceService<T> + NotificationSource + Send + Sync,
        <T as TryFrom<ThirdPartyItem>>::Error: Send + Sync,
    {
        let existing_notifications = self
            .list_notifications(
                executor,
                vec![],
                true,
                Some(task.id),
                NotificationListOrder::UpdatedAtAsc,
                vec![],
                None,
                user_id,
                None,
            )
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
                .any(|n| n.kind == notification_source_kind);
            if !existing_notifications.is_empty() && !task_has_a_notification_from_the_same_source {
                return Ok(None);
            }

            debug!(
                "Creating notification from {} task {}",
                notification_source_kind, task.id
            );
            return self
                .save_third_party_item_as_notification(
                    executor,
                    third_party_item,
                    third_party_task_service,
                    Some(task.id),
                    user_id,
                )
                .await
                .map(Some);
        }

        // Update existing notifications for a task that is not in the Inbox anymore
        let mut updated_notifications = self
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            notification_id = notification_id.to_string(),
            patch,
            apply_task_side_effects,
            apply_notification_side_effects,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    pub async fn update_invitation_from_notification(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        notification_id: NotificationId,
        patch: &InvitationPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Notification>>, UniversalInboxError> {
        let Some(notification) = self
            .get_notification(executor, notification_id, for_user_id)
            .await?
        else {
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        if notification.kind != NotificationSourceKind::GoogleCalendar {
            return Err(UniversalInboxError::UnsupportedAction(format!(
                "Cannot update invitation from notification {notification_id}, expected GoogleCalendar notification"
            )));
        }

        let updated_event = self
            .google_calendar_service
            .answer_invitation(
                executor,
                &notification.source_item,
                patch.response_status,
                for_user_id,
            )
            .await?;

        // Update the third party item with the updated event data
        let mut updated_third_party_item = notification.source_item.clone();
        updated_third_party_item.data =
            ThirdPartyItemData::GoogleCalendarEvent(Box::new(updated_event));
        self.third_party_item_service
            .upgrade()
            .context("Unable to access third_party_item_service from notification_service")?
            .read()
            .await
            .create_or_update_third_party_item(executor, Box::new(updated_third_party_item))
            .await?;

        let updated_notification = self
            .patch_notification(
                executor,
                notification_id,
                &NotificationPatch {
                    status: Some(NotificationStatus::Deleted),
                    ..Default::default()
                },
                true,
                true,
                for_user_id,
            )
            .await?;

        Ok(updated_notification)
    }
}
