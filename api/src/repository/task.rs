use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{postgres::PgRow, types::Json, FromRow, Postgres, QueryBuilder, Row, Transaction};
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    task::{
        service::TaskPatch, CreateOrUpdateTaskRequest, DueDate, Task, TaskId, TaskPriority,
        TaskSourceKind, TaskStatus, TaskSummary,
    },
    user::UserId,
    Page,
};

use crate::universal_inbox::{UniversalInboxError, UpdateStatus, UpsertStatus};

use super::{third_party::ThirdPartyItemRow, FromRowWithPrefix, Repository};

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
    async fn search_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        matches: &str,
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
    async fn update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        patch: &TaskPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError>;
}

#[async_trait]
impl TaskRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        field(task_id = id.to_string()),
        err
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
                  task.body as task__body,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data as task__source_item__data,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data as task__sink_item__data,
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

        row.map(|task_row| task_row.try_into()).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        field(task_id = id.to_string()),
        err
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

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
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
                  task.body as task__body,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data as task__source_item__data,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data as task__sink_item__data,
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

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        field(
            status = status.to_string(),
            only_synced_tasks,
            user.id = user_id.to_string()
        ),
        err
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
                  task.body as task__body,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data as task__source_item__data,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data as task__sink_item__data,
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

        query_builder.push(" ORDER BY task.updated_at ASC LIMIT 100");

        let rows = query_builder
            .build_query_as::<TaskRow>()
            .fetch_all(&mut **executor)
            .await
            .map_err(|err| {
                let message = format!("Failed to fetch tasks from storage: {err}");
                UniversalInboxError::DatabaseError {
                    source: err,
                    message,
                }
            })?;

        Ok(Page {
            page: 1,
            per_page: 100,
            total: count.try_into().unwrap(), // count(*) cannot be negative
            content: rows
                .iter()
                .map(|r| r.try_into())
                .collect::<Result<Vec<Task>, UniversalInboxError>>()?,
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        field(matches, user.id = user_id.to_string()),
        err
    )]
    async fn search_tasks(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        matches: &str,
        user_id: UserId,
    ) -> Result<Vec<TaskSummary>, UniversalInboxError> {
        // TODO: cleanup, only keep [a-zA-Z0-9]
        let ts_query = matches
            .split_whitespace()
            .map(|word| format!("{word}:*"))
            .collect::<Vec<String>>()
            .join(" & ");

        let rows = sqlx::query_as!(
            TaskSummaryRow,
            r#"
                SELECT
                  task.id,
                  sink_item.source_id,
                  task.title,
                  task.body,
                  task.priority,
                  task.due_at as "due_at: Json<Option<DueDate>>",
                  task.tags,
                  task.project
                FROM
                  task,
                  to_tsquery('english', $1) query,
                  third_party_item sink_item
                WHERE
                  query @@ title_body_project_tags_tsv
                  AND task.status::TEXT = 'Active'
                  AND task.user_id = $2
                  AND task.sink_item_id = sink_item.id
                ORDER BY ts_rank_cd(title_body_project_tags_tsv, query) DESC
                LIMIT 10;
            "#,
            ts_query,
            user_id.0
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
        fields(task_id = task.id.to_string()),
        err
    )]
    async fn create_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task: Box<Task>,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let priority: u8 = task.priority.into();

        sqlx::query!(
            r#"
                INSERT INTO task
                  (
                    id,
                    title,
                    body,
                    status,
                    completed_at,
                    priority,
                    due_at,
                    tags,
                    parent_id,
                    project,
                    is_recurring,
                    created_at,
                    updated_at,
                    kind,
                    source_item_id,
                    sink_item_id,
                    user_id
                  )
                VALUES
                  ($1, $2, $3, $4::task_status, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14::task_kind, $15, $16, $17)
            "#,
            task.id.0,
            task.title,
            task.body,
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
            task.user_id.0
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
            kind = kind.to_string(),
            status = status.to_string(),
            user.id = user_id.to_string()
        ),
        err
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
                  task.body as task__body,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data as task__source_item__data,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data as task__sink_item__data,
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
            task_id = task_request.id.to_string(),
            task_kind = task_request.kind.to_string(),
            task_source_item_id = task_request.source_item.id.to_string(),
            user.id = task_request.user_id.to_string()
        ),
        err
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
                task.body as task__body,
                task.status as task__status,
                task.completed_at as task__completed_at,
                task.priority as task__priority,
                task.due_at as task__due_at,
                task.tags as task__tags,
                task.parent_id as task__parent_id,
                task.project as task__project,
                task.is_recurring as task__is_recurring,
                task.created_at as task__created_at,
                task.updated_at as task__updated_at,
                task.kind::TEXT as task__kind,
                task.user_id as task__user_id,
                source_item.id as task__source_item__id,
                source_item.source_id as task__source_item__source_id,
                source_item.data as task__source_item__data,
                source_item.created_at as task__source_item__created_at,
                source_item.updated_at as task__source_item__updated_at,
                source_item.user_id as task__source_item__user_id,
                source_item.integration_connection_id as task__source_item__integration_connection_id,
                sink_item.id as task__sink_item__id,
                sink_item.source_id as task__sink_item__source_id,
                sink_item.data as task__sink_item__data,
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
        let parent_id = task_request.parent_id.map(|id| id.0);

        if let Some(existing_task) = existing_task {
            if existing_task == (*task_request.clone()).into() {
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
            separated
                .push("title = ")
                .push_bind_unseparated(task_request.title.clone());
            separated
                .push("body = ")
                .push_bind_unseparated(task_request.body.clone());
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
            separated
                .push("parent_id = ")
                .push_bind_unseparated(parent_id);
            if task_request.project.has_value() {
                separated
                    .push("project = ")
                    .push_bind_unseparated(task_request.project.clone().into_value());
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
                    due_at: task_request
                        .due_at
                        .value
                        .clone()
                        .unwrap_or_else(|| existing_task.due_at.clone()),
                    project: task_request
                        .project
                        .value
                        .clone()
                        .unwrap_or_else(|| existing_task.project.clone()),
                    ..Into::<Task>::into(*task_request)
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
                    body,
                    status,
                    completed_at,
                    priority,
                    due_at,
                    tags,
                    parent_id,
                    project,
                    is_recurring,
                    created_at,
                    updated_at,
                    kind,
                    source_item_id,
                    sink_item_id,
                    user_id
                  )
                VALUES
                  ($1, $2, $3, $4::task_status, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14::task_kind, $15, $16, $17)
                RETURNING
                  id
            "#,
                    task_request.id.0,
                    task_request.title,
                    task_request.body,
                    task_request.status.to_string() as _,
                    completed_at,
                    priority as i32,
                    Json(task_request.due_at.clone().into_value()) as Json<Option<DueDate>>,
                    &task_request.tags,
                    parent_id,
                    task_request.project.clone().into_value(),
                    task_request.is_recurring,
                    task_request.created_at.naive_utc(),
                    task_request.updated_at.naive_utc(),
                    task_request.kind.to_string() as _,
                    task_request.source_item.id.0,
                    task_request.sink_item.as_ref().map(|item| item.id.0),
                    task_request.user_id.0
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
        fields(
            task_id = task_id.to_string(),
            patch,
            user.id = for_user_id.to_string()
        ),
        err
    )]
    async fn update_task(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        task_id: TaskId,
        patch: &TaskPatch,
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Missing `status` field value to update task {task_id}"),
            });
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

        if let Some(project) = &patch.project {
            separated
                .push(" project = ")
                .push_bind_unseparated(project.to_string());
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
            separated.push(" body = ").push_bind_unseparated(body);
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
                  task.body as task__body,
                  task.status as task__status,
                  task.completed_at as task__completed_at,
                  task.priority as task__priority,
                  task.due_at as task__due_at,
                  task.tags as task__tags,
                  task.parent_id as task__parent_id,
                  task.project as task__project,
                  task.is_recurring as task__is_recurring,
                  task.created_at as task__created_at,
                  task.updated_at as task__updated_at,
                  task.kind::TEXT as task__kind,
                  task.user_id as task__user_id,
                  source_item.id as task__source_item__id,
                  source_item.source_id as task__source_item__source_id,
                  source_item.data as task__source_item__data,
                  source_item.created_at as task__source_item__created_at,
                  source_item.updated_at as task__source_item__updated_at,
                  source_item.user_id as task__source_item__user_id,
                  source_item.integration_connection_id as task__source_item__integration_connection_id,
                  sink_item.id as task__sink_item__id,
                  sink_item.source_id as task__sink_item__source_id,
                  sink_item.data as task__sink_item__data,
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

        if let Some(project) = &patch.project {
            separated
                .push(" project != ")
                .push_bind_unseparated(project.to_string());
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

        if let Some(body) = &patch.body {
            separated.push(" body != ").push_bind_unseparated(body);
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
            Ok(UpdateStatus {
                updated: updated_task_row.is_updated,
                result: Some(Box::new(updated_task_row.task_row.try_into().unwrap())),
            })
        } else {
            Ok(UpdateStatus {
                updated: false,
                result: None,
            })
        }
    }
}

#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "task_status")]
enum PgTaskStatus {
    Active,
    Done,
    Deleted,
}

#[derive(Debug)]
pub struct TaskRow {
    id: Uuid,
    title: String,
    body: String,
    status: PgTaskStatus,
    completed_at: Option<NaiveDateTime>,
    priority: i32,
    due_at: Json<Option<DueDate>>,
    tags: Vec<String>,
    parent_id: Option<Uuid>,
    project: String,
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
        Ok(TaskRow {
            id: row.try_get(format!("{prefix}id").as_str())?,
            title: row.try_get(format!("{prefix}title").as_str())?,
            body: row.try_get(format!("{prefix}body").as_str())?,
            status: row.try_get::<PgTaskStatus, &str>(format!("{prefix}status").as_str())?,
            completed_at: row.try_get(format!("{prefix}completed_at").as_str())?,
            priority: row.try_get(format!("{prefix}priority").as_str())?,
            due_at: row.try_get(format!("{prefix}due_at").as_str())?,
            tags: row.try_get(format!("{prefix}tags").as_str())?,
            parent_id: row.try_get(format!("{prefix}parent_id").as_str())?,
            project: row.try_get(format!("{prefix}project").as_str())?,
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
    body: String,
    priority: i32,
    due_at: Json<Option<DueDate>>,
    tags: Vec<String>,
    project: String,
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
            body: row.body.to_string(),
            priority,
            due_at: row.due_at.0.clone(),
            tags: row.tags.clone(),
            project: row.project.to_string(),
        })
    }
}
