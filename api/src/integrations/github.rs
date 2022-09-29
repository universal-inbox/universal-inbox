use anyhow::Context;
use format_serde_error::SerdeError;
use http::{HeaderMap, HeaderValue};

use crate::universal_inbox::UniversalInboxError;
use universal_inbox::integrations::github::GithubNotification;

pub struct GithubService {
    client: reqwest::Client,
    github_base_url: String,
}

static GITHUB_BASE_URL: &str = "https://api.github.com";
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

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

impl GithubService {
    pub fn new(
        auth_token: &str,
        github_base_url: Option<String>,
    ) -> Result<GithubService, UniversalInboxError> {
        Ok(GithubService {
            client: build_github_client(auth_token).context("Cannot build Github client")?,
            github_base_url: github_base_url.unwrap_or_else(|| GITHUB_BASE_URL.to_string()),
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
}
