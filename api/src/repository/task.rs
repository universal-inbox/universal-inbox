use std::collections::HashMap;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{FromRow, Postgres, QueryBuilder, Row, Transaction, postgres::PgRow, types::Json};
use tracing::{debug, warn};
use uuid::Uuid;

use universal_inbox::{
    DEFAULT_PAGE_SIZE, Page, PageToken,
    context::{ContextId, summarize_contexts},
    integration_connection::provider::IntegrationProviderKind,
    task::{
        CreateOrUpdateTaskRequest, DueDate, Task, TaskId, TaskPriority, TaskProject,
        TaskSourceKind, TaskStatus, TaskSummary, service::TaskPatch,
    },
    third_party::{
        integrations::ticktick::TickTickList,
        item::{ThirdPartyItemData, ThirdPartyItemKind},
    },
    user::UserId,
};

use crate::observability::attr;
use crate::universal_inbox::{UniversalInboxError, UpdateStatus, UpsertStatus};
use crate::utils::crypto::{aad, data_keyring};

use super::{
    FromRowWithPrefix, Repository,
    context::ContextRepository,
    decode_rows_skipping_invalid,
    third_party::{ThirdPartyItemRow, decrypt_third_party_item_data},
};

/// Outcome of binding existing Todoist tasks to their project contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskContextBackfill {
    /// Tasks bound by this run
    pub bound: u64,
    /// Todoist tasks still without a context (project deleted upstream, or no context yet)
    pub unmatched: u64,
    /// Todoist tasks whose Todoist item could not be read
    pub unreadable: u64,
}

#[async_trait]
pub trait TaskRepository {
    async fn get_one_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: TaskId,
    ) -> Result<Option<Task>, UniversalInboxError>;
    async fn does_task_exist(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: TaskId,
    ) -> Result<bool, UniversalInboxError>;
    async fn get_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn fetch_all_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: TaskStatus,
        only_synced_tasks: bool,
        user_id: UserId,
    ) -> Result<Page<Task>, UniversalInboxError>;
    /// Every task of the user, whatever its status, for the user data export
    async fn fetch_all_tasks_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn search_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        matches: &str,
        provider_kind: Option<IntegrationProviderKind>,
        user_id: UserId,
    ) -> Result<Vec<TaskSummary>, UniversalInboxError>;
    async fn create_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task: Box<Task>,
    ) -> Result<Box<Task>, UniversalInboxError>;
    async fn update_stale_tasks_status_from_source_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        active_source_task_ids: Vec<String>,
        kind: TaskSourceKind,
        status: TaskStatus,
        user_id: UserId,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn create_or_update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task: Box<CreateOrUpdateTaskRequest>,
    ) -> Result<UpsertStatus<Box<Task>>, UniversalInboxError>;
    /// Bind the user's tasks living in `task_manager` (Todoist or TickTick) to the context of
    /// their project, by id: the task's item in it (its sink, or its source for a task of that
    /// task manager) carries the `project_id` the context is bound on. A task's `context_id` is
    /// set or corrected, never cleared, so a rerun changes nothing. Never matches on a name.
    async fn backfill_task_context_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        task_manager: IntegrationProviderKind,
    ) -> Result<TaskContextBackfill, UniversalInboxError>;
    /// The user's tasks whose item of `item_kind` (their source or their sink, e.g. a Linear task
    /// synced to TickTick) has one of `source_ids` upstream, by that id
    async fn find_task_ids_by_item_source_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        item_kind: ThirdPartyItemKind,
        source_ids: &[String],
    ) -> Result<HashMap<String, TaskId>, UniversalInboxError>;
    /// Set the parent of the task `task_id`. `false`: it already had that parent.
    async fn set_task_parent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        parent_id: Option<TaskId>,
    ) -> Result<bool, UniversalInboxError>;
    /// `project`: the project to move the task to, resolved by the service from
    /// `patch.destination`, which the repository does not read
    async fn update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        patch: &TaskPatch,
        project: Option<&TaskProject>,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError>;
    async fn get_task_for_source_item_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item_id: Uuid,
        user_id: UserId,
    ) -> Result<Option<Task>, UniversalInboxError>;
}

impl Repository {
    /// Fill `task.context` (name, path, status, inbox flag) from the context `task.context_id`
    /// points to, for the reads served to clients. One query for all the tasks: their contexts
    /// and the ancestors of those, never the whole tree. A context of another user is ignored.
    pub(crate) async fn hydrate_task_contexts<'a>(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        tasks: impl IntoIterator<Item = &'a mut Task>,
    ) -> Result<(), UniversalInboxError> {
        let tasks: Vec<&mut Task> = tasks.into_iter().collect();
        let mut context_ids: Vec<ContextId> =
            tasks.iter().filter_map(|task| task.context_id).collect();
        if context_ids.is_empty() {
            return Ok(());
        }
        context_ids.sort_by_key(|id| id.0);
        context_ids.dedup();

        let contexts = self
            .list_contexts_with_ancestors(executor, &context_ids)
            .await?;
        let summaries = summarize_contexts(&contexts);
        let owners: HashMap<ContextId, UserId> = contexts
            .iter()
            .map(|context| (context.id, context.user_id))
            .collect();
        for task in tasks {
            task.context = task
                .context_id
                .filter(|id| owners.get(id) == Some(&task.user_id))
                .and_then(|id| summaries.get(&id).cloned());
        }
        Ok(())
    }

    async fn hydrate_task_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        mut task: Task,
    ) -> Result<Task, UniversalInboxError> {
        self.hydrate_task_contexts(executor, [&mut task]).await?;
        Ok(task)
    }
}

#[async_trait]
impl TaskRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::TASK_ID } = id.to_string())
    )]
    async fn get_one_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: TaskId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        let row = QueryBuilder::new(
            r#"
                SELECT
                  task.id as task__id,
                  task.title as task__title,
                  task.body_enc as task__body_enc,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.context_id as task__context_id,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data_enc as task__source_item__data_enc,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data_enc as task__sink_item__data_enc,
                  sink_item.created_at as task__sink_item__created_at,
                  sink_item.updated_at as task__sink_item__updated_at,
                  sink_item.user_id as task__sink_item__user_id,
                  sink_item.integration_connection_id as task__sink_item__integration_connection_id
                FROM task
                INNER JOIN third_party_item AS source_item
                  ON task.source_item_id = source_item.id
                LEFT JOIN third_party_item AS sink_item
                  ON task.sink_item_id = sink_item.id
                WHERE task.id =
            "#,
        )
        .push_bind(id.0)
        .build_query_as::<TaskRow>()
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch task {id} from storage: {err}");
            UniversalInboxError::DatabaseError { source: err, message }
        })?;

        let task: Option<Task> = row.map(|task_row| task_row.try_into()).transpose()?;
        match task {
            Some(task) => Ok(Some(self.hydrate_task_context(executor, task).await?)),
            None => Ok(None),
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::TASK_ID } = id.to_string())
    )]
    async fn does_task_exist(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: TaskId,
    ) -> Result<bool, UniversalInboxError> {
        let count: Option<i64> =
            sqlx::query_scalar!(r#"SELECT count(*) FROM task WHERE id = $1"#, id.0)
                .fetch_one(&mut **executor)
                .await
                .map_err(|err| {
                    let message = format!("Failed to check if task {id} exists: {err}");
                    UniversalInboxError::DatabaseError {
                        source: err,
                        message,
                    }
                })?;

        if let Some(1) = count {
            return Ok(true);
        }
        return Ok(false);
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::TASK_IDS } = ?ids))]
    async fn get_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let uuids: Vec<Uuid> = ids.into_iter().map(|id| id.0).collect();
        let rows = QueryBuilder::new(
            r#"
                SELECT
                  task.id as task__id,
                  task.title as task__title,
                  task.body_enc as task__body_enc,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.context_id as task__context_id,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data_enc as task__source_item__data_enc,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data_enc as task__sink_item__data_enc,
                  sink_item.created_at as task__sink_item__created_at,
                  sink_item.updated_at as task__sink_item__updated_at,
                  sink_item.user_id as task__sink_item__user_id,
                  sink_item.integration_connection_id as task__sink_item__integration_connection_id
                FROM task
                INNER JOIN third_party_item AS source_item
                  ON task.source_item_id = source_item.id
                LEFT JOIN third_party_item AS sink_item
                  ON task.sink_item_id = sink_item.id
                WHERE id = any(
            "#,
        )
        .push_bind(&uuids[..])
        .push(")")
        .build_query_as::<TaskRow>()
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to fetch tasks {uuids:?} from storage: {err}");
            UniversalInboxError::DatabaseError { source: err, message }
        })?;

        let mut tasks = rows
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()?;
        self.hydrate_task_contexts(executor, tasks.iter_mut())
            .await?;
        Ok(tasks)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::TASK_STATUS } = status.to_string(),
            { attr::TASK_LIST_ONLY_SYNCED } = only_synced_tasks,
            { attr::USER_ID } = user_id.to_string()
        )
    )]
    async fn fetch_all_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        status: TaskStatus,
        only_synced_tasks: bool,
        user_id: UserId,
    ) -> Result<Page<Task>, UniversalInboxError> {
        fn add_filters(
            query_builder: &mut QueryBuilder<Postgres>,
            status: TaskStatus,
            only_synced_tasks: bool,
            user_id: UserId,
        ) {
            let mut separated = query_builder.separated(" AND ");
            separated
                .push("task.status::TEXT = ")
                .push_bind_unseparated(status.to_string());

            if only_synced_tasks {
                separated.push("task.source_item_id != task.sink_item_id");
            }

            separated
                .push(" task.user_id = ")
                .push_bind_unseparated(user_id.0);
        }

        let mut count_query_builder = QueryBuilder::new(r#"SELECT count(*) FROM task WHERE "#);

        add_filters(&mut count_query_builder, status, only_synced_tasks, user_id);

        let count = count_query_builder
            .build_query_scalar::<i64>()
            .fetch_one(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch tasks count from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  task.id as task__id,
                  task.title as task__title,
                  task.body_enc as task__body_enc,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.context_id as task__context_id,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data_enc as task__source_item__data_enc,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data_enc as task__sink_item__data_enc,
                  sink_item.created_at as task__sink_item__created_at,
                  sink_item.updated_at as task__sink_item__updated_at,
                  sink_item.user_id as task__sink_item__user_id,
                  sink_item.integration_connection_id as task__sink_item__integration_connection_id
                FROM task
                INNER JOIN third_party_item AS source_item
                  ON task.source_item_id = source_item.id
                LEFT JOIN third_party_item AS sink_item
                  ON task.sink_item_id = sink_item.id
                WHERE
            "#,
        );

        add_filters(&mut query_builder, status, only_synced_tasks, user_id);

        query_builder
            .push(" ORDER BY task.updated_at ASC LIMIT ")
            .push_bind(DEFAULT_PAGE_SIZE as i64);

        let rows = query_builder
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch tasks from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        let total: usize = count.try_into().unwrap(); // count(*) cannot be negative
        let mut content = decode_rows_skipping_invalid::<TaskRow>(&rows, "task__id", "task")
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()?;
        self.hydrate_task_contexts(executor, content.iter_mut())
            .await?;
        Ok(Page {
            per_page: DEFAULT_PAGE_SIZE,
            pages_count: total.div_ceil(DEFAULT_PAGE_SIZE),
            total,
            previous_page_token: content.first().map(|t| PageToken::Before(t.updated_at)),
            next_page_token: content.last().map(|t| PageToken::After(t.updated_at)),
            content,
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn fetch_all_tasks_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  task.id as task__id,
                  task.title as task__title,
                  task.body_enc as task__body_enc,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.context_id as task__context_id,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data_enc as task__source_item__data_enc,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data_enc as task__sink_item__data_enc,
                  sink_item.created_at as task__sink_item__created_at,
                  sink_item.updated_at as task__sink_item__updated_at,
                  sink_item.user_id as task__sink_item__user_id,
                  sink_item.integration_connection_id as task__sink_item__integration_connection_id
                FROM task
                INNER JOIN third_party_item AS source_item
                  ON task.source_item_id = source_item.id
                LEFT JOIN third_party_item AS sink_item
                  ON task.sink_item_id = sink_item.id
                WHERE
            "#,
        );
        query_builder
            .push(" task.user_id = ")
            .push_bind(user_id.0)
            .push(" ORDER BY task.created_at ASC");

        let rows = query_builder
            .build()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message =
                    format!("Failed to fetch all tasks of user {user_id} from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        let mut tasks = decode_rows_skipping_invalid::<TaskRow>(&rows, "task__id", "task")
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()?;
        self.hydrate_task_contexts(executor, tasks.iter_mut())
            .await?;
        Ok(tasks)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn search_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        matches: &str,
        provider_kind: Option<IntegrationProviderKind>,
        user_id: UserId,
    ) -> Result<Vec<TaskSummary>, UniversalInboxError> {
        // TODO: cleanup, only keep [a-zA-Z0-9]
        let ts_query = matches
            .split_whitespace()
            .map(|word| format!("{word}:*"))
            .collect::<Vec<String>>()
            .join(" & ");

        let provider_kind_filter = provider_kind.map(|kind| kind.to_string());

        let rows = sqlx::query_as!(
            TaskSummaryRow,
            r#"
                SELECT
                  task.id,
                  sink_item.source_id,
                  task.title,
                  task.body_enc as "body_enc!",
                  task.priority,
                  task.due_at as "due_at: Json<Option<DueDate>>",
                  task.tags,
                  task.context_id,
                  context.name as "context_name?"
                FROM
                  task
                  LEFT JOIN context
                    ON context.id = task.context_id AND context.user_id = task.user_id,
                  to_tsquery('english', $1) query,
                  third_party_item sink_item
                WHERE
                  query @@ title_project_tags_tsv
                  AND task.status::TEXT = 'Active'
                  AND task.user_id = $2
                  AND task.sink_item_id = sink_item.id
                  AND ($3::TEXT IS NULL OR task.kind::TEXT = $3)
                ORDER BY ts_rank_cd(title_project_tags_tsv, query) DESC
                LIMIT 10;
            "#,
            ts_query,
            user_id.0,
            provider_kind_filter,
        )
        .fetch_all(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!("Failed to search tasks from storage: {err}");
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<TaskSummary>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::TASK_ID } = task.id.to_string())
    )]
    async fn create_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task: Box<Task>,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let priority: u8 = task.priority.into();
        let body_enc = encrypt_task_body(&task.body, task.id)?;

        sqlx::query!(
            r#"
                INSERT INTO task
                  (
                    id,
                    title,
                    body_enc,
                    status,
                    completed_at,
                    priority,
                    due_at,
                    tags,
                    parent_id,
                    project,
                    context_id,
                    is_recurring,
                    created_at,
                    updated_at,
                    kind,
                    source_item_id,
                    sink_item_id,
                    user_id
                  )
                VALUES
                  ($1, $2, $3, $4::task_status, $5, $6, $7, $8, $9, $10, $18, $11, $12, $13, $14::task_kind, $15, $16, $17)
            "#,
            task.id.0,
            task.title,
            body_enc,
            task.status.to_string() as _,
            task.completed_at
                .map(|last_read_at| last_read_at.naive_utc()),
            priority as i32,
            Json(task.due_at.clone()) as Json<Option<DueDate>>,
            &task.tags,
            task.parent_id.map(|id| id.0),
            task.project,
            task.is_recurring,
            task.created_at.naive_utc(),
            task.updated_at.naive_utc(),
            task.kind.to_string() as _,
            task.source_item.id.0,
            task.sink_item
                .as_ref()
                .map(|item| item.id.0),
            task.user_id.0,
            task.context_id.map(|id| id.0)
        )
        .execute(&mut **executor)
        .await
        .map_err(|e| {
            let error_code = e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()));
            match error_code {
                // PG `unique_violation` error
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: Some(e),
                    id: task.id.0,
                },
                // PG `check_violation` error
                Some(x) if x == *"23514" => UniversalInboxError::InvalidInputData {
                    source: Some(e),
                    user_error: "Submitted task is invalid".to_string(),
                },
                _ => UniversalInboxError::Unexpected(anyhow!(
                    "Failed to insert new task into storage: {e}"
                )),
            }
        })?;

        Ok(task)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::SYNC_SOURCE_KIND } = kind.to_string(),
            { attr::TASK_STATUS } = status.to_string(),
            { attr::USER_ID } = user_id.to_string()
        )
    )]
    async fn update_stale_tasks_status_from_source_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        active_source_task_ids: Vec<String>,
        kind: TaskSourceKind,
        status: TaskStatus,
        user_id: UserId,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let completed_at = if status == TaskStatus::Done {
            Some(Utc::now().naive_utc())
        } else {
            None
        };

        let mut query_builder = QueryBuilder::new("UPDATE task SET");

        let mut separated = query_builder.separated(", ");
        separated
            .push(" status::TEXT = ")
            .push_bind_unseparated(status.to_string());
        separated
            .push(" completed_at = ")
            .push_bind_unseparated(completed_at);

        query_builder.push(
            r#"
                FROM
                  task as t
                INNER JOIN third_party_item AS source_item
                  ON t.source_item_id = source_item.id
                LEFT JOIN third_party_item AS sink_item
                  ON t.sink_item_id = sink_item.id
                WHERE
                "#,
        );

        let mut separated = query_builder.separated(" AND ");
        separated.push(" task.id = t.id ");
        separated
            .push(" NOT source_item.source_id = ANY(")
            .push_bind_unseparated(&active_source_task_ids[..])
            .push_unseparated(")");
        separated
            .push(" task.kind::TEXT = ")
            .push_bind_unseparated(kind.to_string());
        separated.push(" task.status = 'Active'");
        separated
            .push(" task.user_id = ")
            .push_bind_unseparated(user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  task.id as task__id,
                  task.title as task__title,
                  task.body_enc as task__body_enc,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.context_id as task__context_id,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data_enc as task__source_item__data_enc,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data_enc as task__sink_item__data_enc,
                  sink_item.created_at as task__sink_item__created_at,
                  sink_item.updated_at as task__sink_item__updated_at,
                  sink_item.user_id as task__sink_item__user_id,
                  sink_item.integration_connection_id as task__sink_item__integration_connection_id
              "#,
        );

        let rows = query_builder
            .build_query_as::<TaskRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to update stale tasks status from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::TASK_ID } = task_request.id.to_string(),
            { attr::TASK_KIND } = task_request.kind.to_string(),
            { attr::THIRD_PARTY_ITEM_ID } = task_request.source_item.id.to_string(),
            { attr::USER_ID } = task_request.user_id.to_string()
        )
    )]
    async fn create_or_update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_request: Box<CreateOrUpdateTaskRequest>,
    ) -> Result<UpsertStatus<Box<Task>>, UniversalInboxError> {
        let priority: u8 = task_request.priority.into();

        let mut query_builder = QueryBuilder::new(
            r#"
              SELECT
                task.id as task__id,
                task.title as task__title,
                task.body_enc as task__body_enc,
                task.status as task__status,
                task.completed_at as task__completed_at,
                task.priority as task__priority,
                task.due_at as task__due_at,
                task.tags as task__tags,
                task.parent_id as task__parent_id,
                task.project as task__project,
                task.context_id as task__context_id,
                task.is_recurring as task__is_recurring,
                task.created_at as task__created_at,
                task.updated_at as task__updated_at,
                task.kind::TEXT as task__kind,
                task.user_id as task__user_id,
                source_item.id as task__source_item__id,
                source_item.source_id as task__source_item__source_id,
                source_item.data_enc as task__source_item__data_enc,
                source_item.created_at as task__source_item__created_at,
                source_item.updated_at as task__source_item__updated_at,
                source_item.user_id as task__source_item__user_id,
                source_item.integration_connection_id as task__source_item__integration_connection_id,
                sink_item.id as task__sink_item__id,
                sink_item.source_id as task__sink_item__source_id,
                sink_item.data_enc as task__sink_item__data_enc,
                sink_item.created_at as task__sink_item__created_at,
                sink_item.updated_at as task__sink_item__updated_at,
                sink_item.user_id as task__sink_item__user_id,
                sink_item.integration_connection_id as task__sink_item__integration_connection_id
              FROM task
              INNER JOIN third_party_item AS source_item
                ON task.source_item_id = source_item.id
              LEFT JOIN third_party_item AS sink_item
                ON task.sink_item_id = sink_item.id
              WHERE
            "#,
        );
        let mut separated = query_builder.separated(" AND ");
        // Built `task` could already exist as a source_item or a sink_item
        separated
            .push(" ((source_item.id = ")
            .push_bind_unseparated(task_request.source_item.id.0)
            .push_unseparated(" AND ")
            .push_unseparated(" source_item.kind::TEXT = ")
            .push_bind_unseparated(task_request.source_item.kind().to_string())
            .push_unseparated(") OR (sink_item.id = ")
            .push_bind_unseparated(task_request.source_item.id.0)
            .push_unseparated(" AND ")
            .push_unseparated(" sink_item.kind::TEXT = ")
            .push_bind_unseparated(task_request.source_item.kind().to_string())
            .push_unseparated(")) ");
        separated
            .push(" task.user_id = ")
            .push_bind_unseparated(task_request.user_id.0);

        let existing_task: Option<Task> = query_builder
            .build_query_as::<TaskRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!(
                    "Failed to search for task with source ID {} from storage: {err}",
                    task_request.source_item.source_id
                );
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?
            .map(TryInto::try_into)
            .transpose()?;

        let completed_at = task_request
            .completed_at
            .map(|completed_at| completed_at.naive_utc());

        if let Some(existing_task) = existing_task {
            // The request only *asserts* the fields the integration owns. For the
            // declinable ones (`title`, `due_at`, `project`) an absent value means
            // "keep what is stored", so the comparison target — and the `new` task
            // reported back — resolve against `existing_task` rather than against
            // the request's seed. This keeps the equality check in agreement with
            // what the `UPDATE` below actually writes.
            let requested_task: Task = Task {
                title: task_request
                    .title
                    .clone()
                    .value_or(existing_task.title.clone()),
                due_at: task_request
                    .due_at
                    .clone()
                    .value_or(existing_task.due_at.clone()),
                project: task_request
                    .project
                    .clone()
                    .value_or(existing_task.project.clone()),
                // Only the integration owning the binding changes it
                context_id: task_request
                    .context_id
                    .clone()
                    .value_or(existing_task.context_id),
                // Set by the parent pass of the sync once the batch is stored, never here
                parent_id: existing_task.parent_id,
                ..(*task_request.clone()).into()
            };

            if existing_task == requested_task {
                debug!(
                    "Existing {} task {} (from {}) for {} does not need updating: {:?}",
                    existing_task.kind,
                    existing_task.id,
                    existing_task.source_item.source_id,
                    existing_task.user_id,
                    existing_task.updated_at
                );
                return Ok(UpsertStatus::Untouched(Box::new(existing_task)));
            }

            if existing_task.kind != task_request.kind {
                // Built task may be a sink item after all
                debug!(
                    "Updating existing {} task {} (from sink item {}) for {}",
                    existing_task.kind,
                    existing_task.id,
                    task_request.source_item.source_id,
                    task_request.user_id
                );
            } else {
                debug!(
                    "Updating existing {} task {} (from source item {}) for {}: {:?}",
                    task_request.kind,
                    existing_task.id,
                    task_request.source_item.source_id,
                    task_request.user_id,
                    task_request.status
                );
            }

            let mut query_builder = QueryBuilder::new("UPDATE task SET ");
            let mut separated = query_builder.separated(", ");
            if task_request.title.has_value() {
                separated
                    .push("title = ")
                    .push_bind_unseparated(task_request.title.clone().into_value());
            }
            separated
                .push("body_enc = ")
                .push_bind_unseparated(encrypt_task_body(&task_request.body, existing_task.id)?);
            separated
                .push("status = ")
                .push_bind_unseparated(task_request.status.to_string())
                .push_unseparated("::task_status");
            separated
                .push("completed_at = ")
                .push_bind_unseparated(completed_at);
            separated
                .push("priority = ")
                .push_bind_unseparated(priority as i32);
            if task_request.due_at.has_value() {
                separated
                    .push("due_at = ")
                    .push_bind_unseparated(
                        Json(task_request.due_at.clone().into_value()) as Json<Option<DueDate>>
                    );
            }
            separated
                .push("tags = ")
                .push_bind_unseparated(&task_request.tags);
            if task_request.project.has_value() {
                separated
                    .push("project = ")
                    .push_bind_unseparated(task_request.project.clone().into_value());
            }
            if let Some(context_id) = &task_request.context_id.value {
                separated
                    .push("context_id = ")
                    .push_bind_unseparated(context_id.map(|id| id.0));
            }
            separated
                .push("is_recurring = ")
                .push_bind_unseparated(task_request.is_recurring);
            separated
                .push("updated_at = ")
                .push_bind_unseparated(task_request.updated_at.naive_utc());

            query_builder
                .push(" WHERE id = ")
                .push_bind(existing_task.id.0);

            query_builder
                .build()
                .execute(&mut **executor)
                .await
                .map_err(|err| {
                    let message = format!(
                        "Failed to update task {} from storage: {err}",
                        existing_task.id
                    );
                    UniversalInboxError::DatabaseError {
                        source: err,
                        message,
                    }
                })?;

            return Ok(UpsertStatus::Updated {
                new: Box::new(Task {
                    id: existing_task.id,
                    kind: existing_task.kind,
                    created_at: existing_task.created_at,
                    source_item: existing_task.source_item.clone(),
                    sink_item: existing_task.sink_item.clone(),
                    ..requested_task
                }),
                old: Box::new(existing_task),
            });
        }

        debug!(
            "Creating new {} task {} (from source item {}) for {}",
            task_request.kind,
            task_request.id,
            task_request.source_item.source_id,
            task_request.user_id
        );
        let task_id: TaskId = TaskId(
                sqlx::query_scalar!(
                    r#"
                INSERT INTO task
                  (
                    id,
                    title,
                    body_enc,
                    status,
                    completed_at,
                    priority,
                    due_at,
                    tags,
                    parent_id,
                    project,
                    context_id,
                    is_recurring,
                    created_at,
                    updated_at,
                    kind,
                    source_item_id,
                    sink_item_id,
                    user_id
                  )
                VALUES
                  ($1, $2, $3, $4::task_status, $5, $6, $7, $8, $9, $10, $18, $11, $12, $13, $14::task_kind, $15, $16, $17)
                RETURNING
                  id
            "#,
                    task_request.id.0,
                    // No stored task to fall back to: the seed *is* the value.
                    task_request.title.clone().into_value(),
                    encrypt_task_body(&task_request.body, task_request.id)?,
                    task_request.status.to_string() as _,
                    completed_at,
                    priority as i32,
                    Json(task_request.due_at.clone().into_value()) as Json<Option<DueDate>>,
                    &task_request.tags,
                    // A new task starts at the top level: the nest pass of the sync sets its parent
                    None::<Uuid>,
                    task_request.project.clone().into_value(),
                    task_request.is_recurring,
                    task_request.created_at.naive_utc(),
                    task_request.updated_at.naive_utc(),
                    task_request.kind.to_string() as _,
                    task_request.source_item.id.0,
                    task_request.sink_item.as_ref().map(|item| item.id.0),
                    task_request.user_id.0,
                    task_request.context_id.clone().into_value().map(|id| id.0)
                )
                .fetch_one(&mut **executor)
                .await
                    .map_err(|err| {
                        let message = format!(
                            "Failed to update task with source ID {} from storage: {err}",
                            task_request.source_item.source_id
                        );
                        UniversalInboxError::DatabaseError { source: err, message }
                })?,
            );

        Ok(UpsertStatus::Created(Box::new(Task {
            id: task_id,
            ..Into::<Task>::into(*task_request)
        })))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn backfill_task_context_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        task_manager: IntegrationProviderKind,
    ) -> Result<TaskContextBackfill, UniversalInboxError> {
        let item_kind = match task_manager {
            IntegrationProviderKind::Todoist => ThirdPartyItemKind::TodoistItem,
            IntegrationProviderKind::TickTick => ThirdPartyItemKind::TickTickItem,
            _ => {
                return Err(UniversalInboxError::Unexpected(anyhow!(
                    "{task_manager} tasks are not bound to contexts"
                )));
            }
        };
        // The project id sits in the encrypted item data, out of SQL's reach: decrypt in
        // application code and match on the id only.
        let contexts: HashMap<String, Uuid> = sqlx::query!(
            r#"
                SELECT id, binding_source_id as "binding_source_id!"
                FROM context
                WHERE user_id = $1
                  AND binding_provider = $2::integration_provider_kind
                  AND binding_mirrored
            "#,
            user_id.0,
            task_manager.to_string() as _
        )
        .fetch_all(&mut **executor)
        .await
        .with_context(|| format!("Failed to fetch {task_manager} contexts from storage"))?
        .into_iter()
        .map(|row| (row.binding_source_id, row.id))
        .collect();

        // Every task living in the task manager is checked, not only the unbound ones: a task
        // moved to a project whose context did not exist at the time keeps its old binding until
        // the context appears. A Linear or Slack task synced to it lives there through its sink
        // item, a task of the task manager through its source item (also its sink): the sink is
        // read first. Bounded by the user's task count.
        let candidates = sqlx::query!(
            r#"
                SELECT DISTINCT ON (task.id)
                  task.id as task_id,
                  task.context_id,
                  third_party_item.id as item_id,
                  third_party_item.data_enc as "data_enc!"
                FROM task
                JOIN third_party_item
                  ON third_party_item.id IN (task.sink_item_id, task.source_item_id)
                WHERE task.user_id = $1
                  AND third_party_item.kind::TEXT = $2
                ORDER BY task.id, third_party_item.id = task.sink_item_id DESC NULLS LAST
            "#,
            user_id.0,
            item_kind.to_string()
        )
        .fetch_all(&mut **executor)
        .await
        .with_context(|| format!("Failed to fetch {task_manager} tasks from storage"))?;

        let mut backfill = TaskContextBackfill {
            bound: 0,
            unmatched: 0,
            unreadable: 0,
        };
        for candidate in candidates {
            let project_id =
                match decrypt_third_party_item_data(&candidate.data_enc, candidate.item_id) {
                    Ok(ThirdPartyItemData::TodoistItem(item)) => Some(item.project_id),
                    Ok(ThirdPartyItemData::TickTickItem(item)) => {
                        Some(TickTickList::of_project_id(&item.project_id).source_id().0)
                    }
                    Ok(_) => None,
                    Err(err) => {
                        warn!(
                            { attr::USER_ID } = user_id.to_string(),
                            "Cannot read the {task_manager} item of task {}: {err}",
                            candidate.task_id
                        );
                        backfill.unreadable += 1;
                        continue;
                    }
                };
            // Never clears a binding: no context for the project leaves the task as it is
            let resolved = project_id.and_then(|id| contexts.get(&id));
            match (resolved, candidate.context_id) {
                (Some(context_id), current) if current != Some(*context_id) => {
                    backfill.bound += sqlx::query!(
                        "UPDATE task SET context_id = $1 WHERE id = $2",
                        context_id,
                        candidate.task_id
                    )
                    .execute(&mut **executor)
                    .await
                    .with_context(|| {
                        format!("Failed to bind a {task_manager} task to its context")
                    })?
                    .rows_affected();
                }
                (None, None) => backfill.unmatched += 1,
                _ => {}
            }
        }

        if backfill.unmatched > 0 {
            warn!(
                { attr::USER_ID } = user_id.to_string(),
                "{} {task_manager} task(s) have no context for their project and stay unbound",
                backfill.unmatched
            );
        }
        Ok(backfill)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn find_task_ids_by_item_source_ids(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        item_kind: ThirdPartyItemKind,
        source_ids: &[String],
    ) -> Result<HashMap<String, TaskId>, UniversalInboxError> {
        if source_ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(sqlx::query!(
            r#"
                SELECT DISTINCT task.id, third_party_item.source_id
                FROM task
                JOIN third_party_item
                  ON third_party_item.id IN (task.source_item_id, task.sink_item_id)
                WHERE task.user_id = $1
                  AND third_party_item.kind::TEXT = $2
                  AND third_party_item.source_id = ANY($3)
            "#,
            user_id.0,
            item_kind.to_string(),
            source_ids
        )
        .fetch_all(&mut **executor)
        .await
        .context("Failed to fetch tasks by source id from storage")?
        .into_iter()
        .map(|row| (row.source_id, TaskId(row.id)))
        .collect())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::TASK_ID } = task_id.to_string())
    )]
    async fn set_task_parent(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        parent_id: Option<TaskId>,
    ) -> Result<bool, UniversalInboxError> {
        Ok(sqlx::query!(
            r#"
                UPDATE task SET parent_id = $1
                WHERE id = $2 AND parent_id IS DISTINCT FROM $1
            "#,
            parent_id.map(|id| id.0),
            task_id.0
        )
        .execute(&mut **executor)
        .await
        .context("Failed to set the parent of a task")?
        .rows_affected()
            > 0)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::TASK_ID } = task_id.to_string(),
            { attr::USER_ID } = for_user_id.to_string()
        )
    )]
    async fn update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        patch: &TaskPatch,
        project: Option<&TaskProject>,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Missing `status` field value to update task {task_id}"),
            });
        };

        // The encrypted body cannot be compared in SQL to tell whether it changed
        let body_changed = match &patch.body {
            Some(body) => {
                let current_body_enc: Option<Vec<u8>> = sqlx::query_scalar!(
                    "SELECT body_enc FROM task WHERE id = $1 FOR UPDATE",
                    task_id.0
                )
                .fetch_optional(&mut **executor)
                .await
                .map_err(|err| {
                    let message = format!("Failed to fetch task {task_id} from storage: {err}");
                    UniversalInboxError::DatabaseError {
                        source: err,
                        message,
                    }
                })?;
                Some(match current_body_enc {
                    Some(body_enc) => decrypt_task_body(&body_enc, task_id.0)? != *body,
                    None => true,
                })
            }
            None => None,
        };

        let mut query_builder = QueryBuilder::new("UPDATE task SET");
        let mut separated = query_builder.separated(", ");
        if let Some(status) = patch.status {
            separated
                .push(" status = ")
                .push_bind_unseparated(status.to_string())
                .push_unseparated("::task_status");
            if status == TaskStatus::Done {
                separated
                    .push(" completed_at = ")
                    .push_bind_unseparated(Some(Utc::now().naive_utc()));
            }
        }

        if let Some(project) = project {
            separated
                .push(" context_id = ")
                .push_bind_unseparated(project.context_id.map(|id| id.0));
            // Deprecated column, kept filled until it is dropped
            separated
                .push(" project = ")
                .push_bind_unseparated(project.project_name.clone());
        }

        if let Some(due_at) = &patch.due_at {
            separated
                .push(" due_at = ")
                .push_bind_unseparated(Json(due_at.clone()) as Json<Option<DueDate>>);
        }

        if let Some(priority) = patch.priority {
            separated
                .push(" priority = ")
                .push_bind_unseparated(priority as i32);
        }

        if let Some(body) = &patch.body {
            separated
                .push(" body_enc = ")
                .push_bind_unseparated(encrypt_task_body(body, task_id)?);
        }

        if let Some(title) = &patch.title {
            separated.push(" title = ").push_bind_unseparated(title);
        }

        if let Some(sink_item_id) = &patch.sink_item_id {
            separated
                .push(" sink_item_id = ")
                .push_bind_unseparated(sink_item_id.0);
        }

        query_builder
            .push(
                r#"
                FROM
                  task as t
                INNER JOIN third_party_item AS source_item
                  ON t.source_item_id = source_item.id
                LEFT JOIN third_party_item AS sink_item
                  ON t.sink_item_id = sink_item.id
                WHERE
              "#,
            )
            .separated(" AND ")
            .push(" task.id = t.id ")
            .push(" task.id = ")
            .push_bind_unseparated(task_id.0)
            .push(" task.user_id = ")
            .push_bind_unseparated(for_user_id.0);

        query_builder.push(
            r#"
                RETURNING
                  task.id as task__id,
                  task.title as task__title,
                  task.body_enc as task__body_enc,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.context_id as task__context_id,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data_enc as task__source_item__data_enc,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data_enc as task__sink_item__data_enc,
                  sink_item.created_at as task__sink_item__created_at,
                  sink_item.updated_at as task__sink_item__updated_at,
                  sink_item.user_id as task__sink_item__user_id,
                  sink_item.integration_connection_id as task__sink_item__integration_connection_id,
                  (SELECT"#,
        );

        let mut separated = query_builder.separated(" OR ");
        if let Some(status) = patch.status {
            separated
                .push(" status::TEXT != ")
                .push_bind_unseparated(status.to_string());
        }

        if let Some(project) = project {
            separated
                .push(" context_id IS DISTINCT FROM ")
                .push_bind_unseparated(project.context_id.map(|id| id.0));
            separated
                .push(" project != ")
                .push_bind_unseparated(project.project_name.clone());
        }

        if let Some(sink_item_id) = &patch.sink_item_id {
            separated
                .push(" (sink_item_id is NULL OR sink_item_id != ")
                .push_bind_unseparated(sink_item_id.0)
                .push_unseparated(")");
        }

        if let Some(due_at_value) = &patch.due_at {
            if let Some(due_at) = due_at_value {
                separated
                    .push(" due_at->>'content' != ")
                    .push_bind_unseparated(due_at.to_string());
            } else {
                separated.push(" due_at IS NOT NULL");
            }
        }

        if let Some(priority) = patch.priority {
            separated
                .push(" priority != ")
                .push_bind_unseparated(priority as i32);
        }

        if let Some(body_changed) = body_changed {
            // The encrypted body cannot be compared in SQL
            separated.push(if body_changed { " TRUE" } else { " FALSE" });
        }

        if let Some(title) = &patch.title {
            separated.push(" title != ").push_bind_unseparated(title);
        }

        query_builder
            .push(" FROM task WHERE id = ")
            .push_bind(task_id.0);
        query_builder.push(r#") as "is_updated""#);

        let row: Option<UpdatedTaskRow> = query_builder
            .build_query_as::<UpdatedTaskRow>()
            .fetch_optional(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to update task {task_id} from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        if let Some(updated_task_row) = row {
            let task: Task = updated_task_row.task_row.try_into()?;
            Ok(UpdateStatus {
                updated: updated_task_row.is_updated,
                result: Some(Box::new(self.hydrate_task_context(executor, task).await?)),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::THIRD_PARTY_ITEM_ID } = source_item_id.to_string(),
            { attr::USER_ID } = user_id.to_string()
        )
    )]
    async fn get_task_for_source_item_id(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        source_item_id: Uuid,
        user_id: UserId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        let row = QueryBuilder::new(
            r#"
                SELECT
                  task.id as task__id,
                  task.title as task__title,
                  task.body_enc as task__body_enc,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.context_id as task__context_id,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data_enc as task__source_item__data_enc,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data_enc as task__sink_item__data_enc,
                  sink_item.created_at as task__sink_item__created_at,
                  sink_item.updated_at as task__sink_item__updated_at,
                  sink_item.user_id as task__sink_item__user_id,
                  sink_item.integration_connection_id as task__sink_item__integration_connection_id
                FROM task
                INNER JOIN third_party_item AS source_item
                  ON task.source_item_id = source_item.id
                LEFT JOIN third_party_item AS sink_item
                  ON task.sink_item_id = sink_item.id
                WHERE task.source_item_id =
            "#,
        )
        .push_bind(source_item_id)
        .push(" AND task.user_id = ")
        .push_bind(user_id.0)
        .build_query_as::<TaskRow>()
        .fetch_optional(&mut **executor)
        .await
        .map_err(|err| {
            let message = format!(
                "Failed to fetch task for source_item_id {source_item_id} from storage: {err}"
            );
            UniversalInboxError::DatabaseError {
                source: err,
                message,
            }
        })?;

        row.map(|task_row| task_row.try_into()).transpose()
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "task_status")]
enum PgTaskStatus {
    Active,
    Done,
    Deleted,
}

const TASK_BODY_AAD: &str = "task.body";

fn encrypt_task_body(body: &str, id: TaskId) -> Result<Vec<u8>, UniversalInboxError> {
    data_keyring()?.encrypt(body.as_bytes(), &aad(TASK_BODY_AAD, id.0))
}

/// Decrypt `task.body_enc` of the task `id`
fn decrypt_task_body(body_enc: &[u8], id: Uuid) -> Result<String, UniversalInboxError> {
    data_keyring()?.decrypt_string(body_enc, &aad(TASK_BODY_AAD, id))
}

#[derive(Debug)]
pub struct TaskRow {
    id: Uuid,
    title: String,
    /// Decrypted from `task.body_enc`
    body: String,
    status: PgTaskStatus,
    completed_at: Option<NaiveDateTime>,
    priority: i32,
    due_at: Json<Option<DueDate>>,
    tags: Vec<String>,
    parent_id: Option<Uuid>,
    project: String,
    context_id: Option<Uuid>,
    is_recurring: bool,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    kind: String,
    source_item: ThirdPartyItemRow,
    sink_item: Option<ThirdPartyItemRow>,
    user_id: Uuid,
}

impl FromRow<'_, PgRow> for TaskRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        TaskRow::from_row_with_prefix(row, "task__")
    }
}

impl FromRowWithPrefix<'_, PgRow> for TaskRow {
    fn from_row_with_prefix(row: &PgRow, prefix: &str) -> sqlx::Result<Self> {
        let id: Uuid = row.try_get(format!("{prefix}id").as_str())?;
        let body_enc: Vec<u8> = row.try_get(format!("{prefix}body_enc").as_str())?;
        let body =
            decrypt_task_body(&body_enc, id).map_err(|err| sqlx::Error::Decode(Box::new(err)))?;
        Ok(TaskRow {
            id,
            title: row.try_get(format!("{prefix}title").as_str())?,
            body,
            status: row.try_get::<PgTaskStatus, &str>(format!("{prefix}status").as_str())?,
            completed_at: row.try_get(format!("{prefix}completed_at").as_str())?,
            priority: row.try_get(format!("{prefix}priority").as_str())?,
            due_at: row.try_get(format!("{prefix}due_at").as_str())?,
            tags: row.try_get(format!("{prefix}tags").as_str())?,
            parent_id: row.try_get(format!("{prefix}parent_id").as_str())?,
            project: row.try_get(format!("{prefix}project").as_str())?,
            context_id: row.try_get(format!("{prefix}context_id").as_str())?,
            is_recurring: row.try_get(format!("{prefix}is_recurring").as_str())?,
            created_at: row.try_get(format!("{prefix}created_at").as_str())?,
            updated_at: row.try_get(format!("{prefix}updated_at").as_str())?,
            kind: row.try_get(format!("{prefix}kind").as_str())?,
            user_id: row.try_get(format!("{prefix}user_id").as_str())?,
            source_item: ThirdPartyItemRow::from_row_with_prefix(
                row,
                format!("{prefix}source_item__").as_str(),
            )?,
            sink_item: row
                .try_get::<Option<Uuid>, &str>(format!("{prefix}sink_item__id").as_str())?
                .map(|_| {
                    ThirdPartyItemRow::from_row_with_prefix(
                        row,
                        format!("{prefix}sink_item__").as_str(),
                    )
                })
                .transpose()?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct UpdatedTaskRow {
    #[sqlx(flatten)]
    pub task_row: TaskRow,
    pub is_updated: bool,
}

impl TryFrom<TaskRow> for Task {
    type Error = UniversalInboxError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        (&row).try_into()
    }
}

impl TryFrom<&PgTaskStatus> for TaskStatus {
    type Error = UniversalInboxError;

    fn try_from(status: &PgTaskStatus) -> Result<Self, Self::Error> {
        let status_str = format!("{status:?}");
        status_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: status_str,
            })
    }
}

impl TryFrom<&TaskRow> for Task {
    type Error = UniversalInboxError;

    fn try_from(row: &TaskRow) -> Result<Self, Self::Error> {
        let status = (&row.status).try_into()?;
        let kind_str = row.kind.clone();
        let kind = kind_str
            .parse()
            .map_err(|e| UniversalInboxError::InvalidEnumData {
                source: e,
                output: kind_str,
            })?;
        let priority = TaskPriority::try_from(row.priority as u8)
            .with_context(|| format!("Failed to parse {} as TaskPriority", row.priority))?;

        Ok(Task {
            id: row.id.into(),
            title: row.title.to_string(),
            body: row.body.to_string(),
            status,
            completed_at: row
                .completed_at
                .map(|completed_at| DateTime::from_naive_utc_and_offset(completed_at, Utc)),
            priority,
            due_at: row.due_at.0.clone(),
            tags: row.tags.clone(),
            parent_id: row.parent_id.map(|id| id.into()),
            project: row.project.to_string(),
            context_id: row.context_id.map(|id| id.into()),
            // Filled by `Repository::hydrate_task_contexts`
            context: None,
            is_recurring: row.is_recurring,
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(row.updated_at, Utc),
            kind,
            source_item: row.source_item.clone().try_into()?,
            sink_item: row
                .sink_item
                .as_ref()
                .map(|item| item.try_into())
                .transpose()?,
            user_id: row.user_id.into(),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct TaskSummaryRow {
    id: Uuid,
    source_id: String,
    title: String,
    body_enc: Vec<u8>,
    priority: i32,
    due_at: Json<Option<DueDate>>,
    tags: Vec<String>,
    context_id: Option<Uuid>,
    context_name: Option<String>,
}

impl TryFrom<&TaskSummaryRow> for TaskSummary {
    type Error = UniversalInboxError;

    fn try_from(row: &TaskSummaryRow) -> Result<Self, Self::Error> {
        let priority = TaskPriority::try_from(row.priority as u8)
            .with_context(|| format!("Failed to parse {} as TaskPriority", row.priority))?;

        Ok(TaskSummary {
            id: row.id.into(),
            source_id: row.source_id.to_string(),
            title: row.title.to_string(),
            body: decrypt_task_body(&row.body_enc, row.id)?,
            priority,
            due_at: row.due_at.0.clone(),
            tags: row.tags.clone(),
            context_id: row.context_id.map(Into::into),
            context_name: row.context_name.clone(),
        })
    }
}
