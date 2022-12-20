use actix_http::uri::Authority;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use format_serde_error::SerdeError;
use futures::stream::{self, TryStreamExt};
use http::{HeaderMap, HeaderValue, Uri};

use universal_inbox::notification::integrations::github::GithubNotification;

use crate::{
    integrations::notification::{
        NotificationSource, NotificationSourceKind, NotificationSourceService,
    },
    universal_inbox::UniversalInboxError,
};

#[derive(Clone)]
pub struct GithubService {
    client: reqwest::Client,
    github_base_url: String,
    page_size: usize,
}

static GITHUB_BASE_URL: &str = "https://api.github.com";
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

impl GithubService {
    pub fn new(
        auth_token: &str,
        github_base_url: Option<String>,
        page_size: usize,
    ) -> Result<GithubService, UniversalInboxError> {
        Ok(GithubService {
            client: build_github_client(auth_token).context("Cannot build Github client")?,
            github_base_url: github_base_url.unwrap_or_else(|| GITHUB_BASE_URL.to_string()),
            page_size,
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_notifications(
        &self,
        page: u32,
        per_page: usize,
    ) -> Result<Vec<GithubNotification>, UniversalInboxError> {
        let url = format!(
            "{}/notifications?page={page}&per_page={per_page}",
            self.github_base_url
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Cannot fetch notifications from Github API")?
            .text()
            .await
            .context("Failed to fetch notifications response from Github API")?;

        let notifications: Vec<GithubNotification> = serde_json::from_str(&response)
            .map_err(|err| SerdeError::new(response, err))
            .context("Failed to parse response")?;

        Ok(notifications)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn mark_thread_as_read(&self, thread_id: &str) -> Result<(), UniversalInboxError> {
        let response = self
            .client
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

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn unsubscribe_from_thread(
        &self,
        thread_id: &str,
    ) -> Result<(), UniversalInboxError> {
        let response = self
            .client
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
}

fn build_github_client(auth_token: &str) -> Result<reqwest::Client, reqwest::Error> {
    let mut headers = HeaderMap::new();

    headers.insert(
        "Accept",
        HeaderValue::from_static("application/vnd.github.v3+json"),
    );
    let mut auth_header_value: HeaderValue = format!("token {}", auth_token).parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()
}

#[tracing::instrument(level = "debug")]
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
impl NotificationSourceService<GithubNotification> for GithubService {
    async fn fetch_all_notifications(
        &self,
    ) -> Result<Vec<GithubNotification>, UniversalInboxError> {
        Ok(stream::try_unfold((1, false), |(page, stop)| async move {
            if stop {
                return Ok(None);
            }

            let response = self.fetch_notifications(page, self.page_size).await;

            response.map(|github_notifs| {
                let notifs_count = github_notifs.len();
                let is_last_page = notifs_count < self.page_size;
                Some((github_notifs, (page + 1, is_last_page)))
            })
        })
        .try_collect::<Vec<Vec<GithubNotification>>>()
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<GithubNotification>>())
    }

    async fn delete_notification_from_source(
        &self,
        source_id: &str,
    ) -> Result<(), UniversalInboxError> {
        self.mark_thread_as_read(source_id).await
    }

    async fn unsubscribe_notification_from_source(
        &self,
        source_id: &str,
    ) -> Result<(), UniversalInboxError> {
        self.unsubscribe_from_thread(source_id).await
    }
}

impl NotificationSource for GithubService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Github
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
