use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use futures::stream::{self, TryStreamExt};
use graphql_client::{GraphQLQuery, Response};
use http::{HeaderMap, HeaderValue};
use notification::RawGithubNotification;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Extension};
use reqwest_tracing::{
    DisableOtelPropagation, OtelPathNames, SpanBackendWithUrl, TracingMiddleware,
};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use url::Url;
use uuid::Uuid;
use wiremock::{
    matchers::{body_partial_json, method, path_regex},
    Mock, MockServer, ResponseTemplate,
};

use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{Notification, NotificationSource, NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::github::{GithubNotification, GithubNotificationItem, GithubUrl},
        item::{ThirdPartyItem, ThirdPartyItemFromSource, ThirdPartyItemSourceKind},
    },
    user::UserId,
};

use crate::{
    integrations::{
        github::graphql::{
            discussion_query, pull_request_query, DiscussionQuery, PullRequestQuery,
        },
        notification::ThirdPartyNotificationSourceService,
        oauth2::AccessToken,
        third_party::ThirdPartyItemSourceService,
        APP_USER_AGENT,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::graphql::assert_no_error_in_graphql_response,
};

pub mod graphql;
pub mod notification;

#[derive(Clone)]
pub struct GithubService {
    github_base_url: String,
    github_base_path: String,
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
        let github_base_url = github_base_url.unwrap_or_else(|| GITHUB_BASE_URL.to_string());
        let github_base_path = Url::parse(&github_base_url)
            .context("Cannot parse Github base URL")?
            .path()
            .to_string();
        let github_graphql_url = format!("{}/graphql", github_base_url);

        Ok(GithubService {
            github_base_url,
            github_base_path: if &github_base_path == "/" {
                "".to_string()
            } else {
                github_base_path
            },
            github_graphql_url,
            page_size,
            integration_connection_service,
        })
    }

    pub async fn mock_all(mock_server: &MockServer) {
        Mock::given(method("GET"))
            .and(path_regex("/notifications"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(json!([])),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_partial_json(
                json!({ "operationName": "PullRequestQuery" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&Response::<pull_request_query::ResponseData> {
                        data: None,
                        errors: None,
                        extensions: None,
                    }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_partial_json(
                json!({ "operationName": "DiscussionQuery" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&Response::<discussion_query::ResponseData> {
                        data: None,
                        errors: None,
                        extensions: None,
                    }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("PATCH"))
            .and(path_regex("/notifications/threads/.*"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("content-type", "application/json"),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("PUT"))
            .and(path_regex("/notifications/threads/[^/]*/subscription"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("content-type", "application/json"),
            )
            .mount(mock_server)
            .await;
    }

    fn build_github_rest_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ClientWithMiddleware, UniversalInboxError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );

        self.build_github_client(access_token, headers)
    }

    fn build_github_graphql_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ClientWithMiddleware, UniversalInboxError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github.merge-info-preview+json"),
        );

        self.build_github_client(access_token, headers)
    }

    fn build_github_client(
        &self,
        access_token: &AccessToken,
        mut headers: HeaderMap,
    ) -> Result<ClientWithMiddleware, UniversalInboxError> {
        let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);

        let reqwest_client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(APP_USER_AGENT)
            .build()
            .context("Cannot build Github client")?;
        Ok(ClientBuilder::new(reqwest_client)
            .with_init(Extension(
                OtelPathNames::known_paths([
                    format!(
                        "{}/notifications/threads/{{thread_id}}/subscription",
                        self.github_base_path
                    ),
                    format!(
                        "{}/notifications/threads/{{thread_id}}",
                        self.github_base_path
                    ),
                    format!("{}/notifications", self.github_base_path),
                    format!("{}/graphql", self.github_base_path),
                ])
                .context("Cannot build Otel path names")?,
            ))
            .with_init(Extension(DisableOtelPropagation))
            .with(TracingMiddleware::<SpanBackendWithUrl>::new())
            .build())
    }

    pub async fn fetch_notifications(
        &self,
        page: u32,
        per_page: usize,
        access_token: &AccessToken,
    ) -> Result<Vec<RawGithubNotification>, UniversalInboxError> {
        let url = format!(
            "{}/notifications?page={page}&per_page={per_page}",
            self.github_base_url
        );
        let response = self
            .build_github_rest_client(access_token)?
            .get(&url)
            .send()
            .await
            .context("Cannot fetch notifications from Github API")?
            .text()
            .await
            .context("Failed to fetch notifications response from Github API")?;

        let notifications: Vec<RawGithubNotification> = serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        Ok(notifications)
    }

    pub async fn mark_thread_as_read(
        &self,
        thread_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        let response = self
            .build_github_rest_client(access_token)?
            .patch(format!(
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
        let response = self
            .build_github_rest_client(access_token)?
            .put(format!(
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

        let response = self
            .build_github_graphql_client(access_token)?
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

    pub async fn query_discussion(
        &self,
        owner: String,
        repository: String,
        discussion_number: i64,
        access_token: &AccessToken,
    ) -> Result<discussion_query::ResponseData, UniversalInboxError> {
        let request_body = DiscussionQuery::build_query(discussion_query::Variables {
            owner,
            repository,
            discussion_number,
        });

        let response = self
            .build_github_graphql_client(access_token)?
            .post(&self.github_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch discussion from Github graphql API")?
            .text()
            .await
            .context("Failed to fetch discussion response from Github graphql API")?;

        let discussion_response: graphql_client::Response<discussion_query::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&discussion_response, GITHUB_GRAPHQL_API_NAME)?;

        Ok(discussion_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Github graphql response"))?)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            raw_github_notification_id = raw_github_notification.id.to_string(),
            user.id = user_id.to_string()
        ),
        err
    )]
    pub async fn fetch_github_notification_item<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        raw_github_notification: &RawGithubNotification,
        user_id: UserId,
    ) -> Result<Option<GithubNotificationItem>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Github, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot fetch Github notification details without an access token")
            })?;

        let github_notification_item =
            if let Some(ref resource_url) = raw_github_notification.subject.url {
                match GithubUrl::try_from_api_url(resource_url) {
                    Ok(GithubUrl::PullRequest {
                        owner,
                        repository,
                        number,
                    }) => Some(GithubNotificationItem::GithubPullRequest(
                        self.query_pull_request(owner, repository, number, &access_token)
                            .await?
                            .try_into()?,
                    )),
                    Ok(GithubUrl::Discussion {
                        owner,
                        repository,
                        number,
                    }) => Some(GithubNotificationItem::GithubDiscussion(
                        self.query_discussion(owner, repository, number, &access_token)
                            .await?
                            .try_into()?,
                    )),
                    // Not yet implemented resource type
                    Err(_) => None,
                }
            } else {
                None
            };

        Ok(github_notification_item)
    }
}

#[async_trait]
impl ThirdPartyItemSourceService<GithubNotification> for GithubService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn fetch_items<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Github, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Github notifications without an access token"))?;

        let raw_github_notifications = stream::try_unfold(
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
        .try_collect::<Vec<Vec<RawGithubNotification>>>()
        .await?;

        let mut third_party_items = vec![];
        for raw_github_notification in raw_github_notifications.into_iter().flatten() {
            let github_notification_item = self
                .fetch_github_notification_item(executor, &raw_github_notification, user_id)
                .await?;
            let github_notification = GithubNotification {
                id: raw_github_notification.id.clone(),
                repository: raw_github_notification.repository.clone(),
                subject: raw_github_notification.subject.clone(),
                reason: raw_github_notification.reason.clone(),
                unread: raw_github_notification.unread,
                updated_at: raw_github_notification.updated_at,
                last_read_at: raw_github_notification.last_read_at,
                url: raw_github_notification.url.clone(),
                subscription_url: raw_github_notification.subscription_url.clone(),
                item: github_notification_item,
            };
            third_party_items
                .push(github_notification.into_third_party_item(user_id, integration_connection.id))
        }

        Ok(third_party_items)
    }

    fn is_sync_incremental(&self) -> bool {
        false
    }

    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::GithubNotification
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<GithubNotification> for GithubService {
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
        source: &GithubNotification,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.subject.title.clone(),
            status: if source.unread {
                NotificationStatus::Unread
            } else {
                NotificationStatus::Read
            },
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: source.last_read_at,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::Github,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = source_item.id.to_string(), user.id = user_id.to_string()),
        err
    )]
    async fn delete_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Github, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot delete Github notification without an access token"))?;

        self.mark_thread_as_read(&source_item.source_id, &access_token)
            .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = source_item.id.to_string(), user.id = user_id.to_string()),
        err
    )]
    async fn unsubscribe_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        source_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Github, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot unsubscribe from Github notifications without an access token")
            })?;

        self.mark_thread_as_read(&source_item.source_id, &access_token)
            .await?;
        self.unsubscribe_from_thread(&source_item.source_id, &access_token)
            .await
    }

    async fn snooze_notification_from_source<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Github notifications cannot be snoozed => no-op
        Ok(())
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
