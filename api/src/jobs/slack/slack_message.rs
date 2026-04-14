use std::sync::Arc;

use anyhow::anyhow;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::warn;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection,
        integrations::slack::{SlackConfig, SlackContext, SlackMessageConfig},
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    third_party::item::{ThirdPartyItem, ThirdPartyItemKind},
};

use crate::{
    integrations::slack::{SlackService, find_slack_references_in_message},
    universal_inbox::{
        UniversalInboxError,
        integration_connection::service::IntegrationConnectionService,
        notification::{NotificationEventService, service::NotificationService},
        third_party::service::ThirdPartyItemService,
    },
};

#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn handle_slack_message_push_event(
    executor: &mut Transaction<'_, Postgres>,
    event: &SlackPushEventCallback,
    notification_service: Arc<RwLock<NotificationService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    slack_service: Arc<SlackService>,
) -> Result<(), UniversalInboxError> {
    let current_span = tracing::Span::current();

    let (provider_user_ids, thread_ts) = match event {
        SlackPushEventCallback {
            team_id,
            event:
                SlackEventCallbackBody::Message(SlackMessageEvent {
                    origin: SlackMessageOrigin { thread_ts, .. },
                    content: Some(content),
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
                        extension_credentials: vec![],
                        last_extension_heartbeat_at: None,
                    }),
                )
                .await?
            {
                let slack_api_token =
                    SlackApiToken::new(SlackApiTokenValue(access_token.to_string()))
                        .with_team_id(team_id.clone());
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

            (user_ids, thread_ts.clone())
        }
        _ => {
            current_span.set_attribute("slack.message.outcome", "discarded");
            current_span.set_attribute("slack.message.discard_reason", "not_a_message_event");
            warn!("Slack push event is not a message event");
            return Ok(());
        }
    };

    current_span.set_attribute(
        "slack.referenced_user_count",
        provider_user_ids.len() as i64,
    );

    let integration_connections = integration_connection_service
        .read()
        .await
        .find_integration_connection_per_provider_user_ids(
            executor,
            IntegrationProviderKind::Slack,
            provider_user_ids,
        )
        .await?;
    let handled_integration_connection_ids = integration_connections
        .iter()
        .map(|integration_connection| integration_connection.id)
        .collect::<Vec<_>>();

    current_span.set_attribute(
        "slack.matched_integration_connections_count",
        integration_connections.len() as i64,
    );

    for integration_connection in integration_connections {
        handle_slack_message_push_event_if_enabled(
            executor,
            event,
            integration_connection,
            None,
            notification_service.clone(),
        )
        .await?;
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
            None,
        )
        .await?;

    current_span.set_attribute(
        "slack.known_thread_items_count",
        third_party_items.len() as i64,
    );

    for third_party_item in third_party_items.iter() {
        if !handled_integration_connection_ids.contains(&third_party_item.integration_connection_id)
            && let Some(integration_connection) = integration_connection_service
                .read()
                .await
                .get_integration_connection(executor, third_party_item.integration_connection_id)
                .await?
        {
            handle_slack_message_push_event_if_enabled(
                executor,
                event,
                integration_connection,
                Some(third_party_item),
                notification_service.clone(),
            )
            .await?;
        }
    }

    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err)]
async fn handle_slack_message_push_event_if_enabled(
    executor: &mut Transaction<'_, Postgres>,
    event: &SlackPushEventCallback,
    integration_connection: IntegrationConnection,
    existing_third_party_item: Option<&ThirdPartyItem>,
    notification_service: Arc<RwLock<NotificationService>>,
) -> Result<(), UniversalInboxError> {
    let current_span = tracing::Span::current();
    current_span.set_attribute(
        "slack.integration_connection_id",
        integration_connection.id.to_string(),
    );
    current_span.set_attribute("user.id", integration_connection.user_id.to_string());

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
                ..
            },
        ..
    } = slack_config
    {
        current_span.set_attribute("slack.message_sync_enabled", true);
        let user_id = integration_connection.user_id;
        notification_service
            .read()
            .await
            .save_notification_from_event(
                executor,
                event,
                if *is_2way_sync {
                    None
                } else {
                    existing_third_party_item
                },
                user_id,
            )
            .await?;
    } else {
        current_span.set_attribute("slack.message_sync_enabled", false);
    }

    Ok(())
}
