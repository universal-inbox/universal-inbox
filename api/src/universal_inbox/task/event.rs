use anyhow::Context;
use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tracing::debug;

use universal_inbox::{
    task::Task,
    third_party::{
        integrations::slack::{SlackReaction, SlackStar},
        item::{ThirdPartyItemSource, ThirdPartyItemSourceKind},
    },
    user::UserId,
};

use crate::{
    integrations::slack::SlackService,
    universal_inbox::{
        task::{service::TaskService, TaskEventService},
        UniversalInboxError,
    },
};

#[async_trait]
impl TaskEventService<SlackPushEventCallback> for TaskService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, event))]
    async fn save_task_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        event: &SlackPushEventCallback,
        user_id: UserId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        let Some(third_party_item) = self
            .slack_service
            .fetch_item_from_event(executor, event, user_id)
            .await?
        else {
            return Ok(None);
        };

        let upsert_item = self
            .third_party_item_service
            .upgrade()
            .context("Unable to access third_party_item_service from task_service")?
            .read()
            .await
            .create_or_update_third_party_item(executor, third_party_item.clone())
            .await?;

        let third_party_item_id = upsert_item.value_ref().id;
        let Some(third_party_item) = upsert_item.modified_value() else {
            debug!("Third party item {third_party_item_id} is already up to date");
            return Ok(None);
        };

        match (*third_party_item).get_third_party_item_source_kind() {
            ThirdPartyItemSourceKind::SlackStar => Ok(self
                .create_task_from_third_party_item::<SlackStar, SlackService>(
                    executor,
                    *third_party_item,
                    self.slack_service.clone(),
                    user_id,
                )
                .await?
                .map(|task_result| task_result.task)),
            ThirdPartyItemSourceKind::SlackReaction => Ok(self
                .create_task_from_third_party_item::<SlackReaction, SlackService>(
                    executor,
                    *third_party_item,
                    self.slack_service.clone(),
                    user_id,
                )
                .await?
                .map(|task_result| task_result.task)),
            _ => Ok(None),
        }
    }
}
