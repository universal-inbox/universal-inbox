use anyhow::{anyhow, Context};
use async_trait::async_trait;
use format_serde_error::SerdeError;
use http::{HeaderMap, HeaderValue};
use reqwest::Url;
use uuid::Uuid;

use crate::universal_inbox::{notification::source::NotificationSourceKind, UniversalInboxError};
use universal_inbox::notification::{
    integrations::todoist::TodoistTask, Notification, NotificationMetadata, NotificationStatus,
};

use super::notification::{NotificationSourceService, SourceNotification};

#[derive(Clone)]
pub struct TodoistService {
    client: reqwest::Client,
    todoist_base_url: String,
}

static TODOIST_BASE_URL: &str = "https://api.todoist.com/rest/v2";
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

impl TodoistService {
    pub fn new(
        auth_token: &str,
        todoist_base_url: Option<String>,
    ) -> Result<TodoistService, UniversalInboxError> {
        Ok(TodoistService {
            client: build_todoist_client(auth_token).context("Cannot build Todoist client")?,
            todoist_base_url: todoist_base_url.unwrap_or_else(|| TODOIST_BASE_URL.to_string()),
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_tasks<'a>(
        &self,
        filter: Option<&'a str>,
    ) -> Result<Vec<TodoistTask>, UniversalInboxError> {
        let url = Url::parse_with_params(
            &format!("{}/tasks", self.todoist_base_url),
            filter.map(|f| ("filter", f)).into_iter(),
        )
        .context("Failed to build Todoist URL")?
        .to_string();
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Cannot fetch tasks from Todoist API")?
            .text()
            .await
            .context("Failed to fetch tasks response from Todoist API")?;

        let tasks: Vec<TodoistTask> = serde_json::from_str(&response)
            .map_err(|err| SerdeError::new(response, err))
            .context("Failed to parse response from Todoist")?;

        Ok(tasks)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn delete_task(&self, task_id: &str) -> Result<(), UniversalInboxError> {
        let response = self
            .client
            .delete(&format!("{}/tasks/{task_id}", self.todoist_base_url))
            .send()
            .await
            .with_context(|| format!("Failed to delete Todoist task `{task_id}`"))?;

        match response.error_for_status() {
            Ok(_) => Ok(()),
            Err(err) if err.status() == Some(reqwest::StatusCode::NOT_FOUND) => Ok(()),
            Err(error) => {
                tracing::error!(
                    "An error occurred when trying to delete Todoist task `{task_id}`: {}",
                    error
                );
                Err(UniversalInboxError::Unexpected(anyhow!(
                    "Failed to delete Todoist task `{task_id}`"
                )))
            }
        }
    }
}

fn build_todoist_client(auth_token: &str) -> Result<reqwest::Client, reqwest::Error> {
    let mut headers = HeaderMap::new();

    let mut auth_header_value: HeaderValue = format!("Bearer {auth_token}").parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()
}

#[async_trait]
impl NotificationSourceService<TodoistTask> for TodoistService {
    async fn fetch_all_notifications(&self) -> Result<Vec<TodoistTask>, UniversalInboxError> {
        self.fetch_tasks(Some("#Inbox")).await
    }

    fn build_notification(&self, source: &TodoistTask) -> Box<Notification> {
        Box::new(Notification {
            id: Uuid::new_v4(),
            title: source.content.clone(),
            source_id: source.id.clone(),
            source_html_url: Some(source.url.clone()),
            status: NotificationStatus::Unread,
            metadata: NotificationMetadata::Todoist(source.clone()),
            updated_at: source.created_at,
            last_read_at: None,
            snoozed_until: None,
        })
    }

    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Todoist
    }

    async fn delete_notification_from_source(
        &self,
        source_id: &str,
    ) -> Result<(), UniversalInboxError> {
        self.delete_task(source_id).await
    }

    async fn unsubscribe_notification_from_source(
        &self,
        source_id: &str,
    ) -> Result<(), UniversalInboxError> {
        Err(UniversalInboxError::UnsupportedAction(format!(
            "Cannot unsubscribe from Todoist task `{source_id}`"
        )))
    }
}

impl SourceNotification for TodoistTask {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}
