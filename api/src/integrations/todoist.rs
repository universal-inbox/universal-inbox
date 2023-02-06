use std::collections::HashMap;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cached::proc_macro::cached;
use format_serde_error::SerdeError;
use http::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use universal_inbox::task::{
    integrations::todoist::{
        self, TodoistItem, TodoistItemDue, TodoistItemPriority, TodoistProject,
    },
    Task, TaskMetadata, TaskPatch, TaskStatus,
};

use crate::{
    integrations::{
        notification::{NotificationSource, NotificationSourceKind},
        task::{TaskSource, TaskSourceKind, TaskSourceService},
    },
    universal_inbox::UniversalInboxError,
};

#[derive(Clone, Debug)]
pub struct TodoistService {
    client: reqwest::Client,
    pub todoist_sync_base_url: String,
}

static TODOIST_SYNC_BASE_URL: &str = "https://api.todoist.com/sync/v9";
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "type")]
pub enum TodoistSyncCommand {
    #[serde(rename = "item_delete")]
    ItemDelete {
        uuid: Uuid,
        args: TodoistSyncCommandItemDeleteArgs,
    },
    #[serde(rename = "item_complete")]
    ItemComplete {
        uuid: Uuid,
        args: TodoistSyncCommandItemCompleteArgs,
    },
    #[serde(rename = "item_update")]
    ItemUpdate {
        uuid: Uuid,
        args: TodoistSyncCommandItemUpdateArgs,
    },
    #[serde(rename = "item_move")]
    ItemMove {
        uuid: Uuid,
        args: TodoistSyncCommandItemMoveArgs,
    },
    #[serde(rename = "project_add")]
    ProjectAdd {
        uuid: Uuid,
        temp_id: Uuid,
        args: TodoistSyncCommandProjectAddArgs,
    },
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandItemDeleteArgs {
    pub id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandItemCompleteArgs {
    pub id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandItemUpdateArgs {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<Option<TodoistItemDue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<TodoistItemPriority>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandItemMoveArgs {
    pub id: String,
    pub project_id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandProjectAddArgs {
    pub name: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncResponse {
    pub items: Option<Vec<TodoistItem>>,
    pub projects: Option<Vec<TodoistProject>>,
    pub full_sync: bool,
    pub temp_id_mapping: HashMap<String, String>,
    pub sync_token: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncStatusResponse {
    pub sync_status: HashMap<Uuid, TodoistCommandStatus>,
    pub full_sync: bool,
    pub temp_id_mapping: HashMap<String, String>,
    pub sync_token: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(untagged)]
pub enum TodoistCommandStatus {
    Ok(String),
    Error { error_code: i32, error: String },
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

    pub async fn sync_resources(
        &self,
        resource_name: &str,
    ) -> Result<TodoistSyncResponse, UniversalInboxError> {
        let response = self
            .client
            .post(&format!("{}/sync", self.todoist_sync_base_url))
            .json(&json!({ "sync_token": "*", "resource_types": [resource_name] }))
            .send()
            .await
            .context(format!("Cannot sync {resource_name} from Todoist API"))?
            .error_for_status()
            .context(format!("Cannot sync {resource_name} from Todoist API"))?
            .text()
            .await
            .context(format!(
                "Failed to fetch {resource_name} response from Todoist API"
            ))?;

        Ok(serde_json::from_str(&response)
            .map_err(|err| SerdeError::new(response, err))
            .context("Failed to parse response from Todoist")?)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_items(&self) -> Result<Vec<TodoistItem>, UniversalInboxError> {
        let sync_response = self.sync_resources("items").await?;

        sync_response.items.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Todoist response should include `items`"))
        })
    }

    #[tracing::instrument(level = "debug", skip(self), ret)]
    pub async fn send_sync_commands(
        &self,
        commands: Vec<TodoistSyncCommand>,
    ) -> Result<TodoistSyncStatusResponse, UniversalInboxError> {
        let body = json!({ "commands": commands });

        let response = self
            .client
            .post(&format!("{}/sync", self.todoist_sync_base_url))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Cannot send commands {commands:?} to the Todoist API"))?
            .text()
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch response from Todoist API while sending commands {commands:?}"
                )
            })?;

        let sync_response: TodoistSyncStatusResponse = serde_json::from_str(&response)
            .map_err(|err| SerdeError::new(response, err))
            .context("Failed to parse response from Todoist")?;

        // It could be simpler as the first value is actually the `command_id` but httpmock
        // does not allow to use a request value into the mocked response
        let command_result = sync_response.sync_status.values().next();
        match command_result {
            Some(TodoistCommandStatus::Ok(_)) => Ok(sync_response),
            Some(TodoistCommandStatus::Error { error_code: _, error }) => {
                Err(UniversalInboxError::Unexpected(anyhow!(
                    "Unexpected error while sending commands {commands:?}: {:?}",
                    error
                )))
            }
            _ => Err(UniversalInboxError::Unexpected(anyhow!(
                "Failed to parse response from Todoist API while sending commands {commands:?}: {:?}",
                command_result
            ))),
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_all_projects(&self) -> Result<Vec<TodoistProject>, UniversalInboxError> {
        cached_fetch_all_projects(self).await
    }
}

#[cached(
    result = true,
    sync_writes = true,
    size = 1,
    time = 600,
    key = "String",
    convert = r#"{ service.todoist_sync_base_url.clone() }"#
)]
async fn cached_fetch_all_projects(
    service: &TodoistService,
) -> Result<Vec<TodoistProject>, UniversalInboxError> {
    let sync_response: TodoistSyncResponse = service.sync_resources("projects").await?;

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
        let projects = self.fetch_all_projects().await?;
        let project_name = projects
            .iter()
            .find(|project| project.id == source.project_id)
            .map(|project| project.name.clone())
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Failed to find Todoist project with ID {}",
                    source.project_id
                ))
            })?;

        Ok(Box::new(Task {
            id: Uuid::new_v4().into(),
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
            due_at: source.due.as_ref().map(|due| due.into()),
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

    async fn delete_task_from_source(
        &self,
        id: &str,
    ) -> Result<TodoistSyncStatusResponse, UniversalInboxError> {
        self.send_sync_commands(vec![TodoistSyncCommand::ItemDelete {
            uuid: Uuid::new_v4(),
            args: TodoistSyncCommandItemDeleteArgs { id: id.to_string() },
        }])
        .await
    }

    async fn complete_task_from_source(
        &self,
        id: &str,
    ) -> Result<TodoistSyncStatusResponse, UniversalInboxError> {
        self.send_sync_commands(vec![TodoistSyncCommand::ItemComplete {
            uuid: Uuid::new_v4(),
            args: TodoistSyncCommandItemCompleteArgs { id: id.to_string() },
        }])
        .await
    }

    async fn update_task(
        &self,
        id: &str,
        patch: &TaskPatch,
    ) -> Result<Option<TodoistSyncStatusResponse>, UniversalInboxError> {
        let mut commands: Vec<TodoistSyncCommand> = vec![];
        if let Some(ref project_name) = patch.project {
            let projects = self.fetch_all_projects().await?;
            let project_id = if let Some(id) = projects
                .iter()
                .find(|project| project.name == *project_name)
                .map(|project| project.id.clone())
            {
                id
            } else {
                let command_id = Uuid::new_v4();
                let response = self
                    .send_sync_commands(vec![TodoistSyncCommand::ProjectAdd {
                        uuid: command_id,
                        temp_id: Uuid::new_v4(),
                        args: TodoistSyncCommandProjectAddArgs {
                            name: project_name.to_string(),
                        },
                    }])
                    .await?;
                response
                    .temp_id_mapping
                    .values()
                    .next()
                    .context("Cannot find newly added project's ID".to_string())?
                    .to_string()
            };
            commands.push(TodoistSyncCommand::ItemMove {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemMoveArgs {
                    id: id.to_string(),
                    project_id,
                },
            });
        }

        if patch.priority.is_some() || patch.due_at.is_some() {
            let priority = patch.priority.map(|priority| priority.into());
            let due = patch
                .due_at
                .as_ref()
                .map(|due| due.as_ref().map(|d| d.into()));

            commands.push(TodoistSyncCommand::ItemUpdate {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemUpdateArgs {
                    id: id.to_string(),
                    due,
                    priority,
                },
            });
        }

        if commands.is_empty() {
            Ok(None)
        } else {
            Ok(Some(self.send_sync_commands(commands).await?))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use pretty_assertions::assert_eq;
    use rstest::*;

    use universal_inbox::task::DueDate;
    use uuid::uuid;

    #[rstest]
    fn test_parse_todoist_sync_status_response() {
        assert_eq!(serde_json::from_str::<TodoistSyncStatusResponse>(
            r#"
            {
                "sync_status": {
                    "f8539c77-7fd7-4846-afad-3b201f0be8a4": "ok",
                    "f8539c77-7fd7-4846-afad-3b201f0be8a5": { "error_code": 42, "error": "Something went wrong" }
                },
                "temp_id_mapping": {},
                "full_sync": false,
                "sync_token": "abcd"
            }
            "#
        ).unwrap(), TodoistSyncStatusResponse {
            sync_status: HashMap::from([
                (uuid!("f8539c77-7fd7-4846-afad-3b201f0be8a4"), TodoistCommandStatus::Ok("ok".to_string())),
                (uuid!("f8539c77-7fd7-4846-afad-3b201f0be8a5"), TodoistCommandStatus::Error {
                    error_code: 42,
                    error: "Something went wrong".to_string(),
                }),
            ]),
            temp_id_mapping: HashMap::new(),
            full_sync: false,
            sync_token: "abcd".to_string(),
        });
    }

    #[rstest]
    fn test_todoist_sync_command_item_update_args_serialization_no_values() {
        assert_eq!(
            serde_json::to_string(&TodoistSyncCommandItemUpdateArgs {
                id: "123".to_string(),
                due: None,
                priority: None
            })
            .unwrap(),
            json!({ "id": "123" }).to_string()
        );
    }

    #[rstest]
    fn test_todoist_sync_command_item_update_args_serialization_reset_due() {
        assert_eq!(
            serde_json::to_string(&TodoistSyncCommandItemUpdateArgs {
                id: "123".to_string(),
                due: Some(None),
                priority: None
            })
            .unwrap(),
            json!({ "id": "123", "due": null }).to_string()
        );
    }

    #[rstest]
    fn test_todoist_sync_command_item_update_args_serialization_with_values() {
        assert_eq!(
            serde_json::to_string(&TodoistSyncCommandItemUpdateArgs {
                id: "123".to_string(),
                due: Some(Some(TodoistItemDue {
                    string: "every day".to_string(),
                    date: DueDate::Date(NaiveDate::from_ymd_opt(2022, 1, 2).unwrap()),
                    is_recurring: false,
                    timezone: None,
                    lang: "en".to_string()
                })),
                priority: None
            })
            .unwrap(),
            json!({
                "id": "123",
                "due": {
                    "string": "every day",
                    "date": "2022-01-02",
                    "is_recurring": false,
                    "timezone": null,
                    "lang": "en"
                }
            })
            .to_string()
        );
    }
}
