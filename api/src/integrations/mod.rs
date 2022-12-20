use async_trait::async_trait;
use clap::ArgEnum;
use macro_attr::macro_attr;
use serde::{Deserialize, Serialize};

use crate::universal_inbox::UniversalInboxError;

pub mod github;
pub mod todoist;

pub mod notification {
    use super::*;

    macro_attr! {
        // Synchronization sources for notifications
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug, EnumFromStr!, EnumDisplay!)]
        pub enum NotificationSyncSourceKind {
            Github
        }
    }

    macro_attr! {
        // notification sources, either direct or from tasks
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug, EnumFromStr!, EnumDisplay!)]
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

    use universal_inbox::task::Task;

    macro_attr! {
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug, EnumFromStr!, EnumDisplay!)]
        pub enum TaskSyncSourceKind {
            Todoist
        }
    }

    macro_attr! {
        #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug, EnumFromStr!, EnumDisplay!)]
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
        async fn build_task(&self, source: &T) -> Result<Box<Task>, UniversalInboxError>;
    }
}
