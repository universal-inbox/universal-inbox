use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cached::{proc_macro::io_cached, Return};
use chrono::{DateTime, Utc};
use slack_morphism::{
    api::{
        SlackApiBotsInfoRequest, SlackApiConversationsHistoryRequest,
        SlackApiConversationsInfoRequest, SlackApiTeamInfoRequest, SlackApiUsersInfoRequest,
    },
    events::{SlackEventCallbackBody, SlackStarAddedEvent, SlackStarRemovedEvent},
    hyper_tokio::SlackClientHyperHttpsConnector,
    SlackApiToken, SlackApiTokenValue, SlackBotId, SlackBotInfo, SlackChannelId, SlackChannelInfo,
    SlackClient, SlackFile, SlackHistoryMessage, SlackMessageOrigin, SlackMessageSender,
    SlackStarsItem, SlackStarsItemChannel, SlackStarsItemFile, SlackStarsItemFileComment,
    SlackStarsItemGroup, SlackStarsItemIm, SlackStarsItemMessage, SlackTeamId, SlackTeamInfo,
    SlackTs, SlackUser, SlackUserId,
};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::debug;

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
    user::UserId,
};

use crate::{
    integrations::notification::NotificationSourceService,
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

    pub async fn fetch_message(
        &self,
        user_id: UserId,
        slack_api_token: &SlackApiToken,
        channel: &SlackChannelId,
        message: &SlackTs,
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
) -> Result<Return<SlackHistoryMessage>, UniversalInboxError> {
    let client = SlackClient::new(
        SlackClientHyperHttpsConnector::new()
            .context("Failed to initialize new Slack client")?
            .with_slack_api_url(slack_base_url),
    );
    let session = client.open_session(slack_api_token);

    let response = session
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
        })?;

    Ok(Return::new(
        response
            .messages
            .first()
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
impl NotificationSourceService for SlackService {
    async fn delete_notification_from_source<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _source_id: &str,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        todo!()
    }

    async fn unsubscribe_notification_from_source<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _source_id: &str,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        todo!()
    }

    async fn snooze_notification_from_source<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _source_id: &str,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        todo!()
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.0.to_string()), err)]
    async fn fetch_notification_details<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
        user_id: UserId,
    ) -> Result<Option<NotificationDetails>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Slack, None, user_id)
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
                            origin: SlackMessageOrigin { ts, .. },
                            sender: SlackMessageSender { user, bot_id, .. },
                            ..
                        },
                    channel,
                    ..
                }) => {
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
                        .fetch_message(user_id, &slack_api_token, channel, ts)
                        .await?;
                    let channel = self.fetch_channel(&slack_api_token, channel).await?;
                    let team = self
                        .fetch_team(&slack_api_token, &slack_push_event_callback.team_id)
                        .await?;

                    NotificationDetails::SlackMessage(SlackMessageDetails {
                        message,
                        channel,
                        sender,
                        team,
                    })
                }
                SlackStarsItem::File(SlackStarsItemFile {
                    channel,
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
                    NotificationDetails::SlackFile(SlackFileDetails {
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
                        comment: comment.to_string(),
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
