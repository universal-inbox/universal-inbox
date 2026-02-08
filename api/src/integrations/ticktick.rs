use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use cached::proc_macro::cached;
use chrono::{DateTime, Timelike, Utc};
use http::{HeaderMap, HeaderValue};
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use url::Url;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox::{
    integration_connection::{
        integrations::ticktick::TickTickContext,
        provider::{
            IntegrationConnectionContext, IntegrationProviderKind, IntegrationProviderSource,
        },
    },
    notification::{Notification, NotificationSource, NotificationSourceKind, NotificationStatus},
    task::{
        CreateOrUpdateTaskRequest, ProjectSummary, TaskCreation, TaskCreationConfig, TaskSource,
        TaskSourceKind, TaskStatus,
        integrations::ticktick::{TICKTICK_INBOX_PROJECT, TickTickProject},
        service::TaskPatch,
    },
    third_party::{
        integrations::ticktick::{TickTickItem, TickTickItemPriority, TickTickTaskStatus},
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
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
    },
    utils::api::{ApiClient, ApiClientError},
};

#[derive(Clone)]
pub struct TickTickService {
    pub ticktick_base_url: String,
    pub ticktick_base_path: String,
    pub projects_cache_index: Arc<AtomicU64>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    pub max_retry_duration: Duration,
}

static TICKTICK_BASE_URL: &str = "https://api.ticktick.com/open/v1";

/// TickTick API response for creating a task
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickCreateTaskResponse {
    pub id: String,
    pub project_id: String,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub all_day: Option<bool>,
    #[serde(default)]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub time_zone: Option<String>,
    pub priority: TickTickItemPriority,
    pub status: TickTickTaskStatus,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// Request body for creating a task via TickTick API
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickCreateTaskRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_day: Option<bool>,
    pub priority: TickTickItemPriority,
}

/// Request body for updating a task via TickTick API
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickUpdateTaskRequest {
    pub id: String,
    pub project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<TickTickItemPriority>,
}

/// Request body for completing a task via TickTick API
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickCompleteTaskRequest {
    pub id: String,
    pub project_id: String,
}

impl TickTickService {
    pub fn new(
        ticktick_base_url: Option<String>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
        max_retry_duration: Duration,
    ) -> Result<TickTickService, UniversalInboxError> {
        let ticktick_base_url = ticktick_base_url.unwrap_or_else(|| TICKTICK_BASE_URL.to_string());
        let ticktick_base_path = Url::parse(&ticktick_base_url)
            .context("Cannot parse TickTick base URL")?
            .path()
            .to_string();

        Ok(TickTickService {
            ticktick_base_url,
            ticktick_base_path: if &ticktick_base_path == "/" {
                "".to_string()
            } else {
                ticktick_base_path
            },
            projects_cache_index: Arc::new(AtomicU64::new(0)),
            integration_connection_service,
            max_retry_duration,
        })
    }

    pub async fn mock_all(mock_server: &MockServer) {
        // Mock GET /project - list all projects
        Mock::given(method("GET"))
            .and(path("/open/v1/project"))
            .respond_with(ResponseTemplate::new(200).set_body_json::<Vec<TickTickProject>>(vec![]))
            .mount(mock_server)
            .await;

        // Mock GET /project/{projectId}/task - list tasks
        // For testing, return empty array
        Mock::given(method("GET"))
            .and(path("/open/v1/project/inbox/task"))
            .respond_with(ResponseTemplate::new(200).set_body_json::<Vec<TickTickItem>>(vec![]))
            .mount(mock_server)
            .await;

        // Mock POST /task - create task
        Mock::given(method("POST"))
            .and(path("/open/v1/task"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(json!({
                        "id": "mock_task_id",
                        "projectId": "inbox",
                        "title": "Mock task",
                        "priority": 0,
                        "status": 0,
                    })),
            )
            .mount(mock_server)
            .await;
    }

    fn build_ticktick_client(
        &self,
        access_token: &AccessToken,
    ) -> Result<ApiClient, UniversalInboxError> {
        let mut headers = HeaderMap::new();

        let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);

        ApiClient::build(
            headers,
            [
                format!("{}/task", self.ticktick_base_path),
                format!("{}/project", self.ticktick_base_path),
            ],
            self.max_retry_duration,
        )
    }

    /// List all projects for the authenticated user
    pub async fn list_projects(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TickTickProject>, UniversalInboxError> {
        Ok(self
            .build_ticktick_client(access_token)?
            .get(format!("{}/project", self.ticktick_base_url))
            .await
            .context("Failed to list TickTick projects")?)
    }

    /// Fetch all projects with caching
    pub async fn fetch_all_projects(
        &self,
        user_id: UserId,
        access_token: &AccessToken,
    ) -> Result<Vec<TickTickProject>, UniversalInboxError> {
        cached_fetch_all_ticktick_projects(self, user_id, access_token).await
    }

    /// Get a single task by project_id and task_id
    pub async fn get_task(
        &self,
        project_id: &str,
        task_id: &str,
        access_token: &AccessToken,
    ) -> Result<Option<TickTickItem>, UniversalInboxError> {
        match self
            .build_ticktick_client(access_token)?
            .get::<TickTickItem, _>(format!(
                "{}/project/{}/task/{}",
                self.ticktick_base_url, project_id, task_id
            ))
            .await
        {
            Ok(item) => Ok(Some(item)),
            Err(ApiClientError::NetworkError(err))
                if err.status() == Some(reqwest_middleware::reqwest::StatusCode::NOT_FOUND) =>
            {
                Ok(None)
            }
            Err(err) => Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot get task {task_id} from TickTick API: {err}"
            ))),
        }
    }

    /// List all tasks for all projects (used for full sync)
    pub async fn list_all_tasks(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TickTickItem>, UniversalInboxError> {
        let projects = self
            .build_ticktick_client(access_token)?
            .get::<Vec<TickTickProject>, _>(format!("{}/project", self.ticktick_base_url))
            .await
            .context("Failed to list TickTick projects for task sync")?;

        let mut all_tasks = Vec::new();
        let client = self.build_ticktick_client(access_token)?;

        for project in &projects {
            let tasks: Vec<TickTickItem> = client
                .get(format!(
                    "{}/project/{}/task",
                    self.ticktick_base_url, project.id
                ))
                .await
                .with_context(|| {
                    format!("Failed to list tasks for TickTick project {}", project.id)
                })?;
            all_tasks.extend(tasks);
        }

        Ok(all_tasks)
    }

    /// Create a task in TickTick
    pub async fn create_ticktick_task(
        &self,
        request: &TickTickCreateTaskRequest,
        access_token: &AccessToken,
    ) -> Result<TickTickItem, UniversalInboxError> {
        Ok(self
            .build_ticktick_client(access_token)?
            .post(format!("{}/task", self.ticktick_base_url), Some(request))
            .await
            .context("Failed to create TickTick task")?)
    }

    /// Update a task in TickTick
    pub async fn update_ticktick_task(
        &self,
        task_id: &str,
        request: &TickTickUpdateTaskRequest,
        access_token: &AccessToken,
    ) -> Result<TickTickItem, UniversalInboxError> {
        Ok(self
            .build_ticktick_client(access_token)?
            .post(
                format!("{}/task/{}", self.ticktick_base_url, task_id),
                Some(request),
            )
            .await
            .context("Failed to update TickTick task")?)
    }

    /// Complete a task in TickTick
    pub async fn complete_ticktick_task(
        &self,
        project_id: &str,
        task_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        self.build_ticktick_client(access_token)?
            .post_no_response(
                format!(
                    "{}/project/{}/task/{}/complete",
                    self.ticktick_base_url, project_id, task_id
                ),
                Option::<&()>::None,
            )
            .await
            .context("Failed to complete TickTick task")?;
        Ok(())
    }

    /// Delete a task in TickTick
    pub async fn delete_ticktick_task(
        &self,
        project_id: &str,
        task_id: &str,
        access_token: &AccessToken,
    ) -> Result<(), UniversalInboxError> {
        self.build_ticktick_client(access_token)?
            .delete_no_response(format!(
                "{}/project/{}/task/{}",
                self.ticktick_base_url, project_id, task_id
            ))
            .await
            .context("Failed to delete TickTick task")?;
        Ok(())
    }

    /// Create a project in TickTick
    pub async fn create_ticktick_project(
        &self,
        name: &str,
        access_token: &AccessToken,
    ) -> Result<TickTickProject, UniversalInboxError> {
        Ok(self
            .build_ticktick_client(access_token)?
            .post(
                format!("{}/project", self.ticktick_base_url),
                Some(&json!({ "name": name })),
            )
            .await
            .context("Failed to create TickTick project")?)
    }

    #[allow(dead_code, clippy::blocks_in_conditions)]
    async fn fetch_task_by_source_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_id: &str,
        project_id: &str,
        user_id: UserId,
    ) -> Result<Option<TickTickItem>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch a TickTick task without an access token"))?;
        self.get_task(project_id, source_id, &access_token).await
    }

    pub async fn build_task_with_project_name(
        source: &TickTickItem,
        project_name: String,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Box<CreateOrUpdateTaskRequest> {
        Box::new(CreateOrUpdateTaskRequest {
            id: Uuid::new_v4().into(),
            title: source.title.clone(),
            body: source.content.clone().unwrap_or_default(),
            status: if source.is_completed() {
                TaskStatus::Done
            } else {
                TaskStatus::Active
            },
            completed_at: source.completed_time,
            priority: source.priority.into(),
            due_at: DefaultValue::new(None, Some(source.get_due_date())),
            tags: source.tags.clone().unwrap_or_default(),
            parent_id: None,
            project: DefaultValue::new(TICKTICK_INBOX_PROJECT.to_string(), Some(project_name)),
            is_recurring: source.is_recurring(),
            created_at: source
                .created_time
                .unwrap_or_else(|| Utc::now().with_nanosecond(0).unwrap()),
            updated_at: source_third_party_item.updated_at,
            kind: TaskSourceKind::TickTick,
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
    convert = r#"{ format!("{}{}{}", _user_id, service.projects_cache_index.load(Ordering::Relaxed), service.ticktick_base_url.clone()) }"#
)]
async fn cached_fetch_all_ticktick_projects(
    service: &TickTickService,
    _user_id: UserId,
    access_token: &AccessToken,
) -> Result<Vec<TickTickProject>, UniversalInboxError> {
    service.list_projects(access_token).await
}

#[async_trait]
impl ThirdPartyItemSourceService<TickTickItem> for TickTickService {
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
        _last_sync_completed_at: Option<DateTime<Utc>>,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch TickTick tasks without an access token"))?;

        let items = self.list_all_tasks(&access_token).await?;

        // Update the context with the latest sync timestamp
        self.integration_connection_service
            .read()
            .await
            .update_integration_connection_context(
                executor,
                integration_connection.id,
                IntegrationConnectionContext::TickTick(TickTickContext {
                    last_sync_at: Some(Utc::now()),
                }),
            )
            .await
            .map_err(|_| {
                anyhow!(
                    "Failed to update TickTick integration connection {} context",
                    integration_connection.id
                )
            })?;

        Ok(items
            .into_iter()
            .map(|item| item.into_third_party_item(user_id, integration_connection.id))
            .collect())
    }

    fn is_sync_incremental(&self) -> bool {
        // TickTick V1 API does not support incremental sync
        false
    }

    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::TickTick
    }
}

#[async_trait]
impl ThirdPartyTaskService<TickTickItem> for TickTickService {
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
        source: &TickTickItem,
        source_third_party_item: &ThirdPartyItem,
        _task_creation_config: Option<TaskCreationConfig>,
        user_id: UserId,
    ) -> Result<Box<CreateOrUpdateTaskRequest>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot build a TickTick task without an access token"))?;
        let projects = self.fetch_all_projects(user_id, &access_token).await?;
        let project_name = projects
            .iter()
            .find(|project| project.id == source.project_id)
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "No project".to_string());

        Ok(TickTickService::build_task_with_project_name(
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
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot delete a TickTick task without an access token"))?;

        let ticktick_item: TickTickItem = third_party_item.clone().try_into()?;
        self.delete_ticktick_task(
            &ticktick_item.project_id,
            &third_party_item.source_id,
            &access_token,
        )
        .await
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
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot complete a TickTick task without an access token"))?;

        let ticktick_item: TickTickItem = third_party_item.clone().try_into()?;
        self.complete_ticktick_task(
            &ticktick_item.project_id,
            &third_party_item.source_id,
            &access_token,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            third_party_item_id = _third_party_item.id.to_string(),
            third_party_item_source_id = _third_party_item.source_id,
            user.id = _user_id.to_string()
        ),
        err
    )]
    async fn uncomplete_task(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _third_party_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // TickTick V1 API does not support uncompleting a task
        Err(UniversalInboxError::UnsupportedAction(
            "TickTick API does not support uncompleting a task".to_string(),
        ))
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
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot update a TickTick task without an access token"))?;

        // We need the current task to get project_id (required by TickTick API)
        // First, try to resolve project_id from a project name if provided
        let project_id = if let Some(ref project_name) = patch.project_name {
            let project = self
                .get_or_create_project(executor, project_name, user_id, Some(&access_token))
                .await?;
            project.source_id.to_string()
        } else {
            // We need to find the current project_id for this task.
            // The task_id alone isn't enough for TickTick's update API which needs project_id.
            // We'll fetch projects and search for the task across them.
            // For now, use an empty string and let the API resolve it.
            // In practice, the caller should provide the project context.
            String::new()
        };

        let title = patch.title.clone();
        let content = patch.body.clone();
        let priority = patch.priority.map(|p| p.into());
        let due_date = patch
            .due_at
            .as_ref()
            .and_then(|due| due.as_ref().map(|d| d.to_string()));

        let update_request = TickTickUpdateTaskRequest {
            id: id.to_string(),
            project_id,
            title,
            content,
            due_date,
            priority,
        };

        self.update_ticktick_task(id, &update_request, &access_token)
            .await?;

        Ok(())
    }
}

#[async_trait]
impl ThirdPartyTaskSourceService<TickTickItem> for TickTickService {
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
    ) -> Result<TickTickItem, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot create a TickTick task without an access token"))?;

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

        let due_date = task.due_at.as_ref().map(|due| due.to_string());
        let all_day = task
            .due_at
            .as_ref()
            .map(|due| matches!(due, universal_inbox::task::DueDate::Date(_)));

        let create_request = TickTickCreateTaskRequest {
            title: task.title.clone(),
            content: task.body.clone(),
            project_id,
            due_date,
            all_day,
            priority: task.priority.into(),
        };

        self.create_ticktick_task(&create_request, &access_token)
            .await
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
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
        else {
            return Ok(vec![]);
        };

        let projects = self.fetch_all_projects(user_id, &access_token).await?;
        let search_regex = RegexBuilder::new(matches)
            .case_insensitive(true)
            .size_limit(100_000)
            .build()
            .context(format!(
                "Failed to build regular expression from `{matches}`"
            ))?;

        Ok(projects
            .into_iter()
            .filter(|ticktick_project| search_regex.is_match(&ticktick_project.name))
            .map(|ticktick_project| ProjectSummary {
                source_id: ticktick_project.id.into(),
                name: ticktick_project.name,
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
                    .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
                    .await?
                    .ok_or_else(|| {
                        anyhow!(
                            "Cannot create TickTick project {project_name} without an access token"
                        )
                    })?
                    .0
            }
        };

        let projects = self.fetch_all_projects(user_id, &access_token).await?;
        if let Some(project) = projects
            .iter()
            .find(|project| project.name == *project_name)
        {
            return Ok(ProjectSummary {
                source_id: project.id.clone().into(),
                name: project.name.clone(),
            });
        }

        let new_project = self
            .create_ticktick_project(project_name, &access_token)
            .await?;
        self.projects_cache_index.fetch_add(1, Ordering::Relaxed);

        Ok(ProjectSummary {
            source_id: new_project.id.into(),
            name: new_project.name,
        })
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<TickTickItem> for TickTickService {
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
        source: &TickTickItem,
        source_third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<Box<Notification>, UniversalInboxError> {
        Ok(Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: source.title.clone(),
            status: if source.is_completed() {
                NotificationStatus::Deleted
            } else {
                NotificationStatus::Unread
            },
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            user_id,
            kind: NotificationSourceKind::TickTick,
            source_item: source_third_party_item.clone(),
            task_id: None,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(third_party_item_id = _source_item.id.to_string(), user.id = _user_id.to_string()),
        err
    )]
    async fn delete_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        unimplemented!("TickTick notifications cannot be deleted, only TickTick Task can");
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
        unimplemented!("TickTick notifications cannot be unsubscribed, only TickTick Task can");
    }

    async fn snooze_notification_from_source(
        &self,
        _executor: &mut Transaction<'_, Postgres>,
        _source_item: &ThirdPartyItem,
        _snoozed_until_at: DateTime<Utc>,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // TickTick notifications cannot be snoozed => no-op
        Ok(())
    }
}

impl TaskSource for TickTickService {
    fn get_task_source_kind(&self) -> TaskSourceKind {
        TaskSourceKind::TickTick
    }
}

impl IntegrationProviderSource for TickTickService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::TickTick
    }
}

impl NotificationSource for TickTickService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::TickTick
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        false
    }
}
