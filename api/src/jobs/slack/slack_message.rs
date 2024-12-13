use std::sync::Arc;

use anyhow::anyhow;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::warn;

use universal_inbox::{
    integration_connection::{
        integrations::slack::{SlackConfig, SlackContext, SlackMessageConfig},
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
        IntegrationConnection,
    },
    third_party::item::{ThirdPartyItem, ThirdPartyItemKind},
};

use crate::{
    integrations::slack::{find_slack_references_in_message, SlackService},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::{service::NotificationService, NotificationEventService},
        third_party::service::ThirdPartyItemService,
        UniversalInboxError,
    },
};

#[tracing::instrument(
    level = "debug",
    skip(
        executor,
        event,
        notification_service,
        integration_connection_service,
        third_party_item_service,
        slack_service
    ),
    err
)]
pub async fn handle_slack_message_push_event<'a>(
    executor: &mut Transaction<'a, Postgres>,
    event: &SlackPushEventCallback,
    notification_service: Arc<RwLock<NotificationService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    slack_service: Arc<SlackService>,
) -> Result<(), UniversalInboxError> {
    let (provider_user_ids, thread_ts, sender_user_id) = match event {
        SlackPushEventCallback {
            team_id,
            event:
                SlackEventCallbackBody::Message(SlackMessageEvent {
                    origin: SlackMessageOrigin { ref thread_ts, .. },
                    content: Some(ref content),
                    sender:
                        SlackMessageSender {
                            user: ref sender_user_id,
                            ..
                        },
                    ..
                }),
            ..
        } => {
            let references = find_slack_references_in_message(content);
            let mut user_ids: Vec<String> =
                references.users.keys().map(|id| id.to_string()).collect();

            if let Some((access_token, _)) = integration_connection_service
                .read()
                .await
                .find_slack_access_token(
                    executor,
                    IntegrationConnectionContext::Slack(SlackContext {
                        team_id: team_id.clone(),
                    }),
                )
                .await?
            {
                let slack_api_token =
                    SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));
                for usergroup_id in references.usergroups.keys() {
                    let usergroup_users = slack_service
                        .list_users_in_usergroup(usergroup_id, &slack_api_token)
                        .await?;
                    user_ids.extend(
                        usergroup_users
                            .iter()
                            .map(|user_id| user_id.to_string())
                            .collect::<Vec<String>>(),
                    );
                }
            }

            (
                user_ids
                    .into_iter()
                    .filter(|user_id| {
                        sender_user_id
                            .as_ref()
                            .map(|sender_user_id| *user_id != *sender_user_id.0)
                            .unwrap_or(true)
                    })
                    .collect::<Vec<String>>(),
                thread_ts.clone(),
                sender_user_id.clone(),
            )
        }
        _ => {
            warn!("Slack push event is not a message event");
            return Ok(());
        }
    };

    let mut handled_integration_connection_ids = vec![];
    for provider_user_id in provider_user_ids {
        let Some(integration_connection) = integration_connection_service
            .read()
            .await
            .get_integration_connection_per_provider_user_id(
                executor,
                IntegrationProviderKind::Slack,
                provider_user_id.clone(),
            )
            .await?
        else {
            return Ok(());
        };

        handle_slack_message_push_event_if_enabled(
            executor,
            event,
            &integration_connection,
            None,
            notification_service.clone(),
        )
        .await?;
        handled_integration_connection_ids.push(integration_connection.id);
    }

    let Some(thread_ts) = thread_ts else {
        return Ok(());
    };
    let third_party_items = third_party_item_service
        .read()
        .await
        .find_third_party_items_for_source_id(
            executor,
            ThirdPartyItemKind::SlackThread,
            thread_ts.as_ref(),
            sender_user_id,
        )
        .await?;

    for third_party_item in third_party_items.iter() {
        if !handled_integration_connection_ids.contains(&third_party_item.integration_connection_id)
        {
            let Some(integration_connection) = integration_connection_service
                .read()
                .await
                .get_integration_connection(executor, third_party_item.integration_connection_id)
                .await?
            else {
                return Ok(());
            };

            handle_slack_message_push_event_if_enabled(
                executor,
                event,
                &integration_connection,
                Some(third_party_item),
                notification_service.clone(),
            )
            .await?;
        }
    }

    Ok(())
}

async fn handle_slack_message_push_event_if_enabled<'a>(
    executor: &mut Transaction<'a, Postgres>,
    event: &SlackPushEventCallback,
    integration_connection: &IntegrationConnection,
    existing_third_party_item: Option<&ThirdPartyItem>,
    notification_service: Arc<RwLock<NotificationService>>,
) -> Result<(), UniversalInboxError> {
    let IntegrationProvider::Slack {
        config: slack_config,
        ..
    } = &integration_connection.provider
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Integration connection {} provider is supposed to be Slack",
            integration_connection.id
        )));
    };

    if let SlackConfig {
        message_config:
            SlackMessageConfig {
                sync_enabled: true,
                is_2way_sync,
            },
        ..
    } = slack_config
    {
        notification_service
            .read()
            .await
            .save_notification_from_event(
                executor,
                event,
                // In 2way sync scenario, we ignore existing third_party_item
                if *is_2way_sync {
                    None
                } else {
                    existing_third_party_item
                },
                integration_connection.user_id,
            )
            .await?;
    }

    Ok(())
}
