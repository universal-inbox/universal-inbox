use async_trait::async_trait;
use clap::ValueEnum;
use macro_attr::macro_attr;
use serde::{Deserialize, Serialize};

use universal_inbox::{
    task::{Task, TaskCreation, TaskPatch},
    user::UserId,
};

use crate::{
    integrations::todoist::TodoistSyncStatusResponse, universal_inbox::UniversalInboxError,
};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

pub mod github;
pub mod oauth2;
pub mod todoist;

pub mod notification {
    use super::*;

    macro_attr! {
        // Synchronization sources for notifications
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
        pub enum NotificationSyncSourceKind {
            Github
        }
    }

    macro_attr! {
        // notification sources, either direct or from tasks
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
        pub enum NotificationSourceKind {
            Github,
            Todoist
        }
    }

    pub trait NotificationSource {
        fn get_notification_source_kind(&self) -> NotificationSourceKind;
    }

    #[async_trait]
    pub trait NotificationSourceService<T>: NotificationSource {
        async fn fetch_all_notifications(&self) -> Result<Vec<T>, UniversalInboxError>;
        async fn delete_notification_from_source(
            &self,
            source_id: &str,
        ) -> Result<(), UniversalInboxError>;
        async fn unsubscribe_notification_from_source(
            &self,
            source_id: &str,
        ) -> Result<(), UniversalInboxError>;
    }
}

pub mod task {
    use super::*;

    macro_attr! {
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
        pub enum TaskSyncSourceKind {
            Todoist
        }
    }

    macro_attr! {
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, EnumFromStr!, EnumDisplay!)]
        pub enum TaskSourceKind {
            Todoist
        }
    }

    pub trait TaskSource {
        fn get_task_source_kind(&self) -> TaskSourceKind;
    }

    #[async_trait]
    pub trait TaskSourceService<T>: TaskSource {
        async fn fetch_all_tasks(&self) -> Result<Vec<T>, UniversalInboxError>;
        async fn fetch_task(&self, source_id: &str) -> Result<Option<T>, UniversalInboxError>;
        async fn build_task(
            &self,
            source: &T,
            user_id: UserId,
        ) -> Result<Box<Task>, UniversalInboxError>;
        async fn create_task(&self, task: &TaskCreation) -> Result<T, UniversalInboxError>;
        async fn delete_task(
            &self,
            source_id: &str,
        ) -> Result<TodoistSyncStatusResponse, UniversalInboxError>;
        async fn complete_task(
            &self,
            source_id: &str,
        ) -> Result<TodoistSyncStatusResponse, UniversalInboxError>;
        async fn update_task(
            &self,
            id: &str,
            patch: &TaskPatch,
        ) -> Result<Option<TodoistSyncStatusResponse>, UniversalInboxError>;
    }
}
