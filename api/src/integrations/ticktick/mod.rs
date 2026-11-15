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
        CreateOrUpdateTaskRequest, DueDate, ProjectId, ProjectSummary, TaskCreation,
        TaskCreationConfig, TaskDestination, TaskSource, TaskSourceKind, TaskStatus,
        integrations::ticktick::{TICKTICK_INBOX_PROJECT, TickTickProject},
        is_ticktick_inbox_project_id,
        service::TaskPatch,
    },
    third_party::{
        integrations::ticktick::{
            TickTickItem, TickTickItemPriority, TickTickTag, TickTickTaskStatus,
        },
        item::{ThirdPartyItem, ThirdPartyItemFromSource, ThirdPartyItemSourceKind},
    },
    user::UserId,
    utils::default_value::DefaultValue,
};

use crate::observability::attr;
use crate::{
    integrations::{
        mock::MOCK_PROJECT_NAMES,
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

pub mod oauth;

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
/// Response shape of `GET /open/v1/project/{projectId}/data`.
/// Only the `tasks` field is used by the inbox sync.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickProjectData {
    #[serde(default)]
    pub tasks: Vec<TickTickItem>,
}

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
    pub tags: Option<Vec<TickTickTag>>,
}

/// Response shape of `GET /open/v1/tag` — TickTick's tag listing endpoint.
///
/// The endpoint returns one entry per user-defined tag with its display color
/// (a hex string like `#FF6161`). Older accounts / accounts without explicit
/// tag colors may omit the `color` field, hence the optional field.
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TickTickTagDetail {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
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
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_day: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub due_date: Option<Option<String>>,
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

/// Merge tag color metadata from `/open/v1/tag` into each task's tag list.
///
/// TickTick tag names are case-insensitive on their side but the API returns
/// them with their canonical casing. We do a case-insensitive lookup and
/// preserve the casing as it appears on each task. Tags with no matching
/// entry in the tag listing keep their existing color (typically `None`),
/// allowing graceful degradation for ad-hoc tags that haven't been registered
/// in the tag manager yet.
fn enrich_ticktick_item_tags(tasks: &mut [TickTickItem], tag_details: &[TickTickTagDetail]) {
    if tag_details.is_empty() {
        return;
    }

    use std::collections::HashMap;
    let color_by_name: HashMap<String, String> = tag_details
        .iter()
        .filter_map(|detail| {
            detail
                .color
                .as_ref()
                .map(|color| (detail.name.to_lowercase(), color.clone()))
        })
        .collect();

    if color_by_name.is_empty() {
        return;
    }

    for task in tasks {
        if let Some(tags) = task.tags.as_mut() {
            for tag in tags {
                if tag.color.is_some() {
                    continue;
                }
                if let Some(color) = color_by_name.get(&tag.name.to_lowercase()) {
                    tag.color = Some(color.clone());
                }
            }
        }
    }
}

impl TickTickService {
    /// URL of a TickTick API endpoint below the base URL. Each segment is
    /// percent-encoded, so an id containing `/`, `..`, `?` or `#` (item ids
    /// can be client-supplied) stays one path segment instead of retargeting
    /// the request made with the user's TickTick token.
    fn endpoint(&self, segments: &[&str]) -> Result<String, UniversalInboxError> {
        build_endpoint(&self.ticktick_base_url, segments)
    }

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
        let mock_projects: Vec<TickTickProject> = MOCK_PROJECT_NAMES
            .iter()
            .map(|name| TickTickProject {
                id: (*name).to_string(),
                name: (*name).to_string(),
                color: None,
                group_id: None,
                sort_order: None,
                view_mode: None,
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/project"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_projects))
            .mount(mock_server)
            .await;

        // Mock GET /tag - list user tags with color metadata. Returning an empty
        // list keeps tests deterministic; tests that care about colors override
        // this mock explicitly.
        Mock::given(method("GET"))
            .and(path("/tag"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json::<Vec<TickTickTagDetail>>(vec![]),
            )
            .mount(mock_server)
            .await;

        // Mock GET /project/inbox/data - inbox tasks fetched during full sync.
        Mock::given(method("GET"))
            .and(path("/project/inbox/data"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tasks": Vec::<TickTickItem>::new(),
                "columns": Vec::<serde_json::Value>::new(),
            })))
            .mount(mock_server)
            .await;

        // Mock POST /task - create task
        Mock::given(method("POST"))
            .and(path("/task"))
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

        let mut auth_header_value: HeaderValue =
            format!("Bearer {}", access_token.as_str()).parse().unwrap();
        auth_header_value.set_sensitive(true);
        headers.insert("Authorization", auth_header_value);
        // TickTick's edge returns a maintenance HTML page when no Accept header is
        // sent; explicitly asking for JSON forces the API to respond with JSON
        // (including JSON error bodies).
        headers.insert("Accept", HeaderValue::from_static("application/json"));

        ApiClient::build(
            headers,
            otel_known_paths(&self.ticktick_base_path),
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

    /// Fetch tag color metadata via `GET /open/v1/tag`.
    ///
    /// TickTick exposes the user's tag list with display colors here. Some
    /// accounts/scopes don't have access (the endpoint may 404 or 401), in
    /// which case we return an empty list and downstream sync proceeds with
    /// name-only tags. We never fail a sync just because tag metadata is
    /// unavailable — colors are decorative, not critical data.
    pub async fn list_tags(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TickTickTagDetail>, UniversalInboxError> {
        match self
            .build_ticktick_client(access_token)?
            .get::<Vec<TickTickTagDetail>, _>(format!("{}/tag", self.ticktick_base_url))
            .await
        {
            Ok(tags) => Ok(tags),
            Err(ApiClientError::NetworkError(err))
                if matches!(
                    err.status(),
                    Some(reqwest_middleware::reqwest::StatusCode::NOT_FOUND)
                        | Some(reqwest_middleware::reqwest::StatusCode::UNAUTHORIZED)
                        | Some(reqwest_middleware::reqwest::StatusCode::FORBIDDEN)
                ) =>
            {
                Ok(Vec::new())
            }
            Err(err) => Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot list tags from TickTick API: {err}"
            ))),
        }
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
            .get::<TickTickItem, _>(self.endpoint(&["project", project_id, "task", task_id])?)
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

    /// List all tasks for all projects (used for full sync).
    ///
    /// TickTick's "Inbox" is a special project that is NOT returned by
    /// `GET /open/v1/project` but whose tasks can be fetched via
    /// `GET /open/v1/project/inbox/data`. We pull both so users can see (and
    /// convert to notifications) the tasks they keep in their TickTick inbox.
    ///
    /// Task payloads only contain tag names. We additionally fetch the user's
    /// tag list (with color metadata) and merge each task's tag names against
    /// it so the persisted `TickTickItem` carries the source-native color used
    /// by the notification preview pane redesign.
    pub async fn list_all_tasks(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TickTickItem>, UniversalInboxError> {
        let client = self.build_ticktick_client(access_token)?;

        let projects: Vec<TickTickProject> = client
            .get(format!("{}/project", self.ticktick_base_url))
            .await
            .context("Failed to list TickTick projects for task sync")?;

        let mut all_tasks = Vec::new();

        // Inbox tasks — returned by the special /project/inbox/data endpoint.
        let inbox_data: TickTickProjectData = client
            .get(format!("{}/project/inbox/data", self.ticktick_base_url))
            .await
            .context("Failed to fetch TickTick inbox tasks for task sync")?;
        all_tasks.extend(inbox_data.tasks);

        // Per-project tasks — the /task endpoint used to work but now 404s
        // for user-created projects; /project/{id}/data is the stable shape.
        for project in &projects {
            let project_data: TickTickProjectData = client
                .get(self.endpoint(&["project", &project.id, "data"])?)
                .await
                .with_context(|| {
                    format!("Failed to list tasks for TickTick project {}", project.id)
                })?;
            all_tasks.extend(project_data.tasks);
        }

        // Enrich every task's tags with color metadata from the tag listing
        // endpoint. Best-effort: if /tag is unavailable we fall through and
        // store name-only tags exactly like before.
        let tag_details = self.list_tags(access_token).await.unwrap_or_default();
        if !tag_details.is_empty() {
            enrich_ticktick_item_tags(&mut all_tasks, &tag_details);
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
            .post(self.endpoint(&["task", task_id])?, Some(request))
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
                self.endpoint(&["project", project_id, "task", task_id, "complete"])?,
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
            .delete_no_response(self.endpoint(&["project", project_id, "task", task_id])?)
            .await
            .context("Failed to delete TickTick task")?;
        Ok(())
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
            // TickTick is the user's own task manager: it owns the title and a
            // rename is adopted on every sync.
            title: DefaultValue::new(source.title.clone(), Some(source.title.clone())),
            body: source.content.clone().unwrap_or_default(),
            status: if source.is_completed() {
                TaskStatus::Done
            } else {
                TaskStatus::Active
            },
            completed_at: source.completed_time,
            priority: source.priority.into(),
            due_at: DefaultValue::new(None, Some(source.get_due_date())),
            tags: source.tag_names(),
            parent_id: None,
            project: DefaultValue::new(TICKTICK_INBOX_PROJECT.to_string(), Some(project_name)),
            context_id: DefaultValue::new(None, None),
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
    sync_writes = "by_key",
    max_size = 1,
    ttl_secs = 600,
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
        fields({ attr::USER_ID } = user_id.to_string())
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
        // Inbox tasks have projectId `inbox{userTickTickId}` and are not
        // included in the /project list: display them as "Inbox". The inbox is
        // told by that id (`Task::is_in_inbox`), never by this display name.
        let project_name = if is_ticktick_inbox_project_id(&source.project_id) {
            TICKTICK_INBOX_PROJECT.to_string()
        } else {
            projects
                .iter()
                .find(|project| project.id == source.project_id)
                .map(|project| project.name.clone())
                .unwrap_or_else(|| "No project".to_string())
        };

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
            { attr::THIRD_PARTY_ITEM_ID } = third_party_item.id.to_string(),
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = third_party_item.source_id,
            { attr::USER_ID } = user_id.to_string()
        )
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
            { attr::THIRD_PARTY_ITEM_ID } = third_party_item.id.to_string(),
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = third_party_item.source_id,
            { attr::USER_ID } = user_id.to_string()
        )
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
            { attr::THIRD_PARTY_ITEM_ID } = _third_party_item.id.to_string(),
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = _third_party_item.source_id,
            { attr::USER_ID } = _user_id.to_string()
        )
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
            { attr::THIRD_PARTY_ITEM_SOURCE_ID } = third_party_item.source_id,
            { attr::USER_ID } = user_id.to_string()
        )
    )]
    async fn update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        third_party_item: &ThirdPartyItem,
        patch: &TaskPatch,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Only call the TickTick API if there are actual field changes to send.
        // Status changes (Deleted, Done, Active) are already handled by
        // delete_task/complete_task/uncomplete_task above.
        if patch.title.is_none()
            && patch.body.is_none()
            && patch.priority.is_none()
            && patch.due_at.is_none()
            && patch.destination.is_none()
        {
            return Ok(());
        }

        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot update a TickTick task without an access token"))?;

        // TickTick's `POST /open/v1/task/{taskId}` endpoint requires the task's
        // `projectId` in the body; sending an empty string makes the API create
        // a new task instead of updating the existing one. Resolve the
        // project_id either from an explicit move, or from the stored
        // TickTickItem data on the third-party item.
        let ticktick_item: TickTickItem = third_party_item.clone().try_into()?;
        let project_id = match Self::destination_project_id(patch.destination.as_ref())? {
            Some(project_id) => {
                self.resolve_project(project_id, user_id, &access_token)
                    .await?
                    .source_id
                    .0
            }
            None => ticktick_item.project_id.clone(),
        };

        let title = patch.title.clone();
        let content = patch.body.clone();
        let priority = patch.priority.map(|p| p.into());
        let due_date = patch
            .due_at
            .as_ref()
            .map(|due| due.as_ref().map(|d| d.to_string()));

        let update_request = TickTickUpdateTaskRequest {
            id: third_party_item.source_id.clone(),
            project_id,
            title,
            content,
            due_date,
            priority,
        };

        self.update_ticktick_task(&third_party_item.source_id, &update_request, &access_token)
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
        fields({ attr::USER_ID } = user_id.to_string())
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

        let project_id = match Self::destination_project_id(task.destination.as_ref())? {
            Some(project_id) => Some(
                self.resolve_project(project_id, user_id, &access_token)
                    .await?
                    .source_id
                    .0,
            ),
            None => None,
        };

        // Only attach the timezone when the due is a timezone-aware datetime
        // computed from a `time_config` (mirrors the Todoist conversion).
        let time_zone = match (task.due_at.as_ref(), &task.time_config) {
            (Some(DueDate::DateTimeWithTz(_)), Some(time_config)) => {
                Some(time_config.timezone.clone())
            }
            _ => None,
        };

        let duration_minutes = task.time_config.as_ref().and_then(|tc| tc.duration_minutes);

        // TickTick's API has no duration field; model a duration as a
        // `startDate`..`dueDate` range so the task shows as a timed block.
        let (start_date, due_date) = match (task.due_at.as_ref(), duration_minutes) {
            (Some(due @ DueDate::DateTimeWithTz(datetime)), Some(minutes)) => {
                let end = *datetime + chrono::Duration::minutes(minutes as i64);
                (
                    Some(due.to_string()),
                    Some(end.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                )
            }
            (due_at, _) => (None, due_at.map(|due| due.to_string())),
        };

        let all_day = task
            .due_at
            .as_ref()
            .map(|due| matches!(due, DueDate::Date(_)));

        let create_request = TickTickCreateTaskRequest {
            title: task.title.clone(),
            content: task.body.clone(),
            project_id,
            start_date,
            due_date,
            all_day,
            time_zone,
            priority: task.priority.into(),
        };

        self.create_ticktick_task(&create_request, &access_token)
            .await
    }
}

impl TickTickService {
    /// The user's TickTick projects whose name contains `matches`. TickTick projects are not
    /// mirrored as contexts yet: the TickTick picker searches them upstream.
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn search_projects(
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

        // A plain, case-insensitive substring: the user's input is never a pattern
        let matches = matches.to_lowercase();
        let projects = self.fetch_all_projects(user_id, &access_token).await?;
        Ok(projects
            .into_iter()
            .filter(|ticktick_project| ticktick_project.name.to_lowercase().contains(&matches))
            .map(|ticktick_project| ProjectSummary {
                source_id: ticktick_project.id.into(),
                name: ticktick_project.name,
            })
            .collect())
    }

    /// The TickTick project id of `destination`. `None` without a destination. A context is
    /// refused: TickTick projects are not mirrored as contexts yet.
    fn destination_project_id(
        destination: Option<&TaskDestination>,
    ) -> Result<Option<&ProjectId>, UniversalInboxError> {
        match destination {
            None => Ok(None),
            Some(TaskDestination::TickTickProject(project_id)) => Ok(Some(project_id)),
            Some(destination @ TaskDestination::Context(_)) => {
                Err(UniversalInboxError::UnresolvedTaskDestination(format!(
                    "A TickTick task cannot go to {destination}"
                )))
            }
        }
    }

    /// The TickTick project `project_id` (TickTick projects are not mirrored as contexts yet).
    /// An id TickTick does not know is an error: no project is ever created.
    pub async fn resolve_project(
        &self,
        project_id: &ProjectId,
        user_id: UserId,
        access_token: &AccessToken,
    ) -> Result<ProjectSummary, UniversalInboxError> {
        // The inbox is not in the project list
        if is_ticktick_inbox_project_id(&project_id.0) {
            return Ok(ProjectSummary {
                source_id: project_id.clone(),
                name: TICKTICK_INBOX_PROJECT.to_string(),
            });
        }
        self.fetch_all_projects(user_id, access_token)
            .await?
            .into_iter()
            .find(|project| project.id == project_id.0)
            .map(|project| ProjectSummary {
                source_id: project.id.into(),
                name: project.name,
            })
            .ok_or_else(|| {
                UniversalInboxError::UnresolvedTaskDestination(format!(
                    "TickTick project {project_id} does not exist"
                ))
            })
    }

    /// [`Self::resolve_project`], with the user's access token
    pub async fn resolve_project_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        project_id: &ProjectId,
        user_id: UserId,
    ) -> Result<ProjectSummary, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::TickTick, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot find a TickTick project without an access token"))?;
        self.resolve_project(project_id, user_id, &access_token)
            .await
    }
}

#[async_trait]
impl ThirdPartyNotificationSourceService<TickTickItem> for TickTickService {
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
        fields({ attr::THIRD_PARTY_ITEM_ID } = _source_item.id.to_string(), { attr::USER_ID } = _user_id.to_string())
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
        fields({ attr::THIRD_PARTY_ITEM_ID } = _source_item.id.to_string(), { attr::USER_ID } = _user_id.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::*;

    fn make_item(id: &str, tags: Option<Vec<TickTickTag>>) -> TickTickItem {
        TickTickItem {
            id: id.to_string(),
            project_id: "p".to_string(),
            title: "t".to_string(),
            content: None,
            desc: None,
            all_day: None,
            start_date: None,
            due_date: None,
            time_zone: None,
            reminders: None,
            repeat: None,
            priority: TickTickItemPriority::None,
            status: TickTickTaskStatus::Normal,
            completed_time: None,
            sort_order: None,
            items: None,
            tags,
            created_time: None,
            modified_time: None,
        }
    }

    #[rstest]
    fn test_enrich_ticktick_item_tags_populates_color_case_insensitive() {
        let mut tasks = vec![
            make_item(
                "task_a",
                Some(vec![
                    TickTickTag::new("Food"),
                    TickTickTag::new("shopping"),
                    TickTickTag::new("untracked"),
                ]),
            ),
            make_item("task_b", None),
        ];

        let tag_details = vec![
            TickTickTagDetail {
                name: "food".to_string(),
                color: Some("#FF6161".to_string()),
            },
            TickTickTagDetail {
                name: "Shopping".to_string(),
                color: Some("#3B82F6".to_string()),
            },
            // No `color` -> ignored.
            TickTickTagDetail {
                name: "untracked".to_string(),
                color: None,
            },
        ];

        enrich_ticktick_item_tags(&mut tasks, &tag_details);

        let enriched = tasks[0].tags.as_ref().unwrap();
        assert_eq!(enriched[0].name, "Food");
        assert_eq!(enriched[0].color.as_deref(), Some("#FF6161"));
        assert_eq!(enriched[1].name, "shopping");
        assert_eq!(enriched[1].color.as_deref(), Some("#3B82F6"));
        // Tag with no entry in the tag listing keeps its (None) color.
        assert_eq!(enriched[2].name, "untracked");
        assert_eq!(enriched[2].color, None);
        // Tasks without tags are left alone.
        assert_eq!(tasks[1].tags, None);
    }

    #[rstest]
    fn test_enrich_ticktick_item_tags_preserves_existing_color() {
        // If a tag was already enriched (e.g., loaded from JSONB storage),
        // a fresh sync should not overwrite the persisted color.
        let mut tasks = vec![make_item(
            "task_a",
            Some(vec![TickTickTag::with_color("Food", "#000000")]),
        )];
        let tag_details = vec![TickTickTagDetail {
            name: "food".to_string(),
            color: Some("#FF6161".to_string()),
        }];

        enrich_ticktick_item_tags(&mut tasks, &tag_details);

        assert_eq!(
            tasks[0].tags.as_ref().unwrap()[0].color.as_deref(),
            Some("#000000")
        );
    }

    #[rstest]
    fn test_enrich_ticktick_item_tags_no_op_when_listing_empty() {
        let mut tasks = vec![make_item("task_a", Some(vec![TickTickTag::new("Food")]))];
        enrich_ticktick_item_tags(&mut tasks, &[]);
        assert_eq!(tasks[0].tags.as_ref().unwrap()[0].color, None);
    }
}

/// Route templates used to name outbound request spans. Every path the client
/// calls must match one of them, otherwise the span is named `<METHOD> UNKNOWN`.
fn otel_known_paths(base_path: &str) -> Vec<String> {
    [
        "/task",
        "/task/{task_id}",
        "/tag",
        "/project",
        "/project/{project_id}/data",
        "/project/{project_id}/task/{task_id}",
        "/project/{project_id}/task/{task_id}/complete",
    ]
    .iter()
    .map(|path| format!("{base_path}{path}"))
    .collect()
}

/// See [`TickTickService::endpoint`].
fn build_endpoint(base_url: &str, segments: &[&str]) -> Result<String, UniversalInboxError> {
    let mut url = Url::parse(base_url).context("Cannot parse TickTick base URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("TickTick base URL cannot be a base"))?
        .pop_if_empty()
        .extend(segments);
    Ok(url.to_string())
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    use reqwest_tracing::OtelPathNames;
    use rstest::*;

    #[rstest]
    #[case("/open/v1/task", "/open/v1/task")]
    #[case("/open/v1/task/t1", "/open/v1/task/{task_id}")]
    #[case("/open/v1/tag", "/open/v1/tag")]
    #[case("/open/v1/project", "/open/v1/project")]
    #[case("/open/v1/project/inbox/data", "/open/v1/project/{project_id}/data")]
    #[case("/open/v1/project/p1/data", "/open/v1/project/{project_id}/data")]
    #[case(
        "/open/v1/project/p1/task/t1",
        "/open/v1/project/{project_id}/task/{task_id}"
    )]
    #[case(
        "/open/v1/project/p1/task/t1/complete",
        "/open/v1/project/{project_id}/task/{task_id}/complete"
    )]
    fn test_otel_known_paths_match_called_endpoints(
        #[case] path: &str,
        #[case] expected_template: &str,
    ) {
        let path_names = OtelPathNames::known_paths(otel_known_paths("/open/v1")).unwrap();
        assert_eq!(path_names.find(path), Some(expected_template));
    }

    fn endpoint(base: &str, segments: &[&str]) -> String {
        build_endpoint(base, segments).unwrap()
    }

    #[test]
    fn test_endpoint_segments_are_percent_encoded() {
        assert_eq!(
            endpoint(
                "https://api.ticktick.com/open/v1",
                &["project", "p1", "task", "t1"]
            ),
            "https://api.ticktick.com/open/v1/project/p1/task/t1"
        );
        assert_eq!(
            endpoint(
                "https://api.ticktick.com/open/v1/",
                &["project", "../../..", "task", "x?foo=1#frag"]
            ),
            "https://api.ticktick.com/open/v1/project/..%2F..%2F../task/x%3Ffoo=1%23frag"
        );
        // A bare dot segment cannot climb out of the API path either
        assert!(
            endpoint(
                "https://api.ticktick.com/open/v1",
                &["project", "..", "task", "t1"]
            )
            .starts_with("https://api.ticktick.com/open/v1/project/")
        );
    }
}
