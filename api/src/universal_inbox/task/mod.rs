use async_trait::async_trait;
use sqlx::{Postgres, Transaction};

use universal_inbox::{task::Task, user::UserId};

use crate::universal_inbox::UniversalInboxError;

pub mod event;
pub mod service;

#[async_trait]
pub trait TaskEventService<T> {
    async fn save_task_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: &T,
        user_id: UserId,
    ) -> Result<Option<Task>, UniversalInboxError>;
}
