use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cached::{proc_macro::io_cached, Return};
use chrono::{DateTime, Utc};
use slack_morphism::{
    errors::{SlackClientApiError, SlackClientError},
    prelude::*,
};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::debug;
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{
        integrations::slack::{
            SlackChannelDetails, SlackFileCommentDetails, SlackFileDetails, SlackGroupDetails,
            SlackImDetails, SlackMessageDetails, SlackMessageSenderDetails,
        },
        Notification, NotificationDetails, NotificationMetadata, NotificationSource,
        NotificationSourceKind,
    },
    task::{service::TaskPatch, Task, TaskCreation, TaskSource, TaskSourceKind, TaskStatus},
    third_party::{
        integrations::slack::{SlackStar, SlackStarState, SlackStarredItem},
        item::{
            ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemSource, ThirdPartyItemSourceKind,
        },
    },
    user::UserId,
    utils::{emoji::replace_emoji_code_in_string_with_emoji, truncate::truncate_with_ellipse},
    HasHtmlUrl,
};

use crate::{
    integrations::{notification::NotificationSourceService, task::ThirdPartyTaskService},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::cache::build_redis_cache,
};

static SLACK_BASE_URL: &str = "https://api.slack.com/api";

#[derive(Clone, Debug)]
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

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn get_chat_permalink(
        &self,
        slack_api_token: &SlackApiToken,
        channel: &SlackChannelId,
        message: &SlackTs,
    ) -> Result<Url, UniversalInboxError> {
        let result =
            cached_get_chat_permalink(&self.slack_base_url, slack_api_token, channel, message)
                .await?;
        if result.was_cached {
            debug!("`get_chat_permalink` cache hit");
        }
        Ok(result.value)
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn fetch_message(
        &self,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
        channel: &SlackChannelId,
        message: &SlackTs,
        thread_message: &Option<SlackTs>,
    ) -> Result<SlackHistoryMessage, UniversalInboxError> {
        let result = cached_fetch_message(
            user_id,
            &self.slack_base_url,
            slack_api_token,
            channel,
            message,
            thread_message,
        )
        .await?;
        if result.was_cached {
            debug!("`fetch_message` cache hit");
        }
        Ok(result.value)
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn fetch_channel(
        &self,
        slack_api_token: &SlackApiToken,
        channel: &SlackChannelId,
    ) -> Result<SlackChannelInfo, UniversalInboxError> {
        let result = cached_fetch_channel(&self.slack_base_url, slack_api_token, channel).await?;
        if result.was_cached {
            debug!("`fetch_channel` cache hit");
        }
        Ok(result.value)
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn fetch_user(
        &self,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
        user: &SlackUserId,
    ) -> Result<SlackUser, UniversalInboxError> {
        let result =
            cached_fetch_user(user_id, &self.slack_base_url, slack_api_token, user).await?;
        if result.was_cached {
            debug!("`fetch_user` cache hit");
        }
        Ok(result.value)
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn fetch_bot(
        &self,
        slack_api_token: &SlackApiToken,
        bot: &SlackBotId,
    ) -> Result<SlackBotInfo, UniversalInboxError> {
        let result = cached_fetch_bot(&self.slack_base_url, slack_api_token, bot).await?;
        if result.was_cached {
            debug!("`fetch_bot` cache hit");
        }
        Ok(result.value)
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn fetch_team(
        &self,
        slack_api_token: &SlackApiToken,
        team: &SlackTeamId,
    ) -> Result<SlackTeamInfo, UniversalInboxError> {
        let result = cached_fetch_team(&self.slack_base_url, slack_api_token, team).await?;
        if result.was_cached {
            debug!("`fetch_team` cache hit");
        }
        Ok(result.value)
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn stars_add(
        &self,
        slack_api_token: &SlackApiToken,
        channel: Option<SlackChannelId>,
        message: Option<SlackTs>,
        file: Option<SlackFileId>,
        file_comment: Option<SlackFileCommentId>,
    ) -> Result<(), UniversalInboxError> {
        let client = SlackClient::new(
            SlackClientHyperHttpsConnector::new()
                .context("Failed to initialize new Slack client")?
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

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn stars_remove(
        &self,
        slack_api_token: &SlackApiToken,
        channel: Option<SlackChannelId>,
        message: Option<SlackTs>,
        file: Option<SlackFileId>,
        file_comment: Option<SlackFileCommentId>,
    ) -> Result<(), UniversalInboxError> {
        let client = SlackClient::new(
            SlackClientHyperHttpsConnector::new()
                .context("Failed to initialize new Slack client")?
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

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, notification),
        fields(notification_id = notification.id.0.to_string()),
        err
    )]
    pub async fn undelete_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
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
        let NotificationMetadata::Slack(ref slack_push_event_callback) = notification.metadata
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Given notification must have been built from a Slack notification"
            )));
        };
        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let (channel, message, file, file_comment) = match &slack_push_event_callback.event {
            SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { item, .. })
            | SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { item, .. }) => match item
            {
                SlackStarsItem::Message(SlackStarsItemMessage {
                    message:
                        SlackHistoryMessage {
                            origin: SlackMessageOrigin { ts, .. },
                            ..
                        },
                    channel,
                    ..
                }) => (Some(channel.clone()), Some(ts.clone()), None, None),
                SlackStarsItem::File(SlackStarsItemFile {
                    channel,
                    file: SlackFile { id, .. },
                    ..
                }) => (Some(channel.clone()), None, Some(id.clone()), None),
                SlackStarsItem::FileComment(SlackStarsItemFileComment {
                    channel, comment, ..
                }) => (Some(channel.clone()), None, None, Some(comment.clone())),
                SlackStarsItem::Channel(SlackStarsItemChannel { channel, .. }) => {
                    (Some(channel.clone()), None, None, None)
                }
                SlackStarsItem::Im(SlackStarsItemIm { channel, .. }) => {
                    (Some(channel.clone()), None, None, None)
                }
                SlackStarsItem::Group(SlackStarsItemGroup { group, .. }) => {
                    (Some(group.clone()), None, None, None)
                }
            },
            // Not yet implemented resource type
            _ => return Ok(()),
        };

        self.stars_add(&slack_api_token, channel, message, file, file_comment)
            .await?;

        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, slack_push_event_callback), err)]
    pub async fn fetch_item_from_event<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        slack_push_event_callback: &SlackPushEventCallback,
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

        let (slack_star_state, slack_star_created_at, slack_star_item) =
            match &slack_push_event_callback.event {
                SlackEventCallbackBody::StarAdded(SlackStarAddedEvent {
                    item, event_ts, ..
                }) => (
                    SlackStarState::StarAdded,
                    event_ts.to_date_time_opt().unwrap_or_else(Utc::now),
                    item,
                ),
                SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent {
                    item,
                    event_ts,
                    ..
                }) => (
                    SlackStarState::StarRemoved,
                    event_ts.to_date_time_opt().unwrap_or_else(Utc::now),
                    item,
                ),
                // Not yet implemented resource type
                _ => return Ok(None),
            };

        let (slack_starred_item, source_id) = match slack_star_item {
            SlackStarsItem::Message(SlackStarsItemMessage {
                message:
                    SlackHistoryMessage {
                        origin: SlackMessageOrigin { ts, thread_ts, .. },
                        sender: SlackMessageSender { user, bot_id, .. },
                        ..
                    },
                channel,
                ..
            }) => {
                let url = self
                    .get_chat_permalink(&slack_api_token, channel, ts)
                    .await?;
                let sender = if let Some(slack_user_id) = user {
                    SlackMessageSenderDetails::User(Box::new(
                        self.fetch_user(user_id, &slack_api_token, slack_user_id)
                            .await?,
                    ))
                } else if let Some(bot_id) = bot_id {
                    SlackMessageSenderDetails::Bot(self.fetch_bot(&slack_api_token, bot_id).await?)
                } else {
                    return Err(UniversalInboxError::Unexpected(anyhow!(
                        "No user or bot found for Slack message {ts} in channel {channel}"
                    )));
                };

                let message = self
                    .fetch_message(user_id, &slack_api_token, channel, ts, thread_ts)
                    .await?;
                let channel = self.fetch_channel(&slack_api_token, channel).await?;
                let team = self
                    .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                    .await?;

                (
                    SlackStarredItem::SlackMessage(SlackMessageDetails {
                        url,
                        message,
                        channel,
                        sender,
                        team,
                    }),
                    ts.to_string(),
                )
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
                        self.fetch_user(user_id, &slack_api_token, slack_user_id)
                            .await?,
                    )
                } else {
                    None
                };
                let channel = self.fetch_channel(&slack_api_token, channel).await?;
                let team = self
                    .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                    .await?;
                (
                    SlackStarredItem::SlackFile(SlackFileDetails {
                        id: Some(id.clone()),
                        title: title.clone(),
                        channel,
                        sender,
                        team,
                    }),
                    id.to_string(),
                )
            }
            SlackStarsItem::FileComment(SlackStarsItemFileComment {
                channel,
                comment,
                file: SlackFile { user, .. },
                ..
            }) => {
                let sender = if let Some(slack_user_id) = user {
                    Some(
                        self.fetch_user(user_id, &slack_api_token, slack_user_id)
                            .await?,
                    )
                } else {
                    None
                };
                let channel = self.fetch_channel(&slack_api_token, channel).await?;
                let team = self
                    .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                    .await?;
                (
                    SlackStarredItem::SlackFileComment(SlackFileCommentDetails {
                        channel,
                        comment_id: comment.clone(),
                        sender,
                        team,
                    }),
                    comment.to_string(),
                )
            }
            SlackStarsItem::Channel(SlackStarsItemChannel { channel, .. }) => {
                let channel = self.fetch_channel(&slack_api_token, channel).await?;
                let team = self
                    .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                    .await?;
                let source_id = channel.id.to_string();
                (
                    SlackStarredItem::SlackChannel(SlackChannelDetails { channel, team }),
                    source_id,
                )
            }
            SlackStarsItem::Im(SlackStarsItemIm { channel, .. }) => {
                let channel = self.fetch_channel(&slack_api_token, channel).await?;
                let team = self
                    .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                    .await?;
                let source_id = channel.id.to_string();
                (
                    SlackStarredItem::SlackIm(SlackImDetails { channel, team }),
                    source_id,
                )
            }
            SlackStarsItem::Group(SlackStarsItemGroup { group, .. }) => {
                let channel = self.fetch_channel(&slack_api_token, group).await?;
                let team = self
                    .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                    .await?;
                let source_id = channel.id.to_string();
                (
                    SlackStarredItem::SlackGroup(SlackGroupDetails { channel, team }),
                    source_id,
                )
            }
        };

        Ok(Some(ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id,
            data: ThirdPartyItemData::SlackStar(SlackStar {
                state: slack_star_state,
                created_at: slack_star_created_at,
                starred_item: slack_starred_item,
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            user_id,
            integration_connection_id: integration_connection.id,
        }))
    }
}

#[io_cached(
    key = "String",
    // Use user_id to avoid leaking a message to an unauthorized user
    convert = r#"{ format!("{}__{}__{}__{}", slack_base_url, _user_id, channel, message) }"#,
    type = "cached::AsyncRedisCache<String, SlackHistoryMessage>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_message`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_message", 60).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_message(
    _user_id: UserId,
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    channel: &SlackChannelId,
    message: &SlackTs,
    thread_message: &Option<SlackTs>,
) -> Result<Return<SlackHistoryMessage>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperHttpsConnector::new()
            .context("Failed to initialize new Slack client")?
            .with_slack_api_url(slack_base_url),
    );
    let session = client.open_session(slack_api_token);

    let messages = if let Some(thread_message) = thread_message {
        session
            .conversations_replies(
                &SlackApiConversationsRepliesRequest::new(channel.clone(), thread_message.clone())
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
            .messages
    } else {
        session
            .conversations_history(
                &SlackApiConversationsHistoryRequest::new()
                    .with_channel(channel.clone())
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
            .messages
    };

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
    convert = r#"{ format!("{}__{}", slack_base_url, channel) }"#,
    type = "cached::AsyncRedisCache<String, SlackChannelInfo>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_channel`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_channel", 24 * 60 * 60).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_channel(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    channel: &SlackChannelId,
) -> Result<Return<SlackChannelInfo>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperHttpsConnector::new()
            .context("Failed to initialize new Slack client")?
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
    type = "cached::AsyncRedisCache<String, SlackUser>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_user`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_user", 24 * 60 * 60).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_user(
    _user_id: UserId,
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    user: &SlackUserId,
) -> Result<Return<SlackUser>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperHttpsConnector::new()
            .context("Failed to initialize new Slack client")?
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
    convert = r#"{ format!("{}__{}", slack_base_url, bot) }"#,
    type = "cached::AsyncRedisCache<String, SlackBotInfo>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_bot`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_bot", 24 * 60 * 60).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_bot(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    bot: &SlackBotId,
) -> Result<Return<SlackBotInfo>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperHttpsConnector::new()
            .context("Failed to initialize new Slack client")?
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
    type = "cached::AsyncRedisCache<String, SlackTeamInfo>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `fetch_team`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:fetch_team", 24 * 60 * 60).await }"##,
    with_cached_flag = true
)]
async fn cached_fetch_team(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    team: &SlackTeamId,
) -> Result<Return<SlackTeamInfo>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperHttpsConnector::new()
            .context("Failed to initialize new Slack client")?
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
    convert = r#"{ format!("{}__{}__{}", slack_base_url, channel, message) }"#,
    type = "cached::AsyncRedisCache<String, Url>",
    map_error = r##"|e| UniversalInboxError::Unexpected(anyhow!("Failed to cache Slack `get_chat_permalink`: {:?}", e))"##,
    create = r##" { build_redis_cache("slack:get_chat_permalink", 7 * 24 * 60 * 60).await }"##,
    with_cached_flag = true
)]
async fn cached_get_chat_permalink(
    slack_base_url: &str,
    slack_api_token: &SlackApiToken,
    channel: &SlackChannelId,
    message: &SlackTs,
) -> Result<Return<Url>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperHttpsConnector::new()
            .context("Failed to initialize new Slack client")?
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

impl ThirdPartyItemSource for SlackService {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::Slack
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
impl NotificationSourceService for SlackService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, notification),
        fields(notification_id = notification.id.0.to_string()),
        err
    )]
    async fn delete_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
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
        let NotificationMetadata::Slack(ref slack_push_event_callback) = notification.metadata
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Given notification must have been built from a Slack notification"
            )));
        };
        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let (channel, message, file, file_comment) = match &slack_push_event_callback.event {
            SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { item, .. })
            | SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { item, .. }) => match item
            {
                SlackStarsItem::Message(SlackStarsItemMessage {
                    message:
                        SlackHistoryMessage {
                            origin: SlackMessageOrigin { ts, .. },
                            ..
                        },
                    channel,
                    ..
                }) => (Some(channel.clone()), Some(ts.clone()), None, None),
                SlackStarsItem::File(SlackStarsItemFile {
                    channel,
                    file: SlackFile { id, .. },
                    ..
                }) => (Some(channel.clone()), None, Some(id.clone()), None),
                SlackStarsItem::FileComment(SlackStarsItemFileComment {
                    channel, comment, ..
                }) => (Some(channel.clone()), None, None, Some(comment.clone())),
                SlackStarsItem::Channel(SlackStarsItemChannel { channel, .. }) => {
                    (Some(channel.clone()), None, None, None)
                }
                SlackStarsItem::Im(SlackStarsItemIm { channel, .. }) => {
                    (Some(channel.clone()), None, None, None)
                }
                SlackStarsItem::Group(SlackStarsItemGroup { group, .. }) => {
                    (Some(group.clone()), None, None, None)
                }
            },
            // Not yet implemented resource type
            _ => return Ok(()),
        };

        // ⚠️ For some reason, the star must be added before being removed
        // Maybe because it does not exists as a `star` but as `saved for later` in the Slack API
        // Nonetheless, the `stars.remove` method actually remove the `saved for later` from the message
        self.stars_add(
            &slack_api_token,
            channel.clone(),
            message.clone(),
            file.clone(),
            file_comment.clone(),
        )
        .await?;
        self.stars_remove(&slack_api_token, channel, message, file, file_comment)
            .await?;
        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.0.to_string()), err)]
    async fn unsubscribe_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        self.delete_notification_from_source(executor, notification, user_id)
            .await
    }

    async fn snooze_notification_from_source<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _notification: &Notification,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Slack stars cannot be snoozed from the API => no-op
        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.0.to_string()), err)]
    async fn fetch_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
        user_id: UserId,
    ) -> Result<Option<NotificationDetails>, UniversalInboxError> {
        // Will soon be deprecated as NotificationDetails will become SlackStarredItem
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Slack notification details without an access token")
            })?;

        let NotificationMetadata::Slack(ref slack_push_event_callback) = notification.metadata
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Given notification must have been built from a Slack notification"
            )));
        };

        let slack_api_token = SlackApiToken::new(SlackApiTokenValue(access_token.to_string()));

        let notification_details = match &slack_push_event_callback.event {
            SlackEventCallbackBody::StarAdded(SlackStarAddedEvent { item, .. })
            | SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent { item, .. }) => match item
            {
                SlackStarsItem::Message(SlackStarsItemMessage {
                    message:
                        SlackHistoryMessage {
                            origin: SlackMessageOrigin { ts, thread_ts, .. },
                            sender: SlackMessageSender { user, bot_id, .. },
                            ..
                        },
                    channel,
                    ..
                }) => {
                    let url = self
                        .get_chat_permalink(&slack_api_token, channel, ts)
                        .await?;
                    let sender = if let Some(slack_user_id) = user {
                        SlackMessageSenderDetails::User(Box::new(
                            self.fetch_user(user_id, &slack_api_token, slack_user_id)
                                .await?,
                        ))
                    } else if let Some(bot_id) = bot_id {
                        SlackMessageSenderDetails::Bot(
                            self.fetch_bot(&slack_api_token, bot_id).await?,
                        )
                    } else {
                        return Err(UniversalInboxError::Unexpected(anyhow!(
                            "No user or bot found for Slack message {ts} in channel {channel}"
                        )));
                    };

                    let message = self
                        .fetch_message(user_id, &slack_api_token, channel, ts, thread_ts)
                        .await?;
                    let channel = self.fetch_channel(&slack_api_token, channel).await?;
                    let team = self
                        .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                        .await?;

                    NotificationDetails::SlackMessage(SlackMessageDetails {
                        url,
                        message,
                        channel,
                        sender,
                        team,
                    })
                }
                SlackStarsItem::File(SlackStarsItemFile {
                    channel,
                    file:
                        SlackFile {
                            id, user, title, ..
                        },
                    ..
                }) => {
                    let sender = if let Some(slack_user_id) = user {
                        Some(
                            self.fetch_user(user_id, &slack_api_token, slack_user_id)
                                .await?,
                        )
                    } else {
                        None
                    };
                    let channel = self.fetch_channel(&slack_api_token, channel).await?;
                    let team = self
                        .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                        .await?;
                    NotificationDetails::SlackFile(SlackFileDetails {
                        id: Some(id.clone()),
                        title: title.clone(),
                        channel,
                        sender,
                        team,
                    })
                }
                SlackStarsItem::FileComment(SlackStarsItemFileComment {
                    channel,
                    comment,
                    file: SlackFile { user, .. },
                    ..
                }) => {
                    let sender = if let Some(slack_user_id) = user {
                        Some(
                            self.fetch_user(user_id, &slack_api_token, slack_user_id)
                                .await?,
                        )
                    } else {
                        None
                    };
                    let channel = self.fetch_channel(&slack_api_token, channel).await?;
                    let team = self
                        .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                        .await?;
                    NotificationDetails::SlackFileComment(SlackFileCommentDetails {
                        channel,
                        comment_id: comment.clone(),
                        sender,
                        team,
                    })
                }
                SlackStarsItem::Channel(SlackStarsItemChannel { channel, .. }) => {
                    let channel = self.fetch_channel(&slack_api_token, channel).await?;
                    let team = self
                        .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                        .await?;
                    NotificationDetails::SlackChannel(SlackChannelDetails { channel, team })
                }
                SlackStarsItem::Im(SlackStarsItemIm { channel, .. }) => {
                    let channel = self.fetch_channel(&slack_api_token, channel).await?;
                    let team = self
                        .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                        .await?;
                    NotificationDetails::SlackIm(SlackImDetails { channel, team })
                }
                SlackStarsItem::Group(SlackStarsItemGroup { group, .. }) => {
                    let channel = self.fetch_channel(&slack_api_token, group).await?;
                    let team = self
                        .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                        .await?;
                    NotificationDetails::SlackGroup(SlackGroupDetails { channel, team })
                }
            },
            // Not yet implemented resource type
            _ => return Ok(None),
        };

        Ok(Some(notification_details))
    }
}

#[async_trait]
impl ThirdPartyTaskService<SlackStar> for SlackService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, _executor, source, source_third_party_item),
        fields(source_id = source.starred_item.id()),
        err
    )]
    async fn third_party_item_into_task<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        source: &SlackStar,
        source_third_party_item: &ThirdPartyItem,
        task_creation: Option<TaskCreation>,
        user_id: UserId,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let task_creation = task_creation.ok_or_else(|| {
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
        let title_with_emojis =
            replace_emoji_code_in_string_with_emoji(&source.starred_item.title());
        let title = truncate_with_ellipse(&title_with_emojis, 50, "...");
        let body = format!("- [{}]({})", title, source.get_html_url());
        let completed_at = if status == TaskStatus::Done {
            Some(Utc::now())
        } else {
            None
        };

        Ok(Box::new(Task {
            id: Uuid::new_v4().into(),
            title,
            body,
            status,
            completed_at,
            priority: task_creation.priority,
            due_at: task_creation.due_at.clone(),
            tags: vec![],
            parent_id: None,
            project: task_creation.project.name.clone(),
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
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    async fn delete_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        self.complete_task(executor, third_party_item, user_id)
            .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    async fn complete_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: &ThirdPartyItem,
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

        let ThirdPartyItemData::SlackStar(slack_star) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                third_party_item.kind()
            )));
        };
        let slack_star_ids = slack_star.starred_item.ids();
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

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    async fn uncomplete_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: &ThirdPartyItem,
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

        let ThirdPartyItemData::SlackStar(slack_star) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Expected Slack third party item but was {}",
                third_party_item.kind()
            )));
        };
        let slack_star_ids = slack_star.starred_item.ids();

        self.stars_add(
            &slack_api_token,
            slack_star_ids.channel_id.clone(),
            slack_star_ids.message_id.clone(),
            slack_star_ids.file_id.clone(),
            slack_star_ids.file_comment_id.clone(),
        )
        .await?;

        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, _executor, _id, _patch, _user_id), err)]
    async fn update_task<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _id: &str,
        _patch: &TaskPatch,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // There is nothing to update in Slack tasks
        Ok(())
    }
}
