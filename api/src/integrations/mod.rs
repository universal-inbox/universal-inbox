use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    notification::{Notification, NotificationDetails, NotificationSource},
    task::{service::TaskPatch, Task, TaskCreation, TaskSource},
    user::UserId,
};

use crate::{
    integrations::todoist::TodoistSyncStatusResponse, universal_inbox::UniversalInboxError,
};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

// tag: New notification integration
pub mod github;
pub mod google_mail;
pub mod linear;
pub mod oauth2;
pub mod slack;
pub mod todoist;

pub mod notification {
    use super::*;

    #[async_trait]
    pub trait NotificationSourceService: NotificationSource {
        async fn delete_notification_from_source<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source_id: &str,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn unsubscribe_notification_from_source<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source_id: &str,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn snooze_notification_from_source<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source_id: &str,
            snoozed_until_at: DateTime<Utc>,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn fetch_notification_details<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            notification: &Notification,
            user_id: UserId,
        ) -> Result<Option<NotificationDetails>, UniversalInboxError>;
    }

    #[async_trait]
    pub trait NotificationSyncSourceService: NotificationSourceService {
        async fn fetch_all_notifications<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            user_id: UserId,
        ) -> Result<Vec<Notification>, UniversalInboxError>;
    }
}

pub mod task {
    use universal_inbox::task::ProjectSummary;

    use super::*;

    #[async_trait]
    pub trait TaskSourceService<T>: TaskSource {
        async fn fetch_all_tasks<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            user_id: UserId,
        ) -> Result<Vec<T>, UniversalInboxError>;
        async fn fetch_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source_id: &str,
            user_id: UserId,
        ) -> Result<Option<T>, UniversalInboxError>;
        async fn build_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source: &T,
            user_id: UserId,
        ) -> Result<Box<Task>, UniversalInboxError>;
        async fn create_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            task: &TaskCreation,
            user_id: UserId,
        ) -> Result<T, UniversalInboxError>;
        async fn delete_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source_id: &str,
            user_id: UserId,
        ) -> Result<TodoistSyncStatusResponse, UniversalInboxError>;
        async fn complete_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source_id: &str,
            user_id: UserId,
        ) -> Result<TodoistSyncStatusResponse, UniversalInboxError>;
        async fn update_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            id: &str,
            patch: &TaskPatch,
            user_id: UserId,
        ) -> Result<Option<TodoistSyncStatusResponse>, UniversalInboxError>;
        async fn search_projects<'a, 'b>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            matches: &'b str,
            user_id: UserId,
        ) -> Result<Vec<ProjectSummary>, UniversalInboxError>;
    }
}
