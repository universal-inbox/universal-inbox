use anyhow::Context;
use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tracing::debug;

use universal_inbox::{
    notification::Notification,
    third_party::{
        integrations::slack::{SlackReaction, SlackStar, SlackThread},
        item::{
            ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemSource, ThirdPartyItemSourceKind,
        },
    },
    user::UserId,
};

use crate::{
    integrations::slack::SlackService,
    universal_inbox::{
        UniversalInboxError,
        notification::{NotificationEventService, service::NotificationService},
    },
};

#[async_trait]
impl NotificationEventService<SlackPushEventCallback> for NotificationService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = existing_third_party_item.map(|tpi| tpi.id.to_string()),
            third_party_item_source_id = existing_third_party_item.map(|tpi| tpi.source_id.clone()),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn save_notification_from_event(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        event: &SlackPushEventCallback,
        existing_third_party_item: Option<&ThirdPartyItem>,
        user_id: UserId,
    ) -> Result<Option<Notification>, UniversalInboxError> {
        let Some(mut third_party_item) = self
            .slack_service
            .fetch_item_from_event(executor, event, user_id)
            .await?
        else {
            return Ok(None);
        };

        // When given a third party item of a SlackThread, we want to ensure that we keep the subscribed status
        // This happen when 2 way sync is disabled
        if let Some(ThirdPartyItem {
            data: ThirdPartyItemData::SlackThread(existing_slack_thread),
            ..
        }) = existing_third_party_item
            && let ThirdPartyItem {
                data: ThirdPartyItemData::SlackThread(ref mut slack_thread),
                ..
            } = third_party_item
        {
            // If the existing thread is not subscribed, we want to keep it that way
            if !existing_slack_thread.subscribed {
                slack_thread.subscribed = false;
            }
        }

        let upsert_item = self
            .third_party_item_service
            .upgrade()
            .context("Unable to access third_party_item_service from notification_service")?
            .read()
            .await
            .create_or_update_third_party_item(executor, Box::new(third_party_item.clone()))
            .await?;

        let third_party_item_id = upsert_item.value_ref().id;
        let Some(third_party_item) = upsert_item.modified_value() else {
            debug!("Third party item {third_party_item_id} is already up to date");
            return Ok(None);
        };

        match (*third_party_item).get_third_party_item_source_kind() {
            ThirdPartyItemSourceKind::SlackStar => Ok(self
                .create_notification_from_third_party_item::<SlackStar, SlackService>(
                    executor,
                    *third_party_item,
                    self.slack_service.clone(),
                    user_id,
                )
                .await?),
            ThirdPartyItemSourceKind::SlackReaction => Ok(self
                .create_notification_from_third_party_item::<SlackReaction, SlackService>(
                    executor,
                    *third_party_item,
                    self.slack_service.clone(),
                    user_id,
                )
                .await?),
            ThirdPartyItemSourceKind::SlackThread => Ok(self
                .create_notification_from_third_party_item::<SlackThread, SlackService>(
                    executor,
                    *third_party_item,
                    self.slack_service.clone(),
                    user_id,
                )
                .await?),
            _ => Ok(None),
        }
    }
}
