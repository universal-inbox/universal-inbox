use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use graphql_client::{GraphQLQuery, Response};
use http::{HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    integration_connection::{IntegrationProvider, IntegrationProviderKind},
    notification::{
        integrations::linear::LinearNotification, Notification, NotificationDetails,
        NotificationSource, NotificationSourceKind,
    },
    user::UserId,
};

use crate::{
    integrations::{
        linear::graphql::{
            issue_update_subscribers, notification_archive, notification_subscribers_query,
            notification_update_snoozed_until_at, notifications_query, IssueUpdateSubscribers,
            NotificationArchive, NotificationSubscribersQuery, NotificationUpdateSnoozedUntilAt,
            NotificationsQuery,
        },
        notification::NotificationSourceService,
        oauth2::AccessToken,
        APP_USER_AGENT,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::graphql::assert_no_error_in_graphql_response,
};

pub mod graphql;

#[derive(Clone, Debug)]
pub struct LinearService {
    linear_graphql_url: String,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}

static LINEAR_GRAPHQL_URL: &str = "https://api.linear.app/graphql";
static LINEAR_GRAPHQL_API_NAME: &str = "Linear";

impl LinearService {
    pub fn new(
        linear_graphql_url: Option<String>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    ) -> Result<LinearService, UniversalInboxError> {
        Ok(LinearService {
            linear_graphql_url: linear_graphql_url
                .unwrap_or_else(|| LINEAR_GRAPHQL_URL.to_string()),
            integration_connection_service,
        })
    }

    pub async fn query_notifications(
        &self,
        access_token: &AccessToken,
    ) -> Result<notifications_query::ResponseData, UniversalInboxError> {
        let request_body = NotificationsQuery::build_query(notifications_query::Variables {});

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch notifications from Linear API")?
            .text()
            .await
            .context("Failed to fetch notifications response from Linear API")?;

        let notifications_response: Response<notifications_query::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&notifications_response, LINEAR_GRAPHQL_API_NAME)?;

        Ok(notifications_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?)
    }

    pub async fn query_notification_subscribers(
        &self,
        access_token: &AccessToken,
        notification_id: String,
    ) -> Result<notification_subscribers_query::ResponseData, UniversalInboxError> {
        let request_body =
            NotificationSubscribersQuery::build_query(notification_subscribers_query::Variables {
                id: notification_id,
            });

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch notification subscribers from Linear API")?
            .text()
            .await
            .context("Failed to fetch notification subscribers response from Linear API")?;

        let notification_response: Response<notification_subscribers_query::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&notification_response, LINEAR_GRAPHQL_API_NAME)?;

        Ok(notification_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?)
    }

    pub async fn archive_notification(
        &self,
        access_token: &AccessToken,
        notification_id: String,
    ) -> Result<(), UniversalInboxError> {
        let request_body = NotificationArchive::build_query(notification_archive::Variables {
            id: notification_id,
        });

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot delete notification from Linear API")?
            .text()
            .await
            .context("Failed to fetch notification archive response from Linear API")?;

        let archive_response: Response<notification_archive::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&archive_response, LINEAR_GRAPHQL_API_NAME)?;

        let response_data = archive_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?;

        if !response_data.notification_archive.success {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Linear API call failed with an unknown error"
            )));
        }

        Ok(())
    }

    pub async fn update_issue_subscribers(
        &self,
        access_token: &AccessToken,
        issue_id: String,
        subscriber_ids: Vec<String>,
    ) -> Result<(), UniversalInboxError> {
        let request_body =
            IssueUpdateSubscribers::build_query(issue_update_subscribers::Variables {
                id: issue_id,
                subscriber_ids,
            });

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot update issue subscribers from Linear API")?
            .text()
            .await
            .context("Failed to fetch issue update subscribers response from Linear API")?;

        let update_response: Response<issue_update_subscribers::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&update_response, LINEAR_GRAPHQL_API_NAME)?;

        let response_data = update_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?;

        if !response_data.issue_update.success {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Linear API call failed with an unknown error"
            )));
        }

        Ok(())
    }

    pub async fn update_notification_snoozed_until_at(
        &self,
        access_token: &AccessToken,
        issue_id: String,
        snoozed_until_at: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let request_body = NotificationUpdateSnoozedUntilAt::build_query(
            notification_update_snoozed_until_at::Variables {
                id: issue_id,
                snoozed_until_at,
            },
        );

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot snooze issue notification from Linear API")?
            .text()
            .await
            .context("Failed to fetch notification update snooze response from Linear API")?;

        let update_response: Response<notification_update_snoozed_until_at::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&update_response, LINEAR_GRAPHQL_API_NAME)?;

        let response_data = update_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?;

        if !response_data.notification_update.success {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Linear API call failed with an unknown error"
            )));
        }

        Ok(())
    }
}

fn build_linear_client(access_token: &AccessToken) -> Result<ClientWithMiddleware, reqwest::Error> {
    let mut headers = HeaderMap::new();

    let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    let reqwest_client = reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()?;
    Ok(ClientBuilder::new(reqwest_client)
        .with(TracingMiddleware::<SpanBackendWithUrl>::new())
        .build())
}

#[async_trait]
impl NotificationSourceService for LinearService {
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, None, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Linear notifications without an access token"))?;

        let notifications_response = self.query_notifications(&access_token).await?;

        TryInto::<Vec<LinearNotification>>::try_into(notifications_response).map(|linear_notifs| {
            linear_notifs
                .into_iter()
                .map(|linear_notif| linear_notif.into_notification(user_id))
                .collect()
        })
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn delete_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, None, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot delete Linear notification without an access token"))?;

        self.archive_notification(&access_token, source_id.to_string())
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn unsubscribe_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, None, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot unsubscribe from a Linear notification without an access token")
            })?;

        let subscribers_response = self
            .query_notification_subscribers(&access_token, source_id.to_string())
            .await?;

        // Only Issue notification have subscribers and can be unsubscribed
        if let notification_subscribers_query::ResponseData {
            notification: notification_subscribers_query::NotificationSubscribersQueryNotification::IssueNotification(
                notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotification {
                    user: notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotificationUser {
                        id: user_id
                    },
                    issue: notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotificationIssue {
                        subscribers: notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotificationIssueSubscribers {
                            nodes
                        }
                    }
                }
            )
        } = subscribers_response {
            let initial_subscribers_count = nodes.len();
            let subscriber_ids: Vec<String> = nodes
                .into_iter()
                .filter_map(|subscriber|
                            (subscriber.id != user_id).then_some(subscriber.id)
                ).collect();
            if initial_subscribers_count > subscriber_ids.len() {
                self
                    .update_issue_subscribers(&access_token, source_id.to_string(), subscriber_ids)
                    .await?;
            }
        }

        self.delete_notification_from_source(executor, source_id, user_id)
            .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn snooze_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        snoozed_until_at: DateTime<Utc>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, None, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Linear notifications without an access token"))?;

        self.update_notification_snoozed_until_at(
            &access_token,
            source_id.to_string(),
            snoozed_until_at,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self, _executor, _notification), fields(notification_id = _notification.id.to_string()), err)]
    async fn fetch_notification_details<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _notification: &Notification,
        _user_id: UserId,
    ) -> Result<Option<NotificationDetails>, UniversalInboxError> {
        // Linear notification details are fetch as part of the fetch_all_notifications call
        // all details are fetch in a single GraphQL call
        Ok(None)
    }
}

impl IntegrationProvider for LinearService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Linear
    }
}

impl NotificationSource for LinearService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Linear
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        true
    }
}
