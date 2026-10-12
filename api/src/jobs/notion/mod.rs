use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Context;
use apalis::prelude::Data;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionId, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::notion::NotionConfig,
        provider::{IntegrationProvider, IntegrationProviderKind},
    },
    notification::{NotificationStatus, service::NotificationPatch},
    third_party::{
        integrations::notion::{NotionBlock, NotionComment},
        item::{ThirdPartyItemFromSource, ThirdPartyItemKind},
    },
};

use crate::{
    integrations::notion::{
        NotionObjectCache, NotionService, RawNotionComment, RawNotionPage, is_access_denied,
        is_unsubscribed_without_new_mention, mentioned_user_ids_in_raw_rich_text,
        webhook::{NotionWebhookEvent, NotionWebhookEventType, NotionWebhookObject},
    },
    integrations::oauth2::AccessToken,
    observability::{RecordSpanError, attr},
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, third_party::service::ThirdPartyItemService,
    },
};

/// How many connections of the event's workspace are tried to read the
/// comment before giving up (each may not see the page).
const MAX_TRIAGE_CONNECTIONS: usize = 3;

/// How many changed blocks of a `page.content_updated` event are read. A
/// paste into a big page must not fan out into hundreds of Notion calls.
const MAX_UPDATED_BLOCKS: usize = 50;

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
        if !event.event_type.is_handled_event() {
            return Ok(());
        }

        let service = notification_service.read().await;
        let mut transaction = service
            .begin()
            .await
            .context("Failed to create new transaction while handling a Notion event")?;

        if event.event_type == NotionWebhookEventType::PageContentUpdated {
            handle_notion_page_content_event(
                &mut transaction,
                &event,
                (*notification_service).clone(),
                (*integration_connection_service).clone(),
                (*third_party_item_service).clone(),
            )
            .await?;
        } else {
            handle_notion_comment_event(
                &mut transaction,
                &event,
                (*notification_service).clone(),
                (*integration_connection_service).clone(),
                (*third_party_item_service).clone(),
            )
            .await?;
        }

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
        if !is_concerned_connection(
            &connection,
            event,
            accessible_person_ids.as_deref(),
            |config| config.sync_comments_enabled,
        ) {
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

/// Handle a `page.content_updated` event: find the users mentioned in the
/// changed blocks, or who were, then store each mentioning block and its
/// notification for them.
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(
        event_id = event.id.as_str(),
        { attr::NOTION_UPDATED_BLOCKS_COUNT } = tracing::field::Empty,
        { attr::NOTION_MENTION_CANDIDATES_COUNT } = tracing::field::Empty
    )
)]
pub async fn handle_notion_page_content_event(
    executor: &mut Transaction<'_, Postgres>,
    event: &NotionWebhookEvent,
    notification_service: Arc<RwLock<NotificationService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
) -> Result<(), UniversalInboxError> {
    let current_span = tracing::Span::current();
    // The same block can be listed several times
    let mut seen_block_ids = HashSet::new();
    let mut block_ids: Vec<String> = event
        .data
        .updated_blocks
        .iter()
        .map(|block| block.id.clone())
        .filter(|block_id| seen_block_ids.insert(block_id.clone()))
        .collect();
    if block_ids.len() > MAX_UPDATED_BLOCKS {
        info!(
            "Notion event {} updated {} blocks, only the first {MAX_UPDATED_BLOCKS} are read",
            event.id,
            block_ids.len()
        );
        block_ids.truncate(MAX_UPDATED_BLOCKS);
    }
    current_span.set_attribute(attr::NOTION_UPDATED_BLOCKS_COUNT, block_ids.len() as i64);
    if block_ids.is_empty() {
        return Ok(());
    }
    let notion_service = notification_service.read().await.notion_service.clone();

    let mentioned_user_ids_per_block = triage_blocks(
        executor,
        event,
        &block_ids,
        &notion_service,
        &integration_connection_service,
    )
    .await?;

    // Users mentioned in a changed block...
    let mut kept_user_ids = vec![];
    for user_id in mentioned_user_ids_per_block
        .values()
        .flatten()
        .cloned()
        .collect::<HashSet<_>>()
    {
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
            && config.sync_mentions_enabled
        {
            kept_user_ids.push(user_id);
        }
    }
    current_span.set_attribute(
        attr::NOTION_MENTION_CANDIDATES_COUNT,
        kept_user_ids.len() as i64,
    );

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

    // ...or already notified about one, in case the mention was removed.
    let mut followed_block_ids: HashMap<IntegrationConnectionId, Vec<String>> = HashMap::new();
    for block_id in &block_ids {
        let items = third_party_item_service
            .read()
            .await
            .find_third_party_items_for_source_id(
                executor,
                ThirdPartyItemKind::NotionBlock,
                block_id,
                None,
            )
            .await?;
        for item in items {
            followed_block_ids
                .entry(item.integration_connection_id)
                .or_default()
                .push(block_id.clone());
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
    }

    let accessible_person_ids = event.accessible_person_ids();
    for connection in connections {
        if !is_concerned_connection(
            &connection,
            event,
            accessible_person_ids.as_deref(),
            |config| config.sync_mentions_enabled,
        ) {
            continue;
        }
        let Some(provider_user_id) = connection.provider_user_id.clone() else {
            continue;
        };
        let followed = followed_block_ids
            .get(&connection.id)
            .cloned()
            .unwrap_or_default();
        let connection_block_ids: Vec<&String> = block_ids
            .iter()
            .filter(|block_id| {
                followed.contains(block_id)
                    || mentioned_user_ids_per_block
                        .get(*block_id)
                        .is_some_and(|user_ids| user_ids.contains(&provider_user_id))
            })
            .collect();
        if connection_block_ids.is_empty() {
            continue;
        }

        let connection_id = connection.id;
        if let Err(error) = save_mentions_for_connection(
            executor,
            &connection,
            &provider_user_id,
            &connection_block_ids,
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
                    "Skipping Notion mentions: {error}"
                );
            } else {
                warn!(
                    { attr::INTEGRATION_CONNECTION_ID } = connection_id.to_string(),
                    "Failed to save Notion mentions: {error:?}"
                );
            }
        }
    }

    Ok(())
}

/// The raw user mentions of each changed block, read with the tokens of a few
/// connections of the event's workspace: a block one cannot see is tried
/// with the next one. A block no token can see has no mention.
async fn triage_blocks(
    executor: &mut Transaction<'_, Postgres>,
    event: &NotionWebhookEvent,
    block_ids: &[String],
    notion_service: &NotionService,
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
) -> Result<HashMap<String, Vec<String>>, UniversalInboxError> {
    let mut mentioned_user_ids_per_block = HashMap::new();
    let mut remaining_block_ids: Vec<&String> = block_ids.iter().collect();
    for (connection_id, access_token) in
        find_triage_access_tokens(executor, event, integration_connection_service).await?
    {
        let mut unreadable_block_ids = vec![];
        for block_id in remaining_block_ids {
            match notion_service.get_block(&access_token, block_id).await {
                Ok(block) => {
                    mentioned_user_ids_per_block.insert(
                        block_id.clone(),
                        mentioned_user_ids_in_raw_rich_text(&block.rich_text()),
                    );
                }
                Err(error) if is_access_denied(&error) => {
                    debug!(
                        { attr::INTEGRATION_CONNECTION_ID } = connection_id.to_string(),
                        "Notion block triage cannot read block {block_id}: {error}"
                    );
                    unreadable_block_ids.push(block_id);
                }
                Err(error) => return Err(error),
            }
        }
        remaining_block_ids = unreadable_block_ids;
        if remaining_block_ids.is_empty() {
            break;
        }
    }
    Ok(mentioned_user_ids_per_block)
}

/// Re-read the blocks with the user's token. A block mentioning the user is
/// saved, and notified when the mention is new. A block that no longer
/// mentions them (or is gone) deletes its notification.
#[allow(clippy::too_many_arguments)]
async fn save_mentions_for_connection(
    executor: &mut Transaction<'_, Postgres>,
    connection: &IntegrationConnection,
    provider_user_id: &str,
    block_ids: &[&String],
    notion_service: &Arc<NotionService>,
    notification_service: &Arc<RwLock<NotificationService>>,
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: &Arc<RwLock<ThirdPartyItemService>>,
) -> Result<(), UniversalInboxError> {
    let Some((access_token, _)) = integration_connection_service
        .read()
        .await
        .find_access_token_for_connection(executor, connection.id, connection.user_id)
        .await?
    else {
        return Ok(());
    };

    let mut cache = NotionObjectCache::default();
    for block_id in block_ids {
        let existing_block = third_party_item_service
            .read()
            .await
            .find_third_party_items_for_source_id(
                executor,
                ThirdPartyItemKind::NotionBlock,
                block_id,
                Some(connection.user_id),
            )
            .await?
            .into_iter()
            .find(|item| item.integration_connection_id == connection.id)
            .map(NotionBlock::try_from)
            .transpose()?;

        let Some(block) = notion_service
            .fetch_mention_block(&access_token, block_id, provider_user_id, &mut cache)
            .await?
        else {
            // Deleted, archived or no longer shared: nothing left to show
            if existing_block.is_some() {
                mark_notification_as_deleted(executor, block_id, connection, notification_service)
                    .await?;
            }
            continue;
        };

        let is_mentioned = block.mentions_user(provider_user_id);
        if is_mentioned
            && block
                .last_edited_by
                .as_ref()
                .is_some_and(|user| user.id == provider_user_id)
        {
            debug!("Notion block {block_id} was last edited by the mentioned user, ignoring it");
            continue;
        }
        let Some(existing_block) = existing_block else {
            if is_mentioned {
                save_block_and_notify(
                    executor,
                    block,
                    connection,
                    notion_service,
                    notification_service,
                    third_party_item_service,
                )
                .await?;
            }
            continue;
        };

        let was_mentioned = existing_block.mentions_user(provider_user_id);
        if is_mentioned && !was_mentioned {
            // Mentioned again after the mention was removed
            save_block_and_notify(
                executor,
                block,
                connection,
                notion_service,
                notification_service,
                third_party_item_service,
            )
            .await?;
            continue;
        }

        // An edit keeps the notification as the user left it
        third_party_item_service
            .read()
            .await
            .create_or_update_third_party_item(
                executor,
                Box::new(block.into_third_party_item(connection.user_id, connection.id)),
            )
            .await?;
        if !is_mentioned {
            mark_notification_as_deleted(executor, block_id, connection, notification_service)
                .await?;
        }
    }
    Ok(())
}

async fn save_block_and_notify(
    executor: &mut Transaction<'_, Postgres>,
    block: NotionBlock,
    connection: &IntegrationConnection,
    notion_service: &Arc<NotionService>,
    notification_service: &Arc<RwLock<NotificationService>>,
    third_party_item_service: &Arc<RwLock<ThirdPartyItemService>>,
) -> Result<(), UniversalInboxError> {
    let third_party_item = third_party_item_service
        .read()
        .await
        .create_or_update_third_party_item(
            executor,
            Box::new(block.into_third_party_item(connection.user_id, connection.id)),
        )
        .await?
        .value();
    notification_service
        .read()
        .await
        .create_notification_from_third_party_item::<NotionBlock, NotionService>(
            executor,
            *third_party_item,
            notion_service.clone(),
            connection.user_id,
        )
        .await?;
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
    for (connection_id, access_token) in
        find_triage_access_tokens(executor, event, integration_connection_service).await?
    {
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
                    { attr::INTEGRATION_CONNECTION_ID } = connection_id.to_string(),
                    "Notion comment triage skipped a connection: {error}"
                );
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    Ok(None)
}

/// The tokens of a few validated connections of the event's workspace, used
/// to read what the event is about before knowing whom it concerns.
async fn find_triage_access_tokens(
    executor: &mut Transaction<'_, Postgres>,
    event: &NotionWebhookEvent,
    integration_connection_service: &Arc<RwLock<IntegrationConnectionService>>,
) -> Result<Vec<(IntegrationConnectionId, AccessToken)>, UniversalInboxError> {
    let workspace_connections = integration_connection_service
        .read()
        .await
        .find_validated_integration_connections_per_provider_workspace(
            executor,
            IntegrationProviderKind::Notion,
            &event.workspace_id,
        )
        .await?;

    let mut access_tokens = vec![];
    for connection in workspace_connections.iter().take(MAX_TRIAGE_CONNECTIONS) {
        if let Some((access_token, _)) = integration_connection_service
            .read()
            .await
            .find_access_token_for_connection(executor, connection.id, connection.user_id)
            .await?
        {
            access_tokens.push((connection.id, access_token));
        }
    }
    Ok(access_tokens)
}

/// The connection is a validated Notion connection of the event's workspace,
/// syncing the event's feature (`is_syncing`), whose user can see the page.
fn is_concerned_connection(
    connection: &IntegrationConnection,
    event: &NotionWebhookEvent,
    accessible_person_ids: Option<&[&str]>,
    is_syncing: impl Fn(&NotionConfig) -> bool,
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
    if !is_syncing(config) || context.workspace_id != event.workspace_id {
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
    if let Some(existing_item) = existing_item {
        let existing_notification = notification_service
            .read()
            .await
            .get_notification_for_source_id(executor, discussion_id, connection.user_id)
            .await?;
        let tracked = NotionComment::try_from(existing_item)?;
        if is_unsubscribed_without_new_mention(
            existing_notification.as_ref(),
            &tracked,
            &discussion,
            &provider_user_id,
        ) {
            debug!("Ignoring a reply in an unsubscribed Notion discussion");
            return Ok(());
        }
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
    source_id: &str,
    connection: &IntegrationConnection,
    notification_service: &Arc<RwLock<NotificationService>>,
) -> Result<(), UniversalInboxError> {
    let service = notification_service.read().await;
    let Some(notification) = service
        .get_notification_for_source_id(executor, source_id, connection.user_id)
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
