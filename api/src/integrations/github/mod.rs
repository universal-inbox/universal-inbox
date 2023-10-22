use std::sync::Arc;

use actix_http::uri::Authority;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::{self, TryStreamExt};
use graphql_client::GraphQLQuery;
use http::{HeaderMap, HeaderValue, Uri};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;

use universal_inbox::{
    integration_connection::{IntegrationProvider, IntegrationProviderKind},
    notification::{
        integrations::github::{GithubNotification, GithubUri},
        Notification, NotificationDetails, NotificationMetadata, NotificationSource,
        NotificationSourceKind,
    },
    user::UserId,
};

use crate::{
    integrations::{
        github::graphql::{pull_request_query, PullRequestQuery},
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

pub fn get_html_url_from_api_url(api_url: &Option<Uri>) -> Option<Uri> {
    api_url.as_ref().and_then(|uri| {
        if uri.host() == Some("api.github.com") && uri.path().starts_with("/repos") {
            let mut uri_parts = uri.clone().into_parts();
            uri_parts.authority = Some(Authority::from_static("github.com"));
            uri_parts.path_and_query = uri_parts
                .path_and_query
                .and_then(|pq| pq.as_str().trim_start_matches("/repos").parse().ok());
            return Uri::from_parts(uri_parts).ok();
        }
        None
    })
}

#[async_trait]
impl NotificationSourceService for GithubService {
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
        let Some(ref resource_url) = github_notification.subject.url else {
            return Ok(None);
        };

        let resource_response = match GithubUri::try_from_api_uri(resource_url) {
            Ok(GithubUri::PullRequest {
                owner,
                repository,
                number,
            }) => {
                self.query_pull_request(owner, repository, number, &access_token)
                    .await?
            }
            // Not yet implemented resource type
            Err(_) => return Ok(None),
        };

        Ok(Some(resource_response.try_into()?))
    }
}

impl IntegrationProvider for GithubService {
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    mod get_html_url_from_api_url {
        use super::*;

        #[rstest]
        #[case(
            "https://api.github.com/repos/octokit/octokit.rb/issues/123",
            "https://github.com/octokit/octokit.rb/issues/123"
        )]
        #[case(
            "https://api.github.com/repos/octokit/octokit.rb/pulls/123",
            "https://github.com/octokit/octokit.rb/pulls/123"
        )]
        fn test_get_html_url_from_api_url_with_valid_api_url(
            #[case] api_url: &str,
            #[case] expected_html_url: &str,
        ) {
            assert_eq!(
                get_html_url_from_api_url(&Some(api_url.parse::<Uri>().unwrap())),
                Some(expected_html_url.parse::<Uri>().unwrap())
            );
        }

        #[rstest]
        fn test_get_html_url_from_api_url_with_invalid_github_api_url(
            #[values(
                None,
                Some("https://api.github.com/octokit/octokit.rb/issues/123"),
                Some("https://github.com/repos/octokit/octokit.rb/issues/123"),
                Some("https://github.com/octokit/octokit.rb/issues/123"),
                Some("https://google.com")
            )]
            api_url: Option<&str>,
        ) {
            assert_eq!(
                get_html_url_from_api_url(&api_url.map(|url| url.parse::<Uri>().unwrap())),
                None
            );
        }
    }
}