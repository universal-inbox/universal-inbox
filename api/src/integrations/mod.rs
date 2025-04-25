use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    notification::{Notification, NotificationSource},
    task::{
        service::TaskPatch, CreateOrUpdateTaskRequest, ProjectSummary, TaskCreation,
        TaskCreationConfig,
    },
    third_party::item::{ThirdPartyItem, ThirdPartyItemSourceKind},
    user::UserId,
};

use crate::{integrations::oauth2::AccessToken, universal_inbox::UniversalInboxError};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

// tag: New notification integration
pub mod github;
pub mod google_calendar;
pub mod google_mail;
pub mod linear;
pub mod oauth2;
pub mod slack;
pub mod todoist;

pub mod third_party {
    use super::*;

    #[async_trait]
    pub trait ThirdPartyItemSourceService<T> {
        async fn fetch_items(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            user_id: UserId,
        ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;

        fn is_sync_incremental(&self) -> bool;

        fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind;
    }
}

pub mod notification {
    use super::*;

    #[async_trait]
    pub trait ThirdPartyNotificationSourceService<T>: NotificationSource {
        async fn third_party_item_into_notification(
            &self,
            source: &T,
            source_third_party_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<Box<Notification>, UniversalInboxError>;
        async fn delete_notification_from_source(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            source_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn unsubscribe_notification_from_source(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            source_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn snooze_notification_from_source(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            source_item: &ThirdPartyItem,
            snoozed_until_at: DateTime<Utc>,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
    }
}

pub mod task {
    use super::*;

    #[async_trait]
    pub trait ThirdPartyTaskService<T> {
        async fn third_party_item_into_task(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            source: &T,
            source_third_party_item: &ThirdPartyItem,
            task_creation_config: Option<TaskCreationConfig>,
            user_id: UserId,
        ) -> Result<Box<CreateOrUpdateTaskRequest>, UniversalInboxError>;
        async fn delete_task(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            third_party_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn complete_task(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            third_party_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn uncomplete_task(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            third_party_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn update_task(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            id: &str,
            patch: &TaskPatch,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
    }

    #[async_trait]
    pub trait ThirdPartyTaskSourceService<T> {
        async fn create_task(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            task: &TaskCreation,
            user_id: UserId,
        ) -> Result<T, UniversalInboxError>;
        async fn search_projects(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            matches: &str,
            user_id: UserId,
        ) -> Result<Vec<ProjectSummary>, UniversalInboxError>;
        async fn get_or_create_project(
            &self,
            executor: &mut Transaction<'_, Postgres>,
            project_name: &str,
            user_id: UserId,
            access_token: Option<&AccessToken>,
        ) -> Result<ProjectSummary, UniversalInboxError>;
    }
}
