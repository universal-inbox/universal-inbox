use anyhow::Context;
use async_trait::async_trait;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tracing::debug;

use universal_inbox::{
    third_party::{
        integrations::slack::{SlackReaction, SlackThread},
        item::{
            ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemSource, ThirdPartyItemSourceKind,
        },
    },
    user::UserId,
};

use crate::observability::attr;
use crate::{
    integrations::slack::{SlackFetchResult, SlackService},
    universal_inbox::{
        UniversalInboxError,
        notification::{
            NotificationEventOutcome, NotificationEventService, service::NotificationService,
        },
    },
};

#[async_trait]
impl NotificationEventService<SlackPushEventCallback> for NotificationService {
    // DEBUG on purpose (span rules: internals are DEBUG, so not exported at the default
    // `otel_trace_directive = "info"`). The INFO per-user spans of the callers
    // (`handle_slack_message_push_event_if_enabled`, `handle_slack_reaction_push_event`) carry
    // the Slack event attributes and the outcome this returns.
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::THIRD_PARTY_ITEM_ID } = existing_third_party_item.map(|tpi| tpi.id.to_string()),
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = existing_third_party_item.map(|tpi| tpi.source_id.clone()),
            { attr::USER_ID } = user_id.to_string()
        )
    )]
    async fn save_notification_from_event(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        event: &SlackPushEventCallback,
        existing_third_party_item: Option<&ThirdPartyItem>,
        user_id: UserId,
    ) -> Result<NotificationEventOutcome, UniversalInboxError> {
        let mut third_party_item = match self
            .slack_service
            .fetch_item_from_event(executor, event, user_id)
            .await?
        {
            SlackFetchResult::Fetched(third_party_item) => third_party_item,
            SlackFetchResult::Skipped(reason) => {
                return Ok(NotificationEventOutcome::Skipped(reason));
            }
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
            return Ok(NotificationEventOutcome::ItemUnchanged);
        };

        let upsert_notification = match (*third_party_item).get_third_party_item_source_kind() {
            ThirdPartyItemSourceKind::SlackReaction => {
                self.upsert_notification_from_third_party_item::<SlackReaction, SlackService>(
                    executor,
                    *third_party_item,
                    self.slack_service.clone(),
                    user_id,
                )
                .await?
            }
            ThirdPartyItemSourceKind::SlackThread => {
                self.upsert_notification_from_third_party_item::<SlackThread, SlackService>(
                    executor,
                    *third_party_item,
                    self.slack_service.clone(),
                    user_id,
                )
                .await?
            }
            _ => return Ok(NotificationEventOutcome::Skipped("unsupported_item_kind")),
        };
        Ok(NotificationEventOutcome::Saved(upsert_notification))
    }
}
