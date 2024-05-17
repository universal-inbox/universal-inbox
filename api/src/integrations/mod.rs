use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use universal_inbox::{
    notification::{Notification, NotificationDetails, NotificationSource},
    task::{service::TaskPatch, ProjectSummary, Task, TaskCreation},
    third_party::item::ThirdPartyItem,
    user::UserId,
};

use crate::{integrations::oauth2::AccessToken, universal_inbox::UniversalInboxError};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

// tag: New notification integration
pub mod github;
pub mod google_mail;
pub mod linear;
pub mod oauth2;
pub mod slack;
pub mod todoist;

pub mod third_party {
    use universal_inbox::third_party::item::ThirdPartyItemSource;

    use super::*;

    #[async_trait]
    pub trait ThirdPartyItemSourceService: ThirdPartyItemSource {
        async fn fetch_items<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            user_id: UserId,
        ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError>;
    }
}

pub mod notification {
    use super::*;

    #[async_trait]
    pub trait NotificationSourceService: NotificationSource {
        async fn delete_notification_from_source<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            notification: &Notification,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn unsubscribe_notification_from_source<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            notification: &Notification,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn snooze_notification_from_source<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            notification: &Notification,
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
    use super::*;

    #[async_trait]
    pub trait ThirdPartyTaskService<T> {
        async fn third_party_item_into_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            source: &T,
            source_third_party_item: &ThirdPartyItem,
            task_creation: Option<TaskCreation>,
            user_id: UserId,
        ) -> Result<Box<Task>, UniversalInboxError>;
        async fn delete_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            third_party_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn complete_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            third_party_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn uncomplete_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            third_party_item: &ThirdPartyItem,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
        async fn update_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            id: &str,
            patch: &TaskPatch,
            user_id: UserId,
        ) -> Result<(), UniversalInboxError>;
    }

    #[async_trait]
    pub trait ThirdPartyTaskSourceService<T> {
        async fn create_task<'a>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            task: &TaskCreation,
            user_id: UserId,
        ) -> Result<T, UniversalInboxError>;
        async fn search_projects<'a, 'b>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            matches: &'b str,
            user_id: UserId,
        ) -> Result<Vec<ProjectSummary>, UniversalInboxError>;
        async fn get_or_create_project<'a, 'b>(
            &self,
            executor: &mut Transaction<'a, Postgres>,
            project_name: &'b str,
            user_id: UserId,
            access_token: Option<&'b AccessToken>,
        ) -> Result<ProjectSummary, UniversalInboxError>;
    }
}
