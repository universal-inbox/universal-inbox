use std::sync::Arc;

use anyhow::{anyhow, Context, Error};
use async_trait::async_trait;
use chrono::{DateTime as ChronoDateTime, Utc};
use format_serde_error::SerdeError;
use graphql_client::{GraphQLQuery, Response};
use http::{HeaderMap, HeaderValue};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use universal_inbox::{
    integration_connection::{IntegrationProvider, IntegrationProviderKind},
    notification::{
        integrations::linear::{LinearIssue, LinearNotification, LinearProject},
        NotificationSource, NotificationSourceKind,
    },
    user::UserId,
};
use uuid::Uuid;

use crate::{
    integrations::{notification::NotificationSourceService, oauth2::AccessToken, APP_USER_AGENT},
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
};

type DateTime = ChronoDateTime<Utc>;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/schema.json",
    query_path = "src/integrations/linear/notifications_query.graphql",
    response_derives = "Debug,Clone,Serialize"
)]
pub struct NotificationsQuery;

impl TryFrom<notifications_query::ResponseData> for Vec<LinearNotification> {
    type Error = UniversalInboxError;

    fn try_from(value: notifications_query::ResponseData) -> Result<Self, Self::Error> {
        value
            .notifications
            .nodes
            .into_iter()
            .map(|notification| match notification {
                notifications_query::NotificationsQueryNotificationsNodes {
                    id,
                    type_,
                    read_at,
                    updated_at,
                    on: notifications_query::NotificationsQueryNotificationsNodesOn::IssueNotification(notifications_query::NotificationsQueryNotificationsNodesOnIssueNotification {
                        issue: notifications_query::NotificationsQueryNotificationsNodesOnIssueNotificationIssue {
                            id: issue_id,
                            title,
                            url,
                        },
                    }),
                } => Ok(Some(LinearNotification::IssueNotification {
                    id: Uuid::parse_str(&id).context(format!("Failed to parse UUID from `{id}`"))?,
                    r#type: type_,
                    read_at,
                    updated_at,
                    issue: LinearIssue {
                        id: Uuid::parse_str(&issue_id).context(format!("Failed to parse UUID from `{issue_id}`"))?,
                        title,
                        url: url.parse().context(format!("Failed to parse URL from `{url}`"))?,
                    },
                })),
                notifications_query::NotificationsQueryNotificationsNodes {
                    id,
                    type_,
                    read_at,
                    updated_at,
                    on: notifications_query::NotificationsQueryNotificationsNodesOn::ProjectNotification(notifications_query::NotificationsQueryNotificationsNodesOnProjectNotification {
                        project: notifications_query::NotificationsQueryNotificationsNodesOnProjectNotificationProject {
                            id: project_id,
                            name,
                            url,
                        },
                    }),
                } => Ok(Some(LinearNotification::ProjectNotification {
                    id: Uuid::parse_str(&id).context(format!("Failed to parse UUID from `{id}`"))?,
                    r#type: type_,
                    read_at,
                    updated_at,
                    project: LinearProject {
                        id: Uuid::parse_str(&project_id).context(format!("Failed to parse UUID from `{project_id}`"))?,
                        name,
                        url: url.parse().context(format!("Failed to parse URL from `{url}`"))?,
                    },
                })),
                // Ignoring any other type of notifications
                _ => Ok(None)
            })
            .filter_map(|linear_notification_result| linear_notification_result.transpose())
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct LinearService {
    linear_graphql_url: String,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}

static LINEAR_GRAPHQL_URL: &str = "https://api.linear.app/graphql";

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
                .map_err(|err| <SerdeError as Into<Error>>::into(SerdeError::new(response, err)))?;

        Ok(notifications_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?)
    }
}

fn build_linear_client(access_token: &AccessToken) -> Result<reqwest::Client, reqwest::Error> {
    let mut headers = HeaderMap::new();

    let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()
}

#[async_trait]
impl NotificationSourceService<LinearNotification> for LinearService {
    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<LinearNotification>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, None, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Linear notifications without an access token"))?;

        let notifications_response = self.query_notifications(&access_token).await?;

        notifications_response.try_into()
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn delete_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        todo!()
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    async fn unsubscribe_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_id: &str,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        todo!()
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
}
