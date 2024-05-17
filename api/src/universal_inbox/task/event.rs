use anyhow::Context;
use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};

use universal_inbox::{task::Task, third_party::item::ThirdPartyItemCreationResult, user::UserId};

use crate::universal_inbox::{
    task::{service::TaskService, TaskEventService},
    UniversalInboxError,
};

#[async_trait]
impl TaskEventService<SlackPushEventCallback> for TaskService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, event), err)]
    async fn save_task_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: SlackPushEventCallback,
        user_id: UserId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        let Some(third_party_item) = self
            .slack_service
            .fetch_item_from_event(executor, &event, user_id)
            .await?
        else {
            return Ok(None);
        };

        let Some(ThirdPartyItemCreationResult {
            task: Some(task), ..
        }) = self
            .third_party_item_service
            .upgrade()
            .context("Unable to access third_party_item_service from task_service")?
            .read()
            .await
            .create_item(executor, third_party_item, user_id)
            .await?
        else {
            return Ok(None);
        };

        Ok(Some(task))
    }
}
