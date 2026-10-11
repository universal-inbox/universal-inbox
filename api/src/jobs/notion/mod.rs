use std::{collections::HashSet, sync::Arc};

use anyhow::Context;
use apalis::prelude::Data;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    notification::{NotificationStatus, service::NotificationPatch},
    third_party::{
        integrations::notion::NotionComment,
        item::{ThirdPartyItemFromSource, ThirdPartyItemKind},
    },
};

use crate::{
    integrations::notion::{
        NotionService, RawNotionComment, RawNotionPage, is_access_denied,
        mentioned_user_ids_in_raw_rich_text,
        webhook::{NotionWebhookEvent, NotionWebhookObject},
    },
    observability::{RecordSpanError, attr},
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, third_party::service::ThirdPartyItemService,
    },
};

/// How many connections of the event's workspace are tried to read the
/// comment before giving up (each may not see the page).
const MAX_TRIAGE_CONNECTIONS: usize = 3;

#[derive(Debug, Serialize, Deserialize)]
pub struct NotionWebhookEventJob(pub NotionWebhookEvent);

#[tracing::instrument(level = "info", skip_all, fields({ attr::ERROR_TYPE } = tracing::field::Empty))]
pub async fn handle_notion_webhook_event(
    job: NotionWebhookEventJob,
    notification_service: Data<Arc<RwLock<NotificationService>>>,
    integration_connection_service: Data<Arc<RwLock<IntegrationConnectionService>>>,
    third_party_item_service: Data<Arc<RwLock<ThirdPartyItemService>>>,
) -> Result<(), UniversalInboxError> {
    let result: Result<(), UniversalInboxError> = async move {
        let event = job.0;
        if !event.event_type.is_comment_event() {
            return Ok(());
        }

        let service = notification_service.read().await;
        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while handling a Notion event")?;

        handle_notion_comment_event(
            &mut transaction,
            &event,
            (*notification_service).clone(),
            (*integration_connection_service).clone(),
            (*third_party_item_service).clone(),
        )
        .await?;

        transaction
            .commit()
            .await
            .context("Failed to commit while handling a Notion event")?;
        Ok(())
    }
    .await;
    result.record_span_error()
}

/// What the triage read about the commented object with a workspace token.
struct CommentTriage {
    comments: Vec<RawNotionComment>,
    page: RawNotionPage,
    parent_block_created_by: Option<String>,
}

/// Handle a `comment.*` event: find the Universal Inbox users it concerns,
/// then store the discussion and its notification for each of them.
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(event_id = event.id.as_str())
)]
pub async fn handle_notion_comment_event(
    executor: &mut Transaction<'_, Postgres>,
    event: &NotionWebhookEvent,
    notification_service: Arc<RwLock<NotificationService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
) -> Result<(), UniversalInboxError> {
    let (Some(page_id), Some(parent)) = (&event.data.page_id, &event.data.parent) else {
        debug!("Notion comment event without page or parent, ignoring it");
        return Ok(());
    };
    let notion_service = notification_service.read().await.notion_service.clone();

    let triage = triage_comment(
        executor,
        event,
        page_id,
        parent,
        &notion_service,
        &integration_connection_service,
    )
    .await?;

    let Some(discussion_id) = event.data.discussion_id.clone().or_else(|| {
        triage.as_ref().and_then(|triage| {
            triage
                .comments
                .iter()
                .find(|comment| comment.id == event.entity.id)
                .map(|comment| comment.discussion_id.clone())
        })
    }) else {
        debug!("Unable to find the discussion of Notion comment, ignoring it");
        return Ok(());
    };

    // Reasons to look at the discussion for a user: author of the comment
    // (replying makes them follow the thread), mentioned in it, creator of
    // the page or of the commented block...
    let mut candidate_user_ids: Vec<String> = event
        .authors
        .iter()
        .filter(|author| author.object_type == "person")
        .map(|author| author.id.clone())
        .collect();
    if let Some(triage) = &triage {
        if let Some(comment) = triage
            .comments
            .iter()
            .find(|comment| comment.id == event.entity.id)
        {
            candidate_user_ids.extend(mentioned_user_ids_in_raw_rich_text(&comment.rich_text));
        }
        candidate_user_ids.extend(triage.page.created_by.iter().map(|user| user.id.clone()));
        candidate_user_ids.extend(triage.parent_block_created_by.clone());
    }
    let mut kept_user_ids = vec![];
    for user_id in candidate_user_ids.into_iter().collect::<HashSet<_>>() {
        let config = integration_connection_service
            .read()
            .await
            .get_integration_connection_config_for_provider_user_id(
                executor,
                IntegrationProviderKind::Notion,
                user_id.clone(),
            )
            .await?;
        if let Some(IntegrationConnectionConfig::Notion(config)) = config
            && config.sync_comments_enabled
        {
            kept_user_ids.push(user_id);
        }
    }

    let mut connections = if kept_user_ids.is_empty() {
        vec![]
    } else {
        integration_connection_service
            .read()
            .await
            .find_integration_connection_per_provider_user_ids(
                executor,
                IntegrationProviderKind::Notion,
                kept_user_ids,
            )
            .await?
    };

    // ...or already following the discussion.
    let followed_items = third_party_item_service
        .read()
        .await
        .find_third_party_items_for_source_id(
            executor,
            ThirdPartyItemKind::NotionComment,
            &discussion_id,
            None,
        )
        .await?;
    for item in followed_items {
        if connections
            .iter()
            .any(|connection| connection.id == item.integration_connection_id)
        {
            continue;
        }
        if let Some(connection) = integration_connection_service
            .read()
            .await
            .get_integration_connection(executor, item.integration_connection_id)
            .await?
        {
            connections.push(connection);
        }
    }

    let accessible_person_ids = event.accessible_person_ids();
    for connection in connections {
        if !is_concerned_connection(&connection, event, accessible_person_ids.as_deref()) {
            continue;
        }
        let connection_id = connection.id;
        if let Err(error) = save_discussion_for_connection(
            executor,
            connection,
            page_id,
            &parent.id,
            &discussion_id,
            &notion_service,
            &notification_service,
            &integration_connection_service,
            &third_party_item_service,
        )
        .await
        {
            if is_access_denied(&error) {
                info!(
                    { attr::INTEGRATION_CONNECTION_ID } = connection_id.to_string(),
                    "Skipping Notion discussion: {error}"
                );
            } else {
                warn!(
                    { attr::INTEGRATION_CONNECTION_ID } = connection_id.to_string(),
                    "Failed to save Notion discussion: {error:?}"
                );
            }
        }
    }

    Ok(())
}

/// Read the comment, its page and its block with the token of a connection of
/// the event's workspace. `None` when no connection can see them.
async fn triage_comment(
    executor: &mut Transaction<'_, Postgres>,
    event: &NotionWebhookEvent,
    page_id: &str,
    parent: &NotionWebhookObject,
    notion_service: &NotionService,
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
) -> Result<Option<CommentTriage>, UniversalInboxError> {
    let workspace_connections = integration_connection_service
        .read()
        .await
        .find_validated_integration_connections_per_provider_workspace(
            executor,
            IntegrationProviderKind::Notion,
            &event.workspace_id,
        )
        .await?;

    for connection in workspace_connections.iter().take(MAX_TRIAGE_CONNECTIONS) {
        let Some((access_token, _)) = integration_connection_service
            .read()
            .await
            .find_access_token_for_connection(executor, connection.id, connection.user_id)
            .await?
        else {
            continue;
        };

        let triage: Result<CommentTriage, UniversalInboxError> = async {
            let comments = notion_service
                .list_comments(&access_token, &parent.id)
                .await?;
            let page = notion_service.get_page(&access_token, page_id).await?;
            let parent_block_created_by = if parent.object_type == "block" {
                notion_service
                    .get_block(&access_token, &parent.id)
                    .await?
                    .created_by
                    .map(|user| user.id)
            } else {
                None
            };
            Ok(CommentTriage {
                comments,
                page,
                parent_block_created_by,
            })
        }
        .await;

        match triage {
            Ok(triage) => return Ok(Some(triage)),
            Err(error) if is_access_denied(&error) => {
                info!(
                    { attr::INTEGRATION_CONNECTION_ID } = connection.id.to_string(),
                    "Notion comment triage skipped a connection: {error}"
                );
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    Ok(None)
}

/// The connection is a validated Notion connection of the event's workspace,
/// syncing comments, whose user can see the page.
fn is_concerned_connection(
    connection: &IntegrationConnection,
    event: &NotionWebhookEvent,
    accessible_person_ids: Option<&[&str]>,
) -> bool {
    if connection.status != IntegrationConnectionStatus::Validated {
        return false;
    }
    let IntegrationProvider::Notion {
        context: Some(context),
        config,
    } = &connection.provider
    else {
        return false;
    };
    if !config.sync_comments_enabled || context.workspace_id != event.workspace_id {
        return false;
    }
    match (accessible_person_ids, &connection.provider_user_id) {
        (Some(ids), Some(provider_user_id)) => ids.contains(&provider_user_id.as_str()),
        (None, Some(_)) => true,
        (_, None) => false,
    }
}

#[allow(clippy::too_many_arguments)]
async fn save_discussion_for_connection(
    executor: &mut Transaction<'_, Postgres>,
    connection: IntegrationConnection,
    page_id: &str,
    parent_id: &str,
    discussion_id: &str,
    notion_service: &Arc<NotionService>,
    notification_service: &Arc<RwLock<NotificationService>>,
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: &Arc<RwLock<ThirdPartyItemService>>,
) -> Result<(), UniversalInboxError> {
    let Some(provider_user_id) = connection.provider_user_id.clone() else {
        return Ok(());
    };
    let Some((access_token, _)) = integration_connection_service
        .read()
        .await
        .find_access_token_for_connection(executor, connection.id, connection.user_id)
        .await?
    else {
        return Ok(());
    };

    let existing_item = third_party_item_service
        .read()
        .await
        .find_third_party_items_for_source_id(
            executor,
            ThirdPartyItemKind::NotionComment,
            discussion_id,
            Some(connection.user_id),
        )
        .await?
        .into_iter()
        .find(|item| item.integration_connection_id == connection.id);

    let discussion = notion_service
        .fetch_discussion(
            &access_token,
            page_id,
            parent_id,
            discussion_id,
            &provider_user_id,
        )
        .await?;

    let Some(discussion) = discussion else {
        // Every comment was deleted: the notification has nothing left to show
        if existing_item.is_some() {
            mark_notification_as_deleted(
                executor,
                discussion_id,
                &connection,
                notification_service,
            )
            .await?;
        }
        return Ok(());
    };

    if existing_item.is_none() && !discussion.is_user_involved(&provider_user_id) {
        return Ok(());
    }

    let third_party_item = discussion.into_third_party_item(connection.user_id, connection.id);
    let upsert_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(executor, Box::new(third_party_item))
        .await?;
    let Some(third_party_item) = upsert_item.modified_value() else {
        debug!("Notion discussion is already up to date");
        return Ok(());
    };

    notification_service
        .read()
        .await
        .create_notification_from_third_party_item::<NotionComment, NotionService>(
            executor,
            *third_party_item,
            notion_service.clone(),
            connection.user_id,
        )
        .await?;
    Ok(())
}

async fn mark_notification_as_deleted(
    executor: &mut Transaction<'_, Postgres>,
    discussion_id: &str,
    connection: &IntegrationConnection,
    notification_service: &Arc<RwLock<NotificationService>>,
) -> Result<(), UniversalInboxError> {
    let service = notification_service.read().await;
    let Some(notification) = service
        .get_notification_for_source_id(executor, discussion_id, connection.user_id)
        .await?
    else {
        return Ok(());
    };
    service
        .patch_notification(
            executor,
            notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
            false,
            false,
            connection.user_id,
        )
        .await?;
    Ok(())
}
