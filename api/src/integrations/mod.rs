use async_trait::async_trait;

use universal_inbox::{
    notification::NotificationSource,
    task::{service::TaskPatch, Task, TaskCreation, TaskSource},
    user::UserId,
};

use crate::{
    integrations::{oauth2::AccessToken, todoist::TodoistSyncStatusResponse},
    universal_inbox::UniversalInboxError,
};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

pub mod github;
pub mod oauth2;
pub mod todoist;

pub mod notification {
    use super::*;

    #[async_trait]
    pub trait NotificationSourceService<T>: NotificationSource {
        async fn fetch_all_notifications(
            &self,
            access_token: &AccessToken,
        ) -> Result<Vec<T>, UniversalInboxError>;
        async fn delete_notification_from_source(
            &self,
            source_id: &str,
            access_token: &AccessToken,
        ) -> Result<(), UniversalInboxError>;
        async fn unsubscribe_notification_from_source(
            &self,
            source_id: &str,
            access_token: &AccessToken,
        ) -> Result<(), UniversalInboxError>;
    }
}

pub mod task {
    use super::*;

    #[async_trait]
    pub trait TaskSourceService<T>: TaskSource {
        async fn fetch_all_tasks(
            &self,
            access_token: &AccessToken,
        ) -> Result<Vec<T>, UniversalInboxError>;
        async fn fetch_task(
            &self,
            source_id: &str,
            access_token: &AccessToken,
        ) -> Result<Option<T>, UniversalInboxError>;
        async fn build_task(
            &self,
            source: &T,
            user_id: UserId,
            access_token: &AccessToken,
        ) -> Result<Box<Task>, UniversalInboxError>;
        async fn create_task(
            &self,
            task: &TaskCreation,
            access_token: &AccessToken,
        ) -> Result<T, UniversalInboxError>;
        async fn delete_task(
            &self,
            source_id: &str,
            access_token: &AccessToken,
        ) -> Result<TodoistSyncStatusResponse, UniversalInboxError>;
        async fn complete_task(
            &self,
            source_id: &str,
            access_token: &AccessToken,
        ) -> Result<TodoistSyncStatusResponse, UniversalInboxError>;
        async fn update_task(
            &self,
            id: &str,
            patch: &TaskPatch,
            access_token: &AccessToken,
        ) -> Result<Option<TodoistSyncStatusResponse>, UniversalInboxError>;
    }
}
