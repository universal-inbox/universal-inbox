use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::{self, TryStreamExt};
use graphql_client::GraphQLQuery;
use http::{HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{
        integrations::github::{GithubNotification, GithubUrl},
        Notification, NotificationDetails, NotificationMetadata, NotificationSource,
        NotificationSourceKind,
    },
    user::UserId,
};

use crate::{
    integrations::{
        github::graphql::{
            discussions_search_query, pull_request_query, DiscussionsSearchQuery, PullRequestQuery,
        },
        notification::{NotificationSourceService, NotificationSyncSourceService},
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
pub struct GithubService {
    github_base_url: String,
    github_graphql_url: String,
    page_size: usize,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}

static GITHUB_BASE_URL: &str = "https://api.github.com";
static GITHUB_GRAPHQL_API_NAME: &str = "Github";

impl GithubService {
    pub fn new(
        github_base_url: Option<String>,
        page_size: usize,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    ) -> Result<GithubService, UniversalInboxError> {
        Ok(GithubService {
            github_base_url: github_base_url
                .clone()
                .unwrap_or_else(|| GITHUB_BASE_URL.to_string()),
            github_graphql_url: format!(
                "{}/graphql",
                github_base_url.unwrap_or_else(|| GITHUB_BASE_URL.to_string())
            ),
            page_size,
            integration_connection_service,
        })
    }

    pub async fn fetch_notifications(
        &self,
        page: u32,
        per_page: usize,
        access_token: &AccessToken,
    ) -> Result<Vec<GithubNotification>, UniversalInboxError> {
        let url = format!(
            "{}/notifications?page={page}&per_page={per_page}",
            self.github_base_url
        );
        let response = build_github_rest_client(access_token)
            .context("Failed to build Github client")?
            .get(&url)
            .send()
            .await
            .context("Cannot fetch notifications from Github API")?
            .text()
            .await
            .context("Failed to fetch notifications response from Github API")?;

        let notifications: Vec<GithubNotification> = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        Ok(notifications)
    }

    pub async fn mark_thread_as_read(
        &self,
        thread_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        let response = build_github_rest_client(access_token)
            .context("Failed to build Github client")?
            .patch(&format!(
                "{}/notifications/threads/{thread_id}",
                self.github_base_url
            ))
            .send()
            .await
            .with_context(|| format!("Failed to mark Github notification `{thread_id}` as read"))?;

        match response.error_for_status() {
            Ok(_) => Ok(()),
            Err(err) if err.status() == Some(reqwest::StatusCode::NOT_FOUND) => Ok(()),
            Err(error) => {
                tracing::error!("An error occurred when trying to mark Github notification `{thread_id}` as read: {}", error);
                Err(UniversalInboxError::Unexpected(anyhow!(
                    "Failed to mark Github notification `{thread_id}` as read"
                )))
            }
        }
    }

    pub async fn unsubscribe_from_thread(
        &self,
        thread_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        let response = build_github_rest_client(access_token)
            .context("Failed to build Github client")?
            .put(&format!(
                "{}/notifications/threads/{thread_id}/subscription",
                self.github_base_url
            ))
            .body(r#"{"ignored": true}"#)
            .send()
            .await
            .with_context(|| {
                format!("Failed to unsubscribe from Github notification `{thread_id}`")
            })?;

        match response.error_for_status() {
            Ok(_) => Ok(()),
            Err(err) if err.status() == Some(reqwest::StatusCode::NOT_FOUND) => Ok(()),
            Err(error) => {
                tracing::error!("An error occurred when trying to unsubscribe from Github notification `{thread_id}`: {}", error);
                Err(UniversalInboxError::Unexpected(anyhow!(
                    "Failed to unsubscribe from Github notification `{thread_id}`"
                )))
            }
        }
    }

    pub async fn query_pull_request(
        &self,
        owner: String,
        repository: String,
        pr_number: i64,
        access_token: &AccessToken,
    ) -> Result<pull_request_query::ResponseData, UniversalInboxError> {
        let request_body = PullRequestQuery::build_query(pull_request_query::Variables {
            owner,
            repository,
            pr_number,
        });

        let response = build_github_graphql_client(access_token)
            .context("Failed to build Github client")?
            .post(&self.github_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch pull request from Github graphql API")?
            .text()
            .await
            .context("Failed to fetch pull request response from Github graphql API")?;

        let pull_request_response: graphql_client::Response<pull_request_query::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&pull_request_response, GITHUB_GRAPHQL_API_NAME)?;

        Ok(pull_request_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Github graphql response"))?)
    }

    pub async fn search_discussion(
        &self,
        repository: &str,
        title: &str,
        access_token: &AccessToken,
    ) -> Result<discussions_search_query::ResponseData, UniversalInboxError> {
        let search_query = format!("repo:{repository} \"{title}\"");
        let request_body =
            DiscussionsSearchQuery::build_query(discussions_search_query::Variables {
                search_query,
            });

        let response = build_github_graphql_client(access_token)
            .context("Failed to build Github client")?
            .post(&self.github_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch discussion from Github graphql API")?
            .text()
            .await
            .context("Failed to fetch dicussion response from Github graphql API")?;

        let discussions_search_response: graphql_client::Response<
            discussions_search_query::ResponseData,
        > = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&discussions_search_response, GITHUB_GRAPHQL_API_NAME)?;

        Ok(discussions_search_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Github graphql response"))?)
    }
}

fn build_github_rest_client(
    access_token: &AccessToken,
) -> Result<ClientWithMiddleware, reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Accept",
        HeaderValue::from_static("application/vnd.github.v3+json"),
    );

    build_github_client(access_token, headers)
}

fn build_github_graphql_client(
    access_token: &AccessToken,
) -> Result<ClientWithMiddleware, reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Accept",
        HeaderValue::from_static("application/vnd.github.merge-info-preview+json"),
    );

    build_github_client(access_token, headers)
}

fn build_github_client(
    access_token: &AccessToken,
    mut headers: HeaderMap,
) -> Result<ClientWithMiddleware, reqwest::Error> {
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
impl NotificationSyncSourceService for GithubService {
    #[allow(clippy::blocks_in_conditions)]
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
            .find_access_token(executor, IntegrationProviderKind::Github, None, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Github notifications without an access token"))?;

        Ok(stream::try_unfold(
            (1, false, access_token),
            |(page, stop, access_token)| async move {
                if stop {
                    return Ok(None);
                }

                let response = self
                    .fetch_notifications(page, self.page_size, &access_token)
                    .await;

                response.map(|github_notifs| {
                    let notifs_count = github_notifs.len();
                    let is_last_page = notifs_count < self.page_size;
                    Some((github_notifs, (page + 1, is_last_page, access_token)))
                })
            },
        )
        .try_collect::<Vec<Vec<GithubNotification>>>()
        .await?
        .into_iter()
        .flatten()
        .map(|github_notif| github_notif.into_notification(user_id))
        .collect())
    }
}

#[async_trait]
impl NotificationSourceService for GithubService {
    #[allow(clippy::blocks_in_conditions)]
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
            .find_access_token(executor, IntegrationProviderKind::Github, None, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot delete Github notification without an access token"))?;

        self.mark_thread_as_read(source_id, &access_token).await
    }

    #[allow(clippy::blocks_in_conditions)]
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
            .find_access_token(executor, IntegrationProviderKind::Github, None, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot unsubscribe from Github notifications without an access token")
            })?;

        self.mark_thread_as_read(source_id, &access_token).await?;
        self.unsubscribe_from_thread(source_id, &access_token).await
    }

    async fn snooze_notification_from_source<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _source_id: &str,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Github notifications cannot be snoozed => no-op
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
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Github, None, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Github notification details without an access token")
            })?;

        let NotificationMetadata::Github(ref github_notification) = notification.metadata else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Given notification must have been built from a Github notification"
            )));
        };

        let notification_details = if let Some(ref resource_url) = github_notification.subject.url {
            match GithubUrl::try_from_api_url(resource_url) {
                Ok(GithubUrl::PullRequest {
                    owner,
                    repository,
                    number,
                }) => self
                    .query_pull_request(owner, repository, number, &access_token)
                    .await?
                    .try_into()?,
                // Not yet implemented resource type
                Err(_) => return Ok(None),
            }
        } else {
            match github_notification.subject.r#type.as_str() {
                "Discussion" => self
                    .search_discussion(
                        &github_notification.repository.full_name,
                        &github_notification.subject.title,
                        &access_token,
                    )
                    .await?
                    .try_into()?,
                _ => return Ok(None),
            }
        };

        Ok(Some(notification_details))
    }
}

impl IntegrationProviderSource for GithubService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Github
    }
}

impl NotificationSource for GithubService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Github
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}
