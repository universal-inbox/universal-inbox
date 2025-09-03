use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{anyhow, Context};

use async_trait::async_trait;
use cached::{proc_macro::io_cached, Return};
use chrono::{DateTime, Timelike, Utc};
use serde_json::json;
use slack_blocks_render::{find_slack_references_in_blocks, SlackReferences};
use slack_morphism::{
    errors::{SlackClientApiError, SlackClientError},
    prelude::*,
};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, warn};
use url::Url;
use uuid::Uuid;
use vec1::Vec1;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{Notification, NotificationSource, NotificationSourceKind, NotificationStatus},
    task::{
        integrations::todoist::TODOIST_INBOX_PROJECT, service::TaskPatch,
        CreateOrUpdateTaskRequest, TaskCreationConfig, TaskSource, TaskSourceKind, TaskStatus,
    },
    third_party::{
        integrations::slack::{
            SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
            SlackImDetails, SlackMessageDetails, SlackMessageSenderDetails, SlackReaction,
            SlackReactionItem, SlackReactionState, SlackStar, SlackStarItem, SlackStarState,
            SlackThread,
        },
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    },
    user::UserId,
    utils::{default_value::DefaultValue, truncate::truncate_with_ellipse},
    HasHtmlUrl,
};

use crate::{
    integrations::{
        notification::ThirdPartyNotificationSourceService, task::ThirdPartyTaskService,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::cache::build_redis_cache,
};

static SLACK_BASE_URL: &str = "https://api.slack.com/api";

#[derive(Clone)]
pub struct SlackService {
    slack_base_url: String,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}

impl SlackService {
    pub fn new(
        slack_base_url: Option<String>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    ) -> Self {
        Self {
            slack_base_url: slack_base_url.unwrap_or_else(|| SLACK_BASE_URL.to_string()),
            integration_connection_service,
        }
    }

    pub async fn mock_all(mock_server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/stars.add"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(json!({ "ok": true })),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/stars.remove"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(json!({ "ok": true })),
            )
            .mount(mock_server)
            .await;
    }

    pub async fn get_chat_permalink(
        &self,
        channel: &SlackChannelId,
        message: &SlackTs,
        slack_api_token: &SlackApiToken,
    ) -> Result<Url, UniversalInboxError> {
        let result =
            cached_get_chat_permalink(&self.slack_base_url, slack_api_token, channel, message)
                .await?;
        if result.was_cached {
            debug!("`get_chat_permalink` cache hit");
        }
        Ok(result.value)
    }

    pub async fn fetch_message(
        &self,
        channel: &SlackChannelId,
        message: &SlackTs,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
    ) -> Result<SlackHistoryMessage, UniversalInboxError> {
        let result = cached_fetch_message(
            user_id,
            &self.slack_base_url,
            slack_api_token,
            channel,
            message,
        )
        .await?;
        if result.was_cached {
            debug!("`fetch_message` cache hit");
        }
        Ok(result.value)
    }

    pub async fn fetch_thread(
        &self,
        channel: &SlackChannelId,
        root_message: &SlackTs,
        current_message: &SlackTs,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
    ) -> Result<Vec1<SlackHistoryMessage>, UniversalInboxError> {
        let result = cached_fetch_thread(
            user_id,
            &self.slack_base_url,
            slack_api_token,
            channel,
            root_message,
            current_message,
        )
        .await?;
        if result.was_cached {
            debug!("`fetch_thread` cache hit");
        }
        result.value.try_into().map_err(|_| {
            UniversalInboxError::Unexpected(anyhow!(
                "A Slack thread must have at least one message"
            ))
        })
    }

    pub async fn fetch_channel(
        &self,
        channel: &SlackChannelId,
        slack_api_token: &SlackApiToken,
    ) -> Result<SlackChannelInfo, UniversalInboxError> {
        let result = cached_fetch_channel(&self.slack_base_url, slack_api_token, channel).await?;
        if result.was_cached {
            debug!("`fetch_channel` cache hit");
        }
        Ok(result.value)
    }

    pub async fn fetch_user(
        &self,
        user: &SlackUserId,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
    ) -> Result<SlackUser, UniversalInboxError> {
        let result =
            cached_fetch_user(user_id, &self.slack_base_url, slack_api_token, user).await?;
        if result.was_cached {
            debug!("`fetch_user` cache hit");
        }
        Ok(result.value)
    }

    pub async fn fetch_user_profile(
        &self,
        user: &SlackUserId,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
    ) -> Result<SlackUserProfile, UniversalInboxError> {
        let slack_user = self.fetch_user(user, user_id, slack_api_token).await?;

        Ok(slack_user.profile.unwrap_or_else(|| {
            let mut profile = SlackUserProfile::new().with_id(slack_user.id);

            if let Some(name) = slack_user.name {
                profile = profile.with_display_name(name);
            }
            if let Some(real_name) = slack_user.real_name {
                profile = profile.with_real_name(real_name);
            }
            if let Some(team) = slack_user.team_id {
                profile = profile.with_team(team);
            }

            profile
        }))
    }

    pub async fn fetch_usergroup(
        &self,
        usergroup_id: &SlackUserGroupId,
        _user_id: UserId,
        slack_api_token: &SlackApiToken,
    ) -> Result<SlackUserGroup, UniversalInboxError> {
        let result = cached_list_usergroups(&self.slack_base_url, slack_api_token).await?;
        if result.was_cached {
            debug!("`list_usergroups` cache hit");
        }
        result
            .value
            .iter()
            .find(|u| u.id == *usergroup_id)
            .cloned()
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Usergroup with id {} not found",
                    usergroup_id
                ))
            })
    }

    pub async fn list_users_in_usergroup(
        &self,
        usergroup_id: &SlackUserGroupId,
        slack_api_token: &SlackApiToken,
    ) -> Result<Vec<SlackUserId>, UniversalInboxError> {
        let result =
            cached_list_users_in_usergroup(&self.slack_base_url, usergroup_id, slack_api_token)
                .await?;
        if result.was_cached {
            debug!("`list_users_in_usergroup` cache hit");
        }
        Ok(result.value)
    }

    pub async fn fetch_bot(
        &self,
        bot: &SlackBotId,
        slack_api_token: &SlackApiToken,
    ) -> Result<SlackBotInfo, UniversalInboxError> {
        let result = cached_fetch_bot(&self.slack_base_url, slack_api_token, bot).await?;
        if result.was_cached {
            debug!("`fetch_bot` cache hit");
        }
        Ok(result.value)
    }

    pub async fn fetch_team(
        &self,
        team: &SlackTeamId,
        slack_api_token: &SlackApiToken,
    ) -> Result<SlackTeamInfo, UniversalInboxError> {
        let result = cached_fetch_team(&self.slack_base_url, slack_api_token, team).await?;
        if result.was_cached {
            debug!("`fetch_team` cache hit");
        }
        Ok(result.value)
    }

    pub async fn list_emojis(
        &self,
        slack_api_token: &SlackApiToken,
    ) -> Result<HashMap<SlackEmojiName, SlackEmojiRef>, UniversalInboxError> {
        let result = cached_list_emojis(&self.slack_base_url, slack_api_token).await?;
        if result.was_cached {
            debug!("`list_emojis` cache hit");
        }
        Ok(result.value)
    }

    pub async fn stars_add(
        &self,
        slack_api_token: &SlackApiToken,
        channel: Option<SlackChannelId>,
        message: Option<SlackTs>,
        file: Option<SlackFileId>,
        file_comment: Option<SlackFileCommentId>,
    ) -> Result<(), UniversalInboxError> {
        let client = SlackClient::new(
            SlackClientHyperConnector::with_connector(
                hyper_rustls::HttpsConnectorBuilder::new()
                    .with_native_roots()
                    .context("Failed to initialize new Slack client")?
                    .https_or_http()
                    .enable_http2()
                    .build(),
            )
            .with_slack_api_url(&self.slack_base_url),
        );
        let session = client.open_session(slack_api_token);

        let mut request = SlackApiStarsAddRequest::new();
        if let Some(channel) = channel {
            request = request.with_channel(channel.clone());
        }
        if let Some(message) = message {
            request = request.with_timestamp(message.clone());
        }
        if let Some(file) = file {
            request = request.with_file(file.clone());
        }
        if let Some(file_comment) = file_comment {
            request = request.with_file_comment(file_comment.clone());
        }

        session
            .stars_add(&request)
            .await
            .map(|_| ())
            .or_else(|e| match &e {
                SlackClientError::ApiError(SlackClientApiError { code, .. }) => {
                    if code == "already_starred" {
                        Ok(())
                    } else {
                        Err(e)
                    }
                }
                _ => Err(e),
            })
            .context("Failed to add Slack star")?;

        Ok(())
    }

    pub async fn stars_remove(
        &self,
        slack_api_token: &SlackApiToken,
        channel: Option<SlackChannelId>,
        message: Option<SlackTs>,
        file: Option<SlackFileId>,
        file_comment: Option<SlackFileCommentId>,
    ) -> Result<(), UniversalInboxError> {
        let client = SlackClient::new(
            SlackClientHyperConnector::with_connector(
                hyper_rustls::HttpsConnectorBuilder::new()
                    .with_native_roots()
                    .context("Failed to initialize new Slack client")?
                    .https_or_http()
                    .enable_http2()
                    .build(),
            )
            .with_slack_api_url(&self.slack_base_url),
        );
        let session = client.open_session(slack_api_token);

        let mut request = SlackApiStarsRemoveRequest::new();
        if let Some(channel) = channel {
            request = request.with_channel(channel.clone());
        }
        if let Some(message) = message {
            request = request.with_timestamp(message.clone());
        }
        if let Some(file) = file {
            request = request.with_file(file.clone());
        }
        if let Some(file_comment) = file_comment {
            request = request.with_file_comment(file_comment.clone());
        }

        session
            .stars_remove(&request)
            .await
            .context("Failed to remove Slack star")?;

        Ok(())
    }

    pub async fn reactions_add(
        &self,
        slack_api_token: &SlackApiToken,
        reaction: SlackReactionName,
        channel: SlackChannelId,
        message: SlackTs,
    ) -> Result<(), UniversalInboxError> {
        let client = SlackClient::new(
            SlackClientHyperConnector::with_connector(
                hyper_rustls::HttpsConnectorBuilder::new()
                    .with_native_roots()
                    .context("Failed to initialize new Slack client")?
                    .https_or_http()
                    .enable_http2()
                    .build(),
            )
            .with_slack_api_url(&self.slack_base_url),
        );
        let session = client.open_session(slack_api_token);

        let request = SlackApiReactionsAddRequest::new(channel, reaction, message);

        session
            .reactions_add(&request)
            .await
            .map(|_| ())
            .or_else(|e| match &e {
                SlackClientError::ApiError(SlackClientApiError { code, .. }) => {
                    if code == "already_reacted" {
                        Ok(())
                    } else {
                        Err(e)
                    }
                }
                _ => Err(e),
            })
            .context("Failed to add Slack reaction")?;

        Ok(())
    }

    pub async fn reactions_remove(
        &self,
        slack_api_token: &SlackApiToken,
        reaction: SlackReactionName,
        channel: SlackChannelId,
        message: SlackTs,
    ) -> Result<(), UniversalInboxError> {
        let client = SlackClient::new(
            SlackClientHyperConnector::with_connector(
                hyper_rustls::HttpsConnectorBuilder::new()
                    .with_native_roots()
                    .context("Failed to initialize new Slack client")?
                    .https_or_http()
                    .enable_http2()
                    .build(),
            )
            .with_slack_api_url(&self.slack_base_url),
        );

        let session = client.open_session(slack_api_token);

        let request = SlackApiReactionsRemoveRequest::new(reaction)
            .with_channel(channel)
            .with_timestamp(message);

        session
            .reactions_remove(&request)
            .await
            .context("Failed to remove Slack reaction")?;

        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    pub async fn fetch_item_from_event(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_push_event_callback: &SlackPushEventCallback,
        user_id: UserId,
    ) -> Result<Option<ThirdPartyItem>, UniversalInboxError> {
        match &slack_push_event_callback.event {
            SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { item, event_ts, .. }) => {
                self.fetch_item_from_slack_star(
                    executor,
                    slack_push_event_callback,
                    SlackStarState::StarAdded,
                    event_ts.to_date_time_opt().unwrap_or_else(Utc::now),
                    item,
                    user_id,
                )
                .await
            }
            SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent {
                item, event_ts, ..
            }) => {
                self.fetch_item_from_slack_star(
                    executor,
                    slack_push_event_callback,
                    SlackStarState::StarRemoved,
                    event_ts.to_date_time_opt().unwrap_or_else(Utc::now),
                    item,
                    user_id,
                )
                .await
            }
            SlackEventCallbackBody::ReactionAdded(SlackReactionAddedEvent {
                item,
                reaction,
                item_user: Some(item_user),
                event_ts,
                ..
            }) => {
                self.fetch_item_from_slack_reaction(
                    executor,
                    slack_push_event_callback,
                    SlackReactionState::ReactionAdded,
                    event_ts.to_date_time_opt().unwrap_or_else(Utc::now),
                    item,
                    item_user,
                    reaction,
                    user_id,
                )
                .await
            }
            SlackEventCallbackBody::ReactionRemoved(SlackReactionRemovedEvent {
                item,
                reaction,
                item_user: Some(item_user),
                event_ts,
                ..
            }) => {
                self.fetch_item_from_slack_reaction(
                    executor,
                    slack_push_event_callback,
                    SlackReactionState::ReactionRemoved,
                    event_ts.to_date_time_opt().unwrap_or_else(Utc::now),
                    item,
                    item_user,
                    reaction,
                    user_id,
                )
                .await
            }
            SlackEventCallbackBody::Message(SlackMessageEvent { origin, .. }) => {
                self.fetch_item_from_slack_message(
                    executor,
                    slack_push_event_callback,
                    origin,
                    user_id,
                )
                .await
            }
            // Not yet implemented resource type
            _ => Ok(None),
        }
    }

    async fn find_and_resolve_slack_references_in_message(
        &self,
        message_content: &SlackMessageContent,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
    ) -> Result<Option<SlackReferences>, UniversalInboxError> {
        let mut references = find_slack_references_in_message(message_content);

        if let Some(ref reactions) = message_content.reactions {
            let emojis_map = self.list_emojis(slack_api_token).await?;
            let reaction_emojis = reactions.iter().map(|r| SlackEmojiName(r.name.to_string()));
            for emoji in reaction_emojis {
                references
                    .emojis
                    .insert(emoji.clone(), emojis_map.get(&emoji).cloned());
            }
        }

        if references.is_empty() {
            return Ok(None);
        }

        let slack_user_ids = references.users.keys().cloned().collect::<Vec<_>>();
        for slack_user_id in slack_user_ids {
            let user_profile = self
                .fetch_user_profile(&slack_user_id, user_id, slack_api_token)
                .await?;
            let user_name = user_profile.display_name.or(user_profile.real_name);
            references.users.insert(slack_user_id, user_name);
        }

        let slack_usergroup_ids = references.usergroups.keys().cloned().collect::<Vec<_>>();
        for slack_usergroup_id in slack_usergroup_ids {
            let usergroup = self
                .fetch_usergroup(&slack_usergroup_id, user_id, slack_api_token)
                .await?;
            references
                .usergroups
                .insert(slack_usergroup_id.clone(), Some(usergroup.handle));
        }

        let slack_channel_ids = references.channels.keys().cloned().collect::<Vec<_>>();
        for slack_channel_id in slack_channel_ids {
            let channel = self
                .fetch_channel(&slack_channel_id, slack_api_token)
                .await?;
            references.channels.insert(slack_channel_id, channel.name);
        }

        let emojis_map = self.list_emojis(slack_api_token).await?;
        let emojis = references.emojis.keys().cloned().collect::<Vec<_>>();
        for emoji in emojis {
            references
                .emojis
                .insert(emoji.clone(), emojis_map.get(&emoji).cloned());
        }

        Ok(Some(references))
    }

    #[allow(clippy::blocks_in_conditions)]
    pub async fn fetch_item_from_slack_star(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_push_event_callback: &SlackPushEventCallback,
        slack_star_state: SlackStarState,
        slack_star_created_at: DateTime<Utc>,
        slack_star_item: &SlackStarsItem,
        user_id: UserId,
    ) -> Result<Option<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;

        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let slack_item = match slack_star_item {
            SlackStarsItem::Message(SlackStarsItemMessage {
                message:
                    SlackHistoryMessage {
                        origin: SlackMessageOrigin { ts, .. },
                        sender: SlackMessageSender { user, bot_id, .. },
                        ..
                    },
                channel,
                ..
            }) => {
                let url = self
                    .get_chat_permalink(channel, ts, &slack_api_token)
                    .await?;
                let sender = if let Some(slack_user_id) = user {
                    SlackMessageSenderDetails::User(Box::new(
                        self.fetch_user_profile(slack_user_id, user_id, &slack_api_token)
                            .await?,
                    ))
                } else if let Some(bot_id) = bot_id {
                    SlackMessageSenderDetails::Bot(Box::new(
                        self.fetch_bot(bot_id, &slack_api_token).await?,
                    ))
                } else {
                    return Err(UniversalInboxError::Unexpected(anyhow!(
                        "No user or bot found for Slack message {ts} in channel {channel}"
                    )));
                };

                let message = self
                    .fetch_message(channel, ts, user_id, &slack_api_token)
                    .await?;
                let channel = self.fetch_channel(channel, &slack_api_token).await?;
                let team = self
                    .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
                    .await?;
                let references = self
                    .find_and_resolve_slack_references_in_message(
                        &message.content,
                        user_id,
                        &slack_api_token,
                    )
                    .await
                    .inspect_err(|err| {
                        warn!(
                            "Failed to resolve Slack references in the message: {:?}",
                            err
                        )
                    })
                    .unwrap_or(None);

                SlackStarItem::SlackMessage(Box::new(SlackMessageDetails {
                    url,
                    message,
                    channel,
                    sender,
                    team,
                    references,
                }))
            }
            SlackStarsItem::File(SlackStarsItemFile {
                channel,
                file: SlackFile {
                    user, id, title, ..
                },
                ..
            }) => {
                let sender = if let Some(slack_user_id) = user {
                    Some(
                        self.fetch_user_profile(slack_user_id, user_id, &slack_api_token)
                            .await?,
                    )
                } else {
                    None
                };
                let channel = self.fetch_channel(channel, &slack_api_token).await?;
                let team = self
                    .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
                    .await?;

                SlackStarItem::SlackFile(Box::new(SlackFileDetails {
                    id: Some(id.clone()),
                    title: title.clone(),
                    channel,
                    sender,
                    team,
                }))
            }
            SlackStarsItem::FileComment(SlackStarsItemFileComment {
                channel,
                comment,
                file: SlackFile { user, .. },
                ..
            }) => {
                let sender = if let Some(slack_user_id) = user {
                    Some(
                        self.fetch_user_profile(slack_user_id, user_id, &slack_api_token)
                            .await?,
                    )
                } else {
                    None
                };
                let channel = self.fetch_channel(channel, &slack_api_token).await?;
                let team = self
                    .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
                    .await?;

                SlackStarItem::SlackFileComment(Box::new(SlackFileCommentDetails {
                    channel,
                    comment_id: comment.clone(),
                    sender,
                    team,
                }))
            }
            SlackStarsItem::Channel(SlackStarsItemChannel { channel, .. }) => {
                let channel = self.fetch_channel(channel, &slack_api_token).await?;
                let team = self
                    .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
                    .await?;

                SlackStarItem::SlackChannel(Box::new(SlackChannelDetails { channel, team }))
            }
            SlackStarsItem::Im(SlackStarsItemIm { channel, .. }) => {
                let channel = self.fetch_channel(channel, &slack_api_token).await?;
                let team = self
                    .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
                    .await?;

                SlackStarItem::SlackIm(Box::new(SlackImDetails { channel, team }))
            }
            SlackStarsItem::Group(SlackStarsItemGroup { group, .. }) => {
                let channel = self.fetch_channel(group, &slack_api_token).await?;
                let team = self
                    .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
                    .await?;

                SlackStarItem::SlackGroup(Box::new(SlackGroupDetails { channel, team }))
            }
        };

        Ok(Some(
            SlackStar {
                state: slack_star_state,
                created_at: slack_star_created_at,
                item: slack_item,
            }
            .into_third_party_item(user_id, integration_connection.id),
        ))
    }

    #[allow(clippy::blocks_in_conditions, clippy::too_many_arguments)]
    pub async fn fetch_item_from_slack_reaction(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_push_event_callback: &SlackPushEventCallback,
        slack_reaction_state: SlackReactionState,
        slack_reaction_created_at: DateTime<Utc>,
        slack_reaction_item: &SlackReactionsItem,
        slack_reaction_item_user_id: &SlackUserId,
        slack_reaction_name: &SlackReactionName,
        user_id: UserId,
    ) -> Result<Option<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;

        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let slack_reaction = match slack_reaction_item {
            SlackReactionsItem::Message(SlackHistoryMessage {
                origin:
                    SlackMessageOrigin {
                        ts,
                        channel: Some(channel),
                        ..
                    },
                ..
            }) => {
                let url = self
                    .get_chat_permalink(channel, ts, &slack_api_token)
                    .await?;
                let sender = SlackMessageSenderDetails::User(Box::new(
                    self.fetch_user_profile(slack_reaction_item_user_id, user_id, &slack_api_token)
                        .await?,
                ));
                let message = self
                    .fetch_message(channel, ts, user_id, &slack_api_token)
                    .await?;
                let channel = self.fetch_channel(channel, &slack_api_token).await?;
                let team = self
                    .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
                    .await?;
                let references = self
                    .find_and_resolve_slack_references_in_message(
                        &message.content,
                        user_id,
                        &slack_api_token,
                    )
                    .await
                    .inspect_err(|err| {
                        warn!(
                            "Failed to resolve Slack references in the message: {:?}",
                            err
                        )
                    })
                    .unwrap_or(None);

                SlackReaction {
                    name: slack_reaction_name.clone(),
                    state: slack_reaction_state,
                    created_at: slack_reaction_created_at,
                    item: SlackReactionItem::SlackMessage(SlackMessageDetails {
                        url,
                        message,
                        channel,
                        sender,
                        team,
                        references,
                    }),
                }
            }
            SlackReactionsItem::Message(SlackHistoryMessage {
                origin: SlackMessageOrigin { channel: None, .. },
                ..
            })
            | SlackReactionsItem::File(_) => return Ok(None),
        };

        Ok(Some(
            slack_reaction.into_third_party_item(user_id, integration_connection.id),
        ))
    }

    #[allow(clippy::blocks_in_conditions, clippy::too_many_arguments)]
    pub async fn fetch_item_from_slack_message(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_push_event_callback: &SlackPushEventCallback,
        origin: &SlackMessageOrigin,
        user_id: UserId,
    ) -> Result<Option<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;

        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let Some(channel_id) = &origin.channel else {
            return Ok(None);
        };
        let root_ts = origin.thread_ts.as_ref().unwrap_or(&origin.ts);
        let messages = self
            .fetch_thread(channel_id, root_ts, &origin.ts, user_id, &slack_api_token)
            .await?;
        let channel = self.fetch_channel(channel_id, &slack_api_token).await?;
        let team = self
            .fetch_team(&slack_push_event_callback.team_id, &slack_api_token)
            .await?;
        let sender_profiles = self
            .fetch_sender_profiles_from_messages(&slack_api_token, &messages, user_id)
            .await?;
        let thread_params = &messages.first().parent;
        let first_unread_message =
            SlackThread::first_unread_message_from_last_read(&thread_params.last_read, &messages);
        let url = self
            .get_chat_permalink(
                channel_id,
                &first_unread_message.origin.ts,
                &slack_api_token,
            )
            .await?;

        let mut references = SlackReferences::new();
        for message in messages.iter() {
            if let Some(refs) = self
                .find_and_resolve_slack_references_in_message(
                    &message.content,
                    user_id,
                    &slack_api_token,
                )
                .await
                .inspect_err(|err| {
                    warn!("Failed to resolve Slack references in the message: {err:?}")
                })
                .unwrap_or(None)
            {
                references.extend(refs);
            }
        }

        let slack_thread = SlackThread {
            url,
            subscribed: thread_params.subscribed.unwrap_or(true),
            last_read: thread_params.last_read.clone(),
            sender_profiles,
            messages,
            channel,
            team,
            references: Some(references),
        };

        Ok(Some(
            slack_thread.into_third_party_item(user_id, integration_connection.id),
        ))
    }

    async fn fetch_sender_profiles_from_messages(
        &self,
        slack_api_token: &SlackApiToken,
        messages: &[SlackHistoryMessage],
        user_id: UserId,
    ) -> Result<HashMap<String, SlackMessageSenderDetails>, UniversalInboxError> {
        let mut sender_profiles = HashMap::new();
        let mut slack_user_ids = Vec::new();
        let mut slack_bot_ids = Vec::new();
        for message in messages {
            match &message.sender {
                SlackMessageSender {
                    bot_id: Some(bot_id),
                    bot_profile: Some(bot_profile),
                    ..
                } => {
                    sender_profiles.insert(
                        bot_id.to_string(),
                        SlackMessageSenderDetails::Bot(Box::new(bot_profile.clone())),
                    );
                }
                SlackMessageSender {
                    bot_id: Some(bot_id),
                    ..
                } => {
                    slack_bot_ids.push(bot_id);
                }
                SlackMessageSender {
                    user: Some(slack_user_id),
                    user_profile: Some(user_profile),
                    ..
                } => {
                    sender_profiles.insert(
                        slack_user_id.to_string(),
                        SlackMessageSenderDetails::User(Box::new(user_profile.clone())),
                    );
                }
                SlackMessageSender {
                    user: Some(slack_user_id),
                    ..
                } => {
                    slack_user_ids.push(slack_user_id);
                }
                _ => {}
            }
        }

        for slack_user_id in slack_user_ids {
            if !sender_profiles.contains_key(slack_user_id.0.as_str()) {
                let user_profile = self
                    .fetch_user_profile(slack_user_id, user_id, slack_api_token)
                    .await
                    .with_context(|| format!("Failed to fetch user profile {slack_user_id}"))?;
                sender_profiles.insert(
                    slack_user_id.to_string(),
                    SlackMessageSenderDetails::User(Box::new(user_profile)),
                );
            }
        }

        for slack_bot_id in slack_bot_ids {
            if !sender_profiles.contains_key(slack_bot_id.0.as_str()) {
                let bot = self
                    .fetch_bot(slack_bot_id, slack_api_token)
                    .await
                    .with_context(|| format!("Failed to fetch bot profile {slack_bot_id}"))?;
                sender_profiles.insert(
                    slack_bot_id.to_string(),
                    SlackMessageSenderDetails::Bot(Box::new(bot)),
                );
            }
        }

        Ok(sender_profiles)
    }

    async fn delete_slack_star(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_star_item: &SlackStarItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;
        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let slack_star_ids = slack_star_item.ids();
        // ⚠️ For some reason, the star must be added before being removed
        // Maybe because it does not exists as a `star` but as `saved for later` in the Slack API
        // Nonetheless, the `stars.remove` method actually remove the `saved for later` from the message
        self.stars_add(
            &slack_api_token,
            slack_star_ids.channel_id.clone(),
            slack_star_ids.message_id.clone(),
            slack_star_ids.file_id.clone(),
            slack_star_ids.file_comment_id.clone(),
        )
        .await?;
        self.stars_remove(
            &slack_api_token,
            slack_star_ids.channel_id.clone(),
            slack_star_ids.message_id.clone(),
            slack_star_ids.file_id.clone(),
            slack_star_ids.file_comment_id.clone(),
        )
        .await?;

        Ok(())
    }

    async fn add_slack_star(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_star_item: &SlackStarItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;
        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let slack_star_ids = slack_star_item.ids();
        self.stars_add(
            &slack_api_token,
            slack_star_ids.channel_id.clone(),
            slack_star_ids.message_id.clone(),
            slack_star_ids.file_id.clone(),
            slack_star_ids.file_comment_id.clone(),
        )
        .await
    }

    async fn delete_slack_reaction(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_reaction_item: &SlackReactionItem,
        reaction_name: &SlackReactionName,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;
        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        if let Some(slack_reaction_ids) = slack_reaction_item.ids() {
            self.reactions_remove(
                &slack_api_token,
                reaction_name.clone(),
                slack_reaction_ids.channel_id.clone(),
                slack_reaction_ids.message_id.clone(),
            )
            .await?;
        }

        Ok(())
    }

    async fn add_slack_reaction(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        slack_reaction_item: &SlackReactionItem,
        reaction_name: &SlackReactionName,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;
        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        if let Some(slack_reaction_ids) = slack_reaction_item.ids() {
            self.reactions_add(
                &slack_api_token,
                reaction_name.clone(),
                slack_reaction_ids.channel_id.clone(),
                slack_reaction_ids.message_id.clone(),
            )
            .await?;
        }

        Ok(())
    }
}

#[io_cached(
    key = "String",
    // Use user_id to avoid leaking a message to an unauthorized user
    convert = r#"{ format!("{}__{}__{}__{}", slack_base_url, _user_id, channel, message) }"#,
    ty = "cached::AsyncRedisCache<String, SlackHistoryMessage>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_message`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_message", Duration::from_secs(60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_message(
    _user_id: UserId,
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    channel: &SlackChannelId,
    message: &SlackTs,
) -> Result<Return<SlackHistoryMessage>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let messages = session
        .conversations_replies(
            &SlackApiConversationsRepliesRequest::new(channel.clone(), message.clone())
                .with_latest(message.clone())
                .with_limit(1)
                .with_inclusive(true),
        )
        .await
        .with_context(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Failed to fetch Slack message {message} in channel {channel}"
            ))
        })?
        .messages;

    Ok(Return::new(
        messages
            .last()
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "No messages found for Slack message {message} in channel {channel}"
                ))
            })?
            .clone(),
    ))
}

#[io_cached(
    key = "String",
    // Use user_id to avoid leaking a message to an unauthorized user
    convert = r#"{ format!("{}__{}__{}__{}__{}", slack_base_url, _user_id, channel, root_message, current_message) }"#,
    ty = "cached::AsyncRedisCache<String, Vec<SlackHistoryMessage>>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_thread`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_thread", Duration::from_secs(60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_thread(
    _user_id: UserId,
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    channel: &SlackChannelId,
    root_message: &SlackTs,
    current_message: &SlackTs,
) -> Result<Return<Vec<SlackHistoryMessage>>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let messages = session
        .conversations_replies(
            &SlackApiConversationsRepliesRequest::new(channel.clone(), root_message.clone())
                .with_latest(current_message.clone())
                .with_inclusive(true),
        )
        .await
        .with_context(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Failed to fetch Slack thread {root_message} in channel {channel}"
            ))
        })?
        .messages;

    Ok(Return::new(messages))
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}__{}", slack_base_url, channel) }"#,
    ty = "cached::AsyncRedisCache<String, SlackChannelInfo>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_channel`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_channel", Duration::from_secs(24 * 60 * 60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_channel(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    channel: &SlackChannelId,
) -> Result<Return<SlackChannelInfo>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .conversations_info(&SlackApiConversationsInfoRequest::new(channel.clone()))
        .await
        .with_context(|| format!("Failed to fetch Slack channel {channel}"))?;

    Ok(Return::new(response.channel))
}

#[io_cached(
    key = "String",
    // Use user_id to avoid leaking user details to an unauthorized user
    convert = r#"{ format!("{}__{}__{}", slack_base_url, _user_id, user) }"#,
    ty = "cached::AsyncRedisCache<String, SlackUser>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_user`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_user", Duration::from_secs(24 * 60 * 60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_user(
    _user_id: UserId,
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    user: &SlackUserId,
) -> Result<Return<SlackUser>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .users_info(&SlackApiUsersInfoRequest::new(user.clone()))
        .await
        .with_context(|| format!("Failed to fetch Slack user {user}"))?;

    Ok(Return::new(response.user))
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}", slack_base_url) }"#,
    ty = "cached::AsyncRedisCache<String, Vec<SlackUserGroup>>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `list_usergroups`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:list_usergroups", Duration::from_secs(12 * 60 * 60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_list_usergroups(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
) -> Result<Return<Vec<SlackUserGroup>>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .usergroups_list(&SlackApiUserGroupsListRequest::new())
        .await
        .with_context(|| "Failed to fetch Slack usergroups".to_string())?;

    Ok(Return::new(response.usergroups))
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}__{}", slack_base_url, usergroup_id) }"#,
    ty = "cached::AsyncRedisCache<String, Vec<SlackUserId>>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `list_users_in_usergroup`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:list_users_in_usergroup", Duration::from_secs(12 * 60 * 60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_list_users_in_usergroup(
    slack_base_url: &str,
    usergroup_id: &SlackUserGroupId,
    slack_api_token: &SlackApiToken,
) -> Result<Return<Vec<SlackUserId>>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .usergroups_users_list(&SlackApiUserGroupsUsersListRequest::new(
            usergroup_id.clone(),
        ))
        .await
        .with_context(|| "Failed to fetch Slack users in usergroup".to_string())?;

    Ok(Return::new(response.users))
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}__{}", slack_base_url, bot) }"#,
    ty = "cached::AsyncRedisCache<String, SlackBotInfo>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_bot`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_bot", Duration::from_secs(24 * 60 * 60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_bot(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    bot: &SlackBotId,
) -> Result<Return<SlackBotInfo>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .bots_info(&SlackApiBotsInfoRequest::new().with_bot(bot.to_string()))
        .await
        .with_context(|| format!("Failed to fetch Slack bot {bot}"))?;

    Ok(Return::new(response.bot))
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}__{}", slack_base_url, team) }"#,
    ty = "cached::AsyncRedisCache<String, SlackTeamInfo>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_team`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_team", Duration::from_secs(24 * 60 * 60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_team(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    team: &SlackTeamId,
) -> Result<Return<SlackTeamInfo>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .team_info(&SlackApiTeamInfoRequest::new().with_team(team.clone()))
        .await
        .with_context(|| format!("Failed to fetch Slack team {team}"))?;

    Ok(Return::new(response.team))
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}", slack_base_url) }"#,
    ty = "cached::AsyncRedisCache<String, HashMap<SlackEmojiName, SlackEmojiRef>>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `list_emojis`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:list_emojis", Duration::from_secs(24 * 60 * 60), false).await }"##,
    with_cached_flag = true
)]
async fn cached_list_emojis(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
) -> Result<Return<HashMap<SlackEmojiName, SlackEmojiRef>>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .emoji_list()
        .await
        .context("Failed to fetch Slack emojis")?;

    Ok(Return::new(response.emoji))
}

#[io_cached(
    key = "String",
    convert = r#"{ format!("{}__{}__{}", slack_base_url, channel, message) }"#,
    ty = "cached::AsyncRedisCache<String, Url>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `get_chat_permalink`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:get_chat_permalink", Duration::from_secs(7 * 24 * 60 * 60), true).await }"##,
    with_cached_flag = true
)]
async fn cached_get_chat_permalink(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    channel: &SlackChannelId,
    message: &SlackTs,
) -> Result<Return<Url>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperConnector::with_connector(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .context("Failed to initialize new Slack client")?
                .https_or_http()
                .enable_http2()
                .build(),
        )
        .with_slack_api_url(slack_base_url),
    );

    let session = client.open_session(slack_api_token);

    let response = session
        .chat_get_permalink(&SlackApiChatGetPermalinkRequest::new(
            channel.clone(),
            message.clone(),
        ))
        .await
        .with_context(|| {
            format!("Failed to get Slack chat permalink for message {message} in channel {channel}")
        })?;

    Ok(Return::new(response.permalink))
}

impl TaskSource for SlackService {
    fn get_task_source_kind(&self) -> TaskSourceKind {
        TaskSourceKind::Slack
    }
}

impl IntegrationProviderSource for SlackService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Slack
    }
}

impl NotificationSource for SlackService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Slack
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<SlackStar> for SlackService {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_third_party_item.source_id,
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn third_party_item_into_notification(
        &self,
        source: &SlackStar,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let status = match source.state {
            SlackStarState::StarAdded => NotificationStatus::Unread,
            SlackStarState::StarRemoved => NotificationStatus::Deleted,
        };

        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.item.render_title(),
            status,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::Slack,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn delete_notification_from_source(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackStar(slack_star) = &source_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                source_item.kind()
            )));
        };

        self.delete_slack_star(executor, &slack_star.item, user_id)
            .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackStar(slack_star) = &source_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                source_item.kind()
            )));
        };

        self.delete_slack_star(executor, &slack_star.item, user_id)
            .await
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Slack stars cannot be snoozed from the API => no-op
        Ok(())
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<SlackReaction> for SlackService {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_third_party_item.source_id,
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn third_party_item_into_notification(
        &self,
        source: &SlackReaction,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let status = match source.state {
            SlackReactionState::ReactionAdded => NotificationStatus::Unread,
            SlackReactionState::ReactionRemoved => NotificationStatus::Deleted,
        };

        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.item.render_title(),
            status,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::Slack,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn delete_notification_from_source(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackReaction(slack_reaction) = &source_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                source_item.kind()
            )));
        };

        self.delete_slack_reaction(
            executor,
            &slack_reaction.item,
            &slack_reaction.name,
            user_id,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackReaction(slack_reaction) = &source_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                source_item.kind()
            )));
        };

        self.delete_slack_reaction(
            executor,
            &slack_reaction.item,
            &slack_reaction.name,
            user_id,
        )
        .await
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Slack stars cannot be snoozed from the API => no-op
        Ok(())
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<SlackThread> for SlackService {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source_third_party_item.source_id,
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn third_party_item_into_notification(
        &self,
        source: &SlackThread,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        let last_message = &source.messages.last();
        let status = if !source.subscribed {
            NotificationStatus::Unsubscribed
        } else {
            match &source.last_read {
                Some(last_read) if *last_read == last_message.origin.ts => {
                    NotificationStatus::Deleted
                }
                _ => NotificationStatus::Unread,
            }
        };

        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.render_title(),
            status,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::Slack,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            user.id = _user_id.to_string()
        ),
        err
    )]
    async fn delete_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // There is no way to mark a Slack thread as read with public API
        // For message in the channel, the read mark can be updated but will mark as
        // read all messages before the given timestamp.
        // This might not be what we want for now, perhaps later with an option
        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_item.id.to_string(),
            user.id = _user_id.to_string()
        ),
        err
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // The is no way to unsubscribe from a Slack thread with the public API
        Ok(())
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Slack messages cannot be snoozed from the API => no-op
        Ok(())
    }
}

#[async_trait]
impl ThirdPartyTaskService<SlackStar> for SlackService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source.item.id(),
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn third_party_item_into_task(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source: &SlackStar,
        source_third_party_item: &ThirdPartyItem,
        task_creation_config: Option<TaskCreationConfig>,
        user_id: UserId,
    ) -> Result<Box<CreateOrUpdateTaskRequest>, UniversalInboxError> {
        let task_creation_config = task_creation_config.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Cannot build a Slack task without a task creation"
            ))
        })?;
        let status = match source.state {
            SlackStarState::StarAdded => TaskStatus::Active,
            SlackStarState::StarRemoved => TaskStatus::Done,
        };
        let created_at = source.created_at;
        let updated_at = source.created_at;
        let title = format!(
            "[{}]({})",
            source.item.render_title(),
            source.get_html_url()
        );
        let body = truncate_with_ellipse(&source.item.render_content(), 16300, "...", false);
        let completed_at = if status == TaskStatus::Done {
            Some(Utc::now())
        } else {
            None
        };

        Ok(Box::new(CreateOrUpdateTaskRequest {
            id: Uuid::new_v4().into(),
            title,
            body,
            status,
            completed_at,
            priority: task_creation_config.priority,
            due_at: DefaultValue::new(task_creation_config.due_at.clone(), None),
            tags: vec![],
            parent_id: None,
            project: DefaultValue::new(
                task_creation_config
                    .project_name
                    .clone()
                    .unwrap_or_else(|| TODOIST_INBOX_PROJECT.to_string()),
                None,
            ),
            is_recurring: false,
            created_at,
            updated_at,
            kind: TaskSourceKind::Slack,
            source_item: source_third_party_item.clone(),
            sink_item: None,
            user_id,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn delete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        <SlackService as ThirdPartyTaskService<SlackStar>>::complete_task::<'_, '_, '_, '_, '_>(
            self,
            executor,
            third_party_item,
            user_id,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn complete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackStar(slack_star) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                third_party_item.kind()
            )));
        };

        self.delete_slack_star(executor, &slack_star.item, user_id)
            .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn uncomplete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackStar(slack_star) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                third_party_item.kind()
            )));
        };
        self.add_slack_star(executor, &slack_star.item, user_id)
            .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_task(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _id: &str,
        _patch: &TaskPatch,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // There is nothing to update in Slack tasks
        Ok(())
    }
}

#[async_trait]
impl ThirdPartyTaskService<SlackReaction> for SlackService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            source_id = source.item.id(),
            third_party_item_id = source_third_party_item.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn third_party_item_into_task(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        source: &SlackReaction,
        source_third_party_item: &ThirdPartyItem,
        task_creation_config: Option<TaskCreationConfig>,
        user_id: UserId,
    ) -> Result<Box<CreateOrUpdateTaskRequest>, UniversalInboxError> {
        let task_creation_config = task_creation_config.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Cannot build a Slack task without a task creation"
            ))
        })?;
        let status = match source.state {
            SlackReactionState::ReactionAdded => TaskStatus::Active,
            SlackReactionState::ReactionRemoved => TaskStatus::Done,
        };
        let created_at = source.created_at;
        let updated_at = source.created_at;
        let title = format!(
            "[{}]({})",
            source.item.render_title(),
            source.get_html_url()
        );
        let body = truncate_with_ellipse(&source.item.render_content(), 16300, "...", false);
        let completed_at = if status == TaskStatus::Done {
            Some(Utc::now())
        } else {
            None
        };

        Ok(Box::new(CreateOrUpdateTaskRequest {
            id: Uuid::new_v4().into(),
            title,
            body,
            status,
            completed_at,
            priority: task_creation_config.priority,
            due_at: DefaultValue::new(task_creation_config.due_at.clone(), None),
            tags: vec![],
            parent_id: None,
            project: DefaultValue::new(
                task_creation_config
                    .project_name
                    .clone()
                    .unwrap_or_else(|| TODOIST_INBOX_PROJECT.to_string()),
                None,
            ),
            is_recurring: false,
            created_at,
            updated_at,
            kind: TaskSourceKind::Slack,
            source_item: source_third_party_item.clone(),
            sink_item: None,
            user_id,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn delete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        <SlackService as ThirdPartyTaskService<SlackReaction>>::complete_task::<'_, '_, '_, '_, '_>(
            self,
            executor,
            third_party_item,
            user_id,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn complete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackReaction(slack_reaction) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                third_party_item.kind()
            )));
        };

        self.delete_slack_reaction(
            executor,
            &slack_reaction.item,
            &slack_reaction.name,
            user_id,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn uncomplete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let ThirdPartyItemData::SlackReaction(slack_reaction) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                third_party_item.kind()
            )));
        };

        self.add_slack_reaction(
            executor,
            &slack_reaction.item,
            &slack_reaction.name,
            user_id,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_task(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _id: &str,
        _patch: &TaskPatch,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // There is nothing to update in Slack tasks
        Ok(())
    }
}

pub fn find_slack_references_in_message(message_content: &SlackMessageContent) -> SlackReferences {
    let mut references = if let Some(blocks) = &message_content.blocks {
        find_slack_references_in_blocks(blocks)
    } else {
        SlackReferences::default()
    };

    if let Some(attachements) = &message_content.attachments {
        for attachment in attachements {
            if let Some(blocks) = &attachment.blocks {
                references.extend(find_slack_references_in_blocks(blocks));
            }
        }
    }

    references
}

pub fn has_slack_references_in_message(message_content: &SlackMessageContent) -> bool {
    !find_slack_references_in_message(message_content).is_empty()
}
