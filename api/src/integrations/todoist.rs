use std::collections::HashMap;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cached::proc_macro::cached;
use chrono::{TimeZone, Utc};
use format_serde_error::SerdeError;
use http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use universal_inbox::task::{
    integrations::todoist::{self, TodoistItem, TodoistProject},
    DueDate, Task, TaskMetadata, TaskStatus,
};

use crate::{
    integrations::{
        notification::{NotificationSource, NotificationSourceKind},
        task::{TaskSource, TaskSourceKind, TaskSourceService},
    },
    universal_inbox::UniversalInboxError,
};

#[derive(Clone)]
pub struct TodoistService {
    client: reqwest::Client,
    todoist_sync_base_url: String,
}

static TODOIST_SYNC_BASE_URL: &str = "https://api.todoist.com/sync/v9";
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncResponse {
    pub items: Option<Vec<TodoistItem>>,
    pub projects: Option<Vec<TodoistProject>>,
    pub full_sync: bool,
    pub temp_id_mapping: HashMap<String, String>,
    pub sync_token: String,
}

impl TodoistService {
    pub fn new(
        auth_token: &str,
        todoist_sync_base_url: Option<String>,
    ) -> Result<TodoistService, UniversalInboxError> {
        Ok(TodoistService {
            client: build_todoist_client(auth_token).context("Cannot build Todoist client")?,
            todoist_sync_base_url: todoist_sync_base_url
                .unwrap_or_else(|| TODOIST_SYNC_BASE_URL.to_string()),
        })
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

        let sync_response: TodoistSyncResponse = serde_json::from_str(&response)
            .map_err(|err| SerdeError::new(response, err))
            .context("Failed to parse response from Todoist")?;

        sync_response.items.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Todoist response should include `items`"))
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn delete_item(&self, id: &str) -> Result<(), UniversalInboxError> {
        let command_uuid = Uuid::new_v4().to_string();
        let json_response: serde_json::value::Value = self
            .client
            .post(&format!("{}/sync", self.todoist_sync_base_url))
            .json(&json!([
                {
                    "type": "item_delete",
                    "uuid": command_uuid,
                    "args": { "id": id }
                }
            ]))
            .send()
            .await
            .with_context(|| format!("Cannot delete item `{id}` from Todoist API"))?
            .json()
            .await
            .with_context(|| {
                format!("Failed to fetch response from Todoist API while deleting item `{id}`")
            })?;

        let sync_status = json_response["sync_status"].as_object().with_context(|| {
            format!("Failed to parse response from Todoist API while deleting item `{id}`")
        })?;
        // It could be simpler as the first value is actually the `command_id` but httpmock
        // does not allow to use a request value into the mocked response
        let command_result = sync_status.values().next();
        match command_result {
            Some(serde_json::Value::String(s)) if s == "ok" => Ok(()),
            Some(serde_json::Value::Object(error_obj)) if error_obj.contains_key("error") => {
                Err(UniversalInboxError::Unexpected(anyhow!(
                    "Unexpected error while deleting item `{id}`: {:?}",
                    error_obj.get("error").unwrap().as_str()
                )))
            }
            _ => Err(UniversalInboxError::Unexpected(anyhow!(
                "Failed to parse response from Todoist API while deleting item `{id}`: {:?}",
                command_result
            ))),
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_all_projects(&self) -> Result<Vec<TodoistProject>, UniversalInboxError> {
        cached_fetch_all_projects(&self.client, &self.todoist_sync_base_url).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
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
    let sync_response: TodoistSyncResponse = client
        .post(&format!("{}/sync", todoist_sync_base_url))
        .json(&json!({ "sync_token": "*", "resource_types": ["projects"] }))
        .send()
        .await
        .context("Cannot sync projects from Todoist API")?
        .json()
        .await
        .context("Failed to fetch and parse projects response from Todoist API")?;

    sync_response.projects.ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow!("Todoist response should include `projects`"))
    })
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
            parent_id: None, // Unsupported for now
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
}

impl TaskSource for TodoistService {
    fn get_task_source_kind(&self) -> TaskSourceKind {
        TaskSourceKind::Todoist
    }
}

impl NotificationSource for TodoistService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Todoist
    }
}
