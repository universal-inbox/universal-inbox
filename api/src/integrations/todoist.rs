use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cached::proc_macro::cached;
use chrono::{TimeZone, Utc};
use format_serde_error::SerdeError;
use http::{HeaderMap, HeaderValue};
use reqwest::Url;
use serde_json::json;
use universal_inbox::task::{
    integrations::todoist::{self, TodoistProject},
    DueDate,
};
use uuid::Uuid;

use universal_inbox::{
    notification::{
        integrations::todoist::TodoistTask, Notification, NotificationMetadata, NotificationStatus,
    },
    task::{integrations::todoist::TodoistItem, Task, TaskMetadata, TaskStatus},
};

use crate::universal_inbox::{
    notification::source::NotificationSourceKind, task::source::TaskSourceKind, UniversalInboxError,
};

use super::{
    notification::{NotificationSourceService, SourceNotification},
    task::{SourceTask, TaskSourceService},
};

#[derive(Clone)]
pub struct TodoistService {
    client: reqwest::Client,
    todoist_base_url: String,
    todoist_sync_base_url: String,
}

static TODOIST_BASE_URL: &str = "https://api.todoist.com/rest/v2";
static TODOIST_SYNC_BASE_URL: &str = "https://api.todoist.com/sync/v9";
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

impl TodoistService {
    pub fn new(
        auth_token: &str,
        todoist_base_url: Option<String>,
        todoist_sync_base_url: Option<String>,
    ) -> Result<TodoistService, UniversalInboxError> {
        Ok(TodoistService {
            client: build_todoist_client(auth_token).context("Cannot build Todoist client")?,
            todoist_base_url: todoist_base_url.unwrap_or_else(|| TODOIST_BASE_URL.to_string()),
            todoist_sync_base_url: todoist_sync_base_url
                .unwrap_or_else(|| TODOIST_SYNC_BASE_URL.to_string()),
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

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_items(&self) -> Result<Vec<TodoistItem>, UniversalInboxError> {
        let response = self
            .client
            .post(&format!("{}/sync", self.todoist_sync_base_url))
            .json(&json!({ "sync_token": "*", "resource_types": ["items"] }))
            .send()
            .await
            .context("Cannot sync items from Todoist API")?
            .text()
            .await
            .context("Failed to fetch items response from Todoist API")?;

        let tasks: Vec<TodoistItem> = serde_json::from_str(&response)
            .map_err(|err| SerdeError::new(response, err))
            .context("Failed to parse response from Todoist")?;

        Ok(tasks)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_all_projects<'a>(&self) -> Result<Vec<TodoistProject>, UniversalInboxError> {
        cached_fetch_all_projects(&self.client, &self.todoist_sync_base_url).await
    }

    pub async fn get_project_name(&self, project_id: &str) -> Result<String, UniversalInboxError> {
        let projects = self.fetch_all_projects().await?;
        projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.name.clone())
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Failed to find Todoist project with ID {project_id}"
                ))
            })
    }
}

#[cached(
    result = true,
    sync_writes = true,
    size = 1,
    time = 600,
    key = "String",
    convert = r#"{ "default".to_string() }"#
)]
async fn cached_fetch_all_projects(
    client: &reqwest::Client,
    todoist_sync_base_url: &str,
) -> Result<Vec<TodoistProject>, UniversalInboxError> {
    Ok(client
        .post(&format!("{}/sync", todoist_sync_base_url))
        .json(&json!({ "sync_token": "*", "resource_types": ["projects"] }))
        .send()
        .await
        .context("Cannot sync projects from Todoist API")?
        .json()
        .await
        .context("Failed to fetch and parse projects response from Todoist API")?)
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

#[async_trait]
impl TaskSourceService<TodoistItem> for TodoistService {
    async fn fetch_all_tasks(&self) -> Result<Vec<TodoistItem>, UniversalInboxError> {
        self.sync_items().await
    }

    async fn build_task(&self, source: &TodoistItem) -> Result<Box<Task>, UniversalInboxError> {
        let project_name = self.get_project_name(&source.project_id).await?;

        Ok(Box::new(Task {
            id: Uuid::new_v4(),
            source_id: source.id.clone(),
            title: source.content.clone(),
            body: source.description.clone(),
            status: if source.checked {
                TaskStatus::Done
            } else {
                TaskStatus::Active
            },
            completed_at: source.completed_at,
            priority: source.priority.into(),
            due_at: Some(DueDate::DateTimeWithTz(
                Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).latest().unwrap(),
            )),
            source_html_url: todoist::get_task_html_url(&source.id),
            tags: source.labels.clone(),
            parent_id: None, // TODO
            project: project_name,
            is_recurring: source
                .due
                .as_ref()
                .map(|due| due.is_recurring)
                .unwrap_or(false),
            created_at: source.added_at,
            metadata: TaskMetadata::Todoist(source.clone()),
        }))
    }

    fn get_task_source_kind(&self) -> TaskSourceKind {
        TaskSourceKind::Todoist
    }
}

impl SourceTask for TodoistItem {
    fn get_id(&self) -> String {
        self.id.clone()
    }
}
