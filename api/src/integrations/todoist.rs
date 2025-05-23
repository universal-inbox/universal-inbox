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
use chrono::{DateTime, Timelike, Utc};
use http::{HeaderMap, HeaderValue, StatusCode};
use regex::RegexBuilder;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Extension};
use reqwest_tracing::{
    DisableOtelPropagation, OtelPathNames, SpanBackendWithUrl, TracingMiddleware,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use url::Url;
use uuid::Uuid;
use wiremock::{
    matchers::{body_partial_json, body_string_contains, method, path},
    Mock, MockServer, ResponseTemplate,
};

use universal_inbox::{
    integration_connection::{
        integrations::todoist::{SyncToken, TodoistContext},
        provider::{
            IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind,
            IntegrationProviderSource,
        },
    },
    notification::{Notification, NotificationSource, NotificationSourceKind, NotificationStatus},
    task::{
        integrations::todoist::{TodoistProject, TODOIST_INBOX_PROJECT},
        service::TaskPatch,
        CreateOrUpdateTaskRequest, ProjectSummary, TaskCreation, TaskCreationConfig, TaskSource,
        TaskSourceKind, TaskStatus,
    },
    third_party::{
        integrations::todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
        item::{ThirdPartyItem, ThirdPartyItemFromSource, ThirdPartyItemSourceKind},
    },
    user::UserId,
    utils::default_value::DefaultValue,
};

use crate::{
    integrations::{
        notification::ThirdPartyNotificationSourceService,
        oauth2::AccessToken,
        task::{ThirdPartyTaskService, ThirdPartyTaskSourceService},
        third_party::ThirdPartyItemSourceService,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
};

#[derive(Clone)]
pub struct TodoistService {
    pub todoist_sync_base_url: String,
    pub todoist_sync_base_path: String,
    pub projects_cache_index: Arc<AtomicU64>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
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
    #[serde(rename = "item_uncomplete")]
    ItemUncomplete {
        uuid: Uuid,
        args: TodoistSyncCommandItemUncompleteArgs,
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
    pub project_id: Option<String>,
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

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncCommandItemUncompleteArgs {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
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
    pub sync_token: SyncToken,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct TodoistSyncStatusResponse {
    pub sync_status: HashMap<Uuid, TodoistCommandStatus>,
    pub full_sync: bool,
    pub temp_id_mapping: HashMap<String, String>,
    pub sync_token: SyncToken,
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
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    ) -> Result<TodoistService, UniversalInboxError> {
        let todoist_sync_base_url =
            todoist_sync_base_url.unwrap_or_else(|| TODOIST_SYNC_BASE_URL.to_string());
        let todoist_sync_base_path = Url::parse(&todoist_sync_base_url)
            .context("Cannot parse Todoist sync base URL")?
            .path()
            .to_string();

        Ok(TodoistService {
            todoist_sync_base_url,
            todoist_sync_base_path: if &todoist_sync_base_path == "/" {
                "".to_string()
            } else {
                todoist_sync_base_path
            },
            projects_cache_index: Arc::new(AtomicU64::new(0)),
            integration_connection_service,
        })
    }

    pub async fn mock_all(mock_server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/sync"))
            .and(body_partial_json(json!({ "resource_types": ["items"] })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(&TodoistSyncResponse {
                    items: Some(vec![]),
                    projects: None,
                    full_sync: false,
                    temp_id_mapping: HashMap::new(),
                    sync_token: SyncToken("abcd".to_string()),
                }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/sync"))
            .and(body_partial_json(json!({ "resource_types": ["projects"] })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(&TodoistSyncResponse {
                    items: None,
                    projects: Some(vec![]),
                    full_sync: false,
                    temp_id_mapping: HashMap::new(),
                    sync_token: SyncToken("abcd".to_string()),
                }),
            )
            .mount(mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/sync"))
            .and(body_string_contains(r#""commands""#))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(&TodoistSyncStatusResponse {
                        sync_status: HashMap::from([(
                            Uuid::new_v4(),
                            TodoistCommandStatus::Ok("ok".to_string()),
                        )]),
                        full_sync: false,
                        temp_id_mapping: HashMap::new(),
                        sync_token: SyncToken("abcd".to_string()),
                    }),
            )
            .mount(mock_server)
            .await;
    }

    fn build_todoist_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ClientWithMiddleware, UniversalInboxError> {
        let mut headers = HeaderMap::new();

        let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);

        let reqwest_client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(APP_USER_AGENT)
            .build()
            .context("Cannot build Todoist client")?;
        Ok(ClientBuilder::new(reqwest_client)
            .with_init(Extension(
                OtelPathNames::known_paths([
                    format!("{}/sync", self.todoist_sync_base_path),
                    format!("{}/items/get", self.todoist_sync_base_path),
                ])
                .context("Cannot build Otel path names")?,
            ))
            .with_init(Extension(DisableOtelPropagation))
            .with(TracingMiddleware::<SpanBackendWithUrl>::new())
            .build())
    }

    pub async fn sync_resources(
        &self,
        resource_name: &str,
        access_token: &AccessToken,
        sync_token: Option<SyncToken>,
    ) -> Result<TodoistSyncResponse, UniversalInboxError> {
        let response = self
            .build_todoist_client(access_token)?
            .post(format!("{}/sync", self.todoist_sync_base_url))
            .json(&json!({
                "sync_token": sync_token
                    .map(|sync_token| sync_token.0)
                    .unwrap_or_else(|| "*".to_string()),
                "resource_types": [resource_name]
            }))
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

        serde_json::from_str(&response)
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))
    }

    pub async fn sync_items(
        &self,
        access_token: &AccessToken,
        sync_token: Option<SyncToken>,
    ) -> Result<TodoistSyncResponse, UniversalInboxError> {
        self.sync_resources("items", access_token, sync_token).await
    }

    pub async fn get_item(
        &self,
        id: &str,
        access_token: &AccessToken,
    ) -> Result<Option<TodoistItem>, UniversalInboxError> {
        let response = self
            .build_todoist_client(access_token)?
            .post(format!("{}/items/get", self.todoist_sync_base_url))
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
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, body.clone()))?;

        Ok(Some(item_info.item))
    }

    pub async fn send_sync_commands(
        &self,
        commands: Vec<TodoistSyncCommand>,
        access_token: &AccessToken,
    ) -> Result<TodoistSyncStatusResponse, UniversalInboxError> {
        let body = json!({ "commands": commands });

        let response = self
            .build_todoist_client(access_token)?
            .post(format!("{}/sync", self.todoist_sync_base_url))
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
            .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        // It could be simpler as the first value is actually the `command_id` but httpmock
        // does not allow to use a request value into the mocked response
        let command_result = sync_response.sync_status.values().next();
        match command_result {
            Some(TodoistCommandStatus::Ok(_)) => Ok(sync_response),
            Some(TodoistCommandStatus::Error { error_code, error: _ }) => {
                if *error_code == 22 {
                    Err(UniversalInboxError::ItemNotFound(format!(
                        "Todoist item not found while sending commands {commands:?}: {command_result:?}"
                    )))
                } else {
                    Err(UniversalInboxError::Unexpected(anyhow!(
                        "Unexpected error while sending commands {commands:?}: {command_result:?}"
                    )))
                }
            }
            _ => Err(UniversalInboxError::Unexpected(anyhow!(
                "Failed to parse response from Todoist API while sending commands {commands:?}: {command_result:?}",
            ))),
        }
    }

    pub async fn fetch_all_projects(
        &self,
        user_id: UserId,
        access_token: &AccessToken,
        sync_token: Option<SyncToken>,
    ) -> Result<Vec<TodoistProject>, UniversalInboxError> {
        cached_fetch_all_projects(self, user_id, access_token, sync_token).await
    }

    #[allow(clippy::blocks_in_conditions)]
    async fn fetch_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_id: &str,
        user_id: UserId,
    ) -> Result<Option<TodoistItem>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch a Todoist task without an access token"))?;
        self.get_item(source_id, &access_token).await
    }

    pub async fn build_task_with_project_name(
        source: &TodoistItem,
        project_name: String,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Box<CreateOrUpdateTaskRequest> {
        Box::new(CreateOrUpdateTaskRequest {
            id: Uuid::new_v4().into(),
            title: source.content.clone(),
            body: source.description.clone(),
            status: if source.checked {
                TaskStatus::Done
            } else if source.is_deleted {
                TaskStatus::Deleted
            } else {
                TaskStatus::Active
            },
            completed_at: source.completed_at,
            priority: source.priority.into(),
            due_at: DefaultValue::new(None, Some(source.due.as_ref().map(|due| due.into()))),
            tags: source.labels.clone(),
            parent_id: None, // Unsupported for now
            project: DefaultValue::new(TODOIST_INBOX_PROJECT.to_string(), Some(project_name)),
            is_recurring: source
                .due
                .as_ref()
                .map(|due| due.is_recurring)
                .unwrap_or(false),
            created_at: source.added_at,
            updated_at: source_third_party_item.updated_at,
            kind: TaskSourceKind::Todoist,
            source_item: source_third_party_item.clone(),
            sink_item: Some(source_third_party_item.clone()),
            user_id,
        })
    }
}

#[cached(
    result = true,
    sync_writes = "by_key",
    size = 1,
    time = 600,
    key = "String",
    convert = r#"{ format!("{}{}{}", _user_id, service.projects_cache_index.load(Ordering::Relaxed), service.todoist_sync_base_url.clone()) }"#
)]
async fn cached_fetch_all_projects(
    service: &TodoistService,
    _user_id: UserId,
    access_token: &AccessToken,
    sync_token: Option<SyncToken>,
) -> Result<Vec<TodoistProject>, UniversalInboxError> {
    let sync_response: TodoistSyncResponse = service
        .sync_resources("projects", access_token, sync_token)
        .await?;

    sync_response.projects.ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow!("Todoist response should include `projects`"))
    })
}

#[async_trait]
impl ThirdPartyItemSourceService<TodoistItem> for TodoistService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn fetch_items(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Todoist task without an access token"))?;

        let items_sync_token = match integration_connection.provider {
            IntegrationProvider::Todoist {
                context: Some(TodoistContext { items_sync_token }),
                ..
            } => Some(items_sync_token),
            _ => None,
        };
        let sync_response = self.sync_items(&access_token, items_sync_token).await?;

        self.integration_connection_service
            .read()
            .await
            .update_integration_connection_context(
                executor,
                integration_connection.id,
                IntegrationConnectionContext::Todoist(TodoistContext {
                    items_sync_token: sync_response.sync_token,
                }),
            )
            .await
            .map_err(|_| {
                anyhow!(
                    "Failed to update Todoist integration connection {} context",
                    integration_connection.id
                )
            })?;

        sync_response
            .items
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!("Todoist response should include `items`"))
            })
            .map(|items| {
                items
                    .into_iter()
                    .map(|item| item.into_third_party_item(user_id, integration_connection.id))
                    .collect()
            })
    }

    fn is_sync_incremental(&self) -> bool {
        true
    }

    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::Todoist
    }
}

#[async_trait]
impl ThirdPartyTaskService<TodoistItem> for TodoistService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = source_third_party_item.id.to_string(),
            third_party_item_source_id = source_third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn third_party_item_into_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source: &TodoistItem,
        source_third_party_item: &ThirdPartyItem,
        _task_creation_config: Option<TaskCreationConfig>,
        user_id: UserId,
    ) -> Result<Box<CreateOrUpdateTaskRequest>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot build a Todoist task without an access token"))?;
        let projects = self
            .fetch_all_projects(user_id, &access_token, None)
            .await?;
        let project_name = projects
            .iter()
            .find(|project| project.id == source.project_id)
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "No project".to_string());

        Ok(TodoistService::build_task_with_project_name(
            source,
            project_name,
            source_third_party_item,
            user_id,
        )
        .await)
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn delete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot delete a Todoist task without an access token"))?;

        self.send_sync_commands(
            vec![TodoistSyncCommand::ItemDelete {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemDeleteArgs {
                    id: third_party_item.source_id.clone(),
                },
            }],
            &access_token,
        )
        .await?;

        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn complete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot complete a Todoist task without an access token"))?;

        self.send_sync_commands(
            vec![TodoistSyncCommand::ItemComplete {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemCompleteArgs {
                    id: third_party_item.source_id.clone(),
                },
            }],
            &access_token,
        )
        .await?;

        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn uncomplete_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot complete a Todoist task without an access token"))?;

        self.send_sync_commands(
            vec![TodoistSyncCommand::ItemUncomplete {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemUncompleteArgs {
                    id: third_party_item.source_id.clone(),
                },
            }],
            &access_token,
        )
        .await?;

        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            task_id = id,
            user.id = user_id.to_string()
        ),
        err
    )]
    async fn update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: &str,
        patch: &TaskPatch,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot update a Todoist task without an access token"))?;
        let mut commands: Vec<TodoistSyncCommand> = vec![];
        if let Some(ref project_name) = patch.project_name {
            let project = self
                .get_or_create_project(executor, project_name, user_id, Some(&access_token))
                .await?;
            commands.push(TodoistSyncCommand::ItemMove {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemMoveArgs {
                    id: id.to_string(),
                    project_id: project.source_id.to_string(),
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
            let content = patch.title.clone();

            commands.push(TodoistSyncCommand::ItemUpdate {
                uuid: Uuid::new_v4(),
                args: TodoistSyncCommandItemUpdateArgs {
                    id: id.to_string(),
                    due,
                    priority,
                    description,
                    content,
                },
            });
        }

        if !commands.is_empty() {
            self.send_sync_commands(commands, &access_token).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl ThirdPartyTaskSourceService<TodoistItem> for TodoistService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    async fn create_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task: &TaskCreation,
        user_id: UserId,
    ) -> Result<TodoistItem, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot create a Todoist task without an access token"))?;
        let project_id = if let Some(project_name) = &task.project_name {
            Some(
                self.get_or_create_project(executor, project_name, user_id, Some(&access_token))
                    .await?
                    .source_id
                    .to_string(),
            )
        } else {
            None
        };
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
                &access_token,
            )
            .await?;

        let task = self
            .fetch_task(
                executor,
                &sync_result
                    .temp_id_mapping
                    .values()
                    .next()
                    .context("Cannot find newly created task's ID".to_string())?
                    .to_string(),
                user_id,
            )
            .await?;

        task.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!("Failed to find newly created task on Todoist"))
        })
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(matches, user.id = user_id.to_string()),
        err
    )]
    async fn search_projects(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        matches: &str,
        user_id: UserId,
    ) -> Result<Vec<ProjectSummary>, UniversalInboxError> {
        let Some((access_token, _)) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
            .await?
        else {
            return Ok(vec![]);
        };

        let projects = self
            .fetch_all_projects(user_id, &access_token, None)
            .await?;
        let search_regex = RegexBuilder::new(matches)
            .case_insensitive(true)
            .size_limit(100_000)
            .build()
            .context(format!(
                "Failed to build regular expression from `{matches}`"
            ))?;

        Ok(projects
            .into_iter()
            .filter(|todoist_project| search_regex.is_match(&todoist_project.name))
            .map(|todoist_project| ProjectSummary {
                source_id: todoist_project.id.into(),
                name: todoist_project.name,
            })
            .collect())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(project_name, user.id = user_id.to_string()),
        err
    )]
    async fn get_or_create_project(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        project_name: &str,
        user_id: UserId,
        access_token: Option<&AccessToken>,
    ) -> Result<ProjectSummary, UniversalInboxError> {
        let access_token = match access_token {
            Some(access_token) => access_token.clone(),
            None => {
                self.integration_connection_service
                    .read()
                    .await
                    .find_access_token(executor, IntegrationProviderKind::Todoist, user_id)
                    .await?
                    .ok_or_else(|| {
                        anyhow!(
                            "Cannot create Todoist project {project_name} without an access token"
                        )
                    })?
                    .0
            }
        };

        let projects = self
            .fetch_all_projects(user_id, &access_token, None)
            .await?;
        if let Some(project) = projects
            .iter()
            .find(|project| project.name == *project_name)
        {
            return Ok(ProjectSummary {
                source_id: project.id.clone().into(),
                name: project.name.clone(),
            });
        }

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
                &access_token,
            )
            .await?;
        self.projects_cache_index.fetch_add(1, Ordering::Relaxed);

        let project_id = response
            .temp_id_mapping
            .values()
            .next()
            .context("Cannot find newly added project's ID".to_string())?
            .to_string();
        Ok(ProjectSummary {
            source_id: project_id.into(),
            name: project_name.to_string(),
        })
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<TodoistItem> for TodoistService {
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
        source: &TodoistItem,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.content.clone(),
            status: if source.checked || source.is_deleted {
                NotificationStatus::Deleted
            } else {
                NotificationStatus::Unread
            },
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::Todoist,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = _source_item.id.to_string(), user.id = _user_id.to_string()),,
        err
    )]
    async fn delete_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        unimplemented!("Todoist notifications cannot be deleted, only Todoist Task can");
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = _source_item.id.to_string(), user.id = _user_id.to_string()),
        err
    )]
    async fn unsubscribe_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        unimplemented!("Todoist notifications cannot be unsubscribed, only Todoist Task can");
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Todoist notifications cannot be snoozed => no-op
        Ok(())
    }
}

impl TaskSource for TodoistService {
    fn get_task_source_kind(&self) -> TaskSourceKind {
        TaskSourceKind::Todoist
    }
}

impl IntegrationProviderSource for TodoistService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Todoist
    }
}

impl NotificationSource for TodoistService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Todoist
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
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
            sync_token: SyncToken("abcd".to_string())
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
