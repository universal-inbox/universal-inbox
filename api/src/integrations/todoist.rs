use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use cached::proc_macro::cached;
use format_serde_error::SerdeError;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{IntegrationProvider, IntegrationProviderKind},
    notification::{NotificationSource, NotificationSourceKind},
    task::{
        integrations::todoist::{
            self, TodoistItem, TodoistItemDue, TodoistItemPriority, TodoistProject,
        },
        Task, TaskCreation, TaskMetadata, TaskPatch, TaskStatus,
    },
    user::UserId,
};

use crate::{
    integrations::{
        oauth2::AccessToken,
        task::{TaskSource, TaskSourceKind, TaskSourceService},
    },
    universal_inbox::UniversalInboxError,
};

#[derive(Clone, Debug)]
pub struct TodoistService {
    pub todoist_sync_base_url: String,
    pub projects_cache_index: Arc<AtomicU64>,
}

static TODOIST_SYNC_BASE_URL: &str = "https://api.todoist.com/sync/v9";
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "type")]
pub enum TodoistSyncCommand {
    #[serde(rename = "item_add")]
    ItemAdd {
        uuid: Uuid,
        temp_id: Uuid,
        args: TodoistSyncCommandItemAddArgs,
    },
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
pub struct TodoistSyncCommandItemAddArgs {
    pub content: String,
    pub description: Option<String>,
    pub project_id: String,
    pub due: Option<TodoistItemDue>,
    pub priority: TodoistItemPriority,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandItemDeleteArgs {
    pub id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandItemCompleteArgs {
    pub id: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct TodoistSyncCommandItemUpdateArgs {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<Option<TodoistItemDue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<TodoistItemPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistItemInfoResponse {
    pub item: TodoistItem,
}

impl TodoistService {
    pub fn new(
        todoist_sync_base_url: Option<String>,
    ) -> Result<TodoistService, UniversalInboxError> {
        Ok(TodoistService {
            todoist_sync_base_url: todoist_sync_base_url
                .unwrap_or_else(|| TODOIST_SYNC_BASE_URL.to_string()),
            projects_cache_index: Arc::new(AtomicU64::new(0)),
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn sync_resources(
        &self,
        resource_name: &str,
        access_token: &AccessToken,
    ) -> Result<TodoistSyncResponse, UniversalInboxError> {
        let response = build_todoist_client(access_token)
            .context("Cannot build Todoist client")?
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
    pub async fn sync_items(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TodoistItem>, UniversalInboxError> {
        let sync_response = self.sync_resources("items", access_token).await?;

        sync_response.items.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Todoist response should include `items`"))
        })
    }

    #[tracing::instrument(level = "debug", skip(self), ret)]
    pub async fn get_item(
        &self,
        id: &str,
        access_token: &AccessToken,
    ) -> Result<Option<TodoistItem>, UniversalInboxError> {
        let response = build_todoist_client(access_token)
            .context("Cannot build Todoist client")?
            .post(&format!("{}/items/get", self.todoist_sync_base_url))
            .form(&[("item_id", id), ("all_data", "false")])
            .send()
            .await
            .context(format!("Cannot get item {id} from Todoist API"))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let body = response
            .error_for_status()
            .context(format!("Cannot get item {id} from Todoist API"))?
            .text()
            .await
            .context(format!(
                "Failed to fetch item {id} response from Todoist API"
            ))?;

        let item_info: TodoistItemInfoResponse = serde_json::from_str(&body)
            .map_err(|err| SerdeError::new(body.clone(), err))
            .context(format!("Failed to parse response from Todoist: {body}"))?;

        Ok(Some(item_info.item))
    }

    #[tracing::instrument(level = "debug", skip(self), ret)]
    pub async fn send_sync_commands(
        &self,
        commands: Vec<TodoistSyncCommand>,
        access_token: &AccessToken,
    ) -> Result<TodoistSyncStatusResponse, UniversalInboxError> {
        let body = json!({ "commands": commands });

        let response = build_todoist_client(access_token)
            .context("Cannot build Todoist client")?
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

    #[tracing::instrument(level = "debug", skip(self), ret)]
    pub async fn fetch_all_projects(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TodoistProject>, UniversalInboxError> {
        cached_fetch_all_projects(self, access_token).await
    }

    async fn get_or_create_project_id(
        &self,
        project_name: &str,
        access_token: &AccessToken,
    ) -> Result<String, UniversalInboxError> {
        let projects = self.fetch_all_projects(access_token).await?;
        if let Some(id) = projects
            .iter()
            .find(|project| project.name == *project_name)
            .map(|project| project.id.clone())
        {
            Ok(id)
        } else {
            let command_id = Uuid::new_v4();
            let response = self
                .send_sync_commands(
                    vec![TodoistSyncCommand::ProjectAdd {
                        uuid: command_id,
                        temp_id: Uuid::new_v4(),
                        args: TodoistSyncCommandProjectAddArgs {
                            name: project_name.to_string(),
                        },
                    }],
                    access_token,
                )
                .await?;
            self.projects_cache_index.fetch_add(1, Ordering::Relaxed);

            Ok(response
                .temp_id_mapping
                .values()
                .next()
                .context("Cannot find newly added project's ID".to_string())?
                .to_string())
        }
    }

    pub async fn build_task_with_project_name(
        source: &TodoistItem,
        project_name: String,
        user_id: UserId,
    ) -> Box<Task> {
        Box::new(Task {
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
            user_id,
        })
    }
}

#[cached(
    result = true,
    sync_writes = true,
    size = 1,
    time = 600,
    key = "String",
    convert = r#"{ format!("{}{}", service.projects_cache_index.load(Ordering::Relaxed), service.todoist_sync_base_url.clone()) }"#
)]
async fn cached_fetch_all_projects(
    service: &TodoistService,
    access_token: &AccessToken,
) -> Result<Vec<TodoistProject>, UniversalInboxError> {
    let sync_response: TodoistSyncResponse =
        service.sync_resources("projects", access_token).await?;

    sync_response.projects.ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow!("Todoist response should include `projects`"))
    })
}

fn build_todoist_client(access_token: &AccessToken) -> Result<reqwest::Client, reqwest::Error> {
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
impl TaskSourceService<TodoistItem> for TodoistService {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_all_tasks(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TodoistItem>, UniversalInboxError> {
        self.sync_items(access_token).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_task(
        &self,
        source_id: &str,
        access_token: &AccessToken,
    ) -> Result<Option<TodoistItem>, UniversalInboxError> {
        self.get_item(source_id, access_token).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn build_task(
        &self,
        source: &TodoistItem,
        user_id: UserId,
        access_token: &AccessToken,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let projects = self.fetch_all_projects(access_token).await?;
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

        Ok(TodoistService::build_task_with_project_name(source, project_name, user_id).await)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_task(
        &self,
        task: &TaskCreation,
        access_token: &AccessToken,
    ) -> Result<TodoistItem, UniversalInboxError> {
        let project_id = self
            .get_or_create_project_id(&task.project.to_string(), access_token)
            .await?;
        let sync_result = self
            .send_sync_commands(
                vec![TodoistSyncCommand::ItemAdd {
                    uuid: Uuid::new_v4(),
                    temp_id: Uuid::new_v4(),
                    args: TodoistSyncCommandItemAddArgs {
                        content: task.title.clone(),
                        description: task.body.clone(),
                        project_id,
                        due: task.due_at.as_ref().map(|due| due.into()),
                        priority: task.priority.into(),
                    },
                }],
                access_token,
            )
            .await?;

        let task = self
            .fetch_task(
                &sync_result
                    .temp_id_mapping
                    .values()
                    .next()
                    .context("Cannot find newly created task's ID".to_string())?
                    .to_string(),
                access_token,
            )
            .await?;

        task.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Failed to find newly created task on Todoist"))
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn delete_task(
        &self,
        id: &str,
        access_token: &AccessToken,
    ) -> Result<TodoistSyncStatusResponse, UniversalInboxError> {
        self.send_sync_commands(
            vec![TodoistSyncCommand::ItemDelete {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemDeleteArgs { id: id.to_string() },
            }],
            access_token,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn complete_task(
        &self,
        id: &str,
        access_token: &AccessToken,
    ) -> Result<TodoistSyncStatusResponse, UniversalInboxError> {
        self.send_sync_commands(
            vec![TodoistSyncCommand::ItemComplete {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemCompleteArgs { id: id.to_string() },
            }],
            access_token,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_task(
        &self,
        id: &str,
        patch: &TaskPatch,
        access_token: &AccessToken,
    ) -> Result<Option<TodoistSyncStatusResponse>, UniversalInboxError> {
        let mut commands: Vec<TodoistSyncCommand> = vec![];
        if let Some(ref project_name) = patch.project {
            let project_id = self
                .get_or_create_project_id(project_name, access_token)
                .await?;
            commands.push(TodoistSyncCommand::ItemMove {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemMoveArgs {
                    id: id.to_string(),
                    project_id,
                },
            });
        }

        if patch.priority.is_some() || patch.due_at.is_some() || patch.body.is_some() {
            let priority = patch.priority.map(|priority| priority.into());
            let due = patch
                .due_at
                .as_ref()
                .map(|due| due.as_ref().map(|d| d.into()));
            let description = patch.body.clone();

            commands.push(TodoistSyncCommand::ItemUpdate {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemUpdateArgs {
                    id: id.to_string(),
                    due,
                    priority,
                    description,
                },
            });
        }

        if commands.is_empty() {
            Ok(None)
        } else {
            Ok(Some(self.send_sync_commands(commands, access_token).await?))
        }
    }
}

impl TaskSource for TodoistService {
    fn get_task_source_kind(&self) -> TaskSourceKind {
        TaskSourceKind::Todoist
    }
}

impl IntegrationProvider for TodoistService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Todoist
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
