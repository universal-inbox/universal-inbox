use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use http::Uri;
use sqlx::{types::Json, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use universal_inbox::task::{
    DueDate, Task, TaskId, TaskMetadata, TaskPatch, TaskPriority, TaskStatus,
};

use crate::{
    integrations::task::TaskSourceKind,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

use super::Repository;

#[async_trait]
pub trait TaskRepository {
    async fn get_one_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: TaskId,
    ) -> Result<Option<Task>, UniversalInboxError>;
    async fn get_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn fetch_all_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: TaskStatus,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn create_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: Box<Task>,
    ) -> Result<Box<Task>, UniversalInboxError>;
    async fn update_stale_tasks_status_from_source_ids<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        active_source_task_ids: Vec<String>,
        kind: TaskSourceKind,
        status: TaskStatus,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn create_or_update_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: Box<Task>,
    ) -> Result<Task, UniversalInboxError>;
    async fn update_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        patch: &'b TaskPatch,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError>;
}

#[async_trait]
impl TaskRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_one_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: TaskId,
    ) -> Result<Option<Task>, UniversalInboxError> {
        let row = sqlx::query_as!(
            TaskRow,
            r#"
                SELECT
                  id,
                  source_id,
                  title,
                  body,
                  status as "status: _",
                  completed_at,
                  priority,
                  due_at as "due_at: Json<Option<DueDate>>",
                  source_html_url,
                  tags,
                  parent_id,
                  project,
                  is_recurring,
                  created_at,
                  metadata as "metadata: Json<TaskMetadata>"
                FROM task
                WHERE id = $1
            "#,
            id.0
        )
        .fetch_optional(executor)
        .await
        .with_context(|| format!("Failed to fetch task {} from storage", id))?;

        row.map(|task_row| task_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let uuids: Vec<Uuid> = ids.into_iter().map(|id| id.0).collect();
        let rows = sqlx::query_as!(
            TaskRow,
            r#"
                SELECT
                  id,
                  source_id,
                  title,
                  body,
                  status as "status: _",
                  completed_at,
                  priority,
                  due_at as "due_at: Json<Option<DueDate>>",
                  source_html_url,
                  tags,
                  parent_id,
                  project,
                  is_recurring,
                  created_at,
                  metadata as "metadata: Json<TaskMetadata>"
                FROM task
                WHERE id = any($1)
            "#,
            &uuids[..]
        )
        .fetch_all(executor)
        .await
        .with_context(|| format!("Failed to fetch tasks {:?} from storage", uuids))?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn fetch_all_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: TaskStatus,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                  id,
                  source_id,
                  title,
                  body,
                  status,
                  completed_at,
                  priority,
                  due_at,
                  source_html_url,
                  tags,
                  parent_id,
                  project,
                  is_recurring,
                  created_at,
                  metadata
                FROM
                  task
                WHERE
                  status::TEXT =
            "#,
        );
        query_builder.push_bind(status.to_string());

        let rows = query_builder
            .build_query_as::<TaskRow>()
            .fetch_all(executor)
            .await
            .context("Failed to fetch tasks from storage")?;

        rows.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: Box<Task>,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let metadata = Json(task.metadata.clone());
        let priority: u8 = task.priority.into();

        sqlx::query!(
            r#"
                INSERT INTO task
                  (
                    id,
                    source_id,
                    title,
                    body,
                    status,
                    completed_at,
                    priority,
                    due_at,
                    source_html_url,
                    tags,
                    parent_id,
                    project,
                    is_recurring,
                    created_at,
                    metadata
                  )
                VALUES
                  ($1, $2, $3, $4, $5::task_status, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            "#,
            task.id.0,
            task.source_id,
            task.title,
            task.body,
            task.status.to_string() as _,
            task.completed_at
                .map(|last_read_at| last_read_at.naive_utc()),
            priority as i32,
            Json(task.due_at.clone()) as Json<Option<DueDate>>,
            task.source_html_url.as_ref().map(|url| url.to_string()),
            &task.tags,
            task.parent_id.map(|id| id.0),
            task.project,
            task.is_recurring,
            task.created_at.naive_utc(),
            metadata as Json<TaskMetadata>,
        )
        .execute(executor)
        .await
        .map_err(|e| {
            let error_code = e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()));
            match error_code {
                // PG `unique_violation` error
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: e,
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

    #[tracing::instrument(level = "debug", skip(self))]
    async fn update_stale_tasks_status_from_source_ids<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        active_source_task_ids: Vec<String>,
        kind: TaskSourceKind,
        status: TaskStatus,
    ) -> Result<Vec<Task>, UniversalInboxError> {
        let completed_at = if status == TaskStatus::Done {
            Some(Utc::now().naive_utc())
        } else {
            None
        };

        let rows = sqlx::query_as!(
            TaskRow,
            r#"
                UPDATE
                  task
                SET
                  status = $1::task_status,
                  completed_at = $2
                WHERE
                  NOT source_id = ANY($3)
                  AND kind::TEXT = $4
                  AND (status = 'Active')
                RETURNING
                  id,
                  source_id,
                  title,
                  body,
                  status as "status: _",
                  completed_at,
                  priority,
                  due_at as "due_at: Json<Option<DueDate>>",
                  source_html_url,
                  tags,
                  parent_id,
                  project,
                  is_recurring,
                  created_at,
                  metadata as "metadata: Json<TaskMetadata>"
            "#,
            status.to_string() as _,
            completed_at,
            &active_source_task_ids[..],
            kind.to_string(),
        )
        .fetch_all(executor)
        .await
        .context("Failed to update stale task status from storage")?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn create_or_update_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task: Box<Task>,
    ) -> Result<Task, UniversalInboxError> {
        let metadata = Json(task.metadata.clone());
        let priority: u8 = task.priority.into();

        let id: TaskId = TaskId(
            sqlx::query_scalar!(
                r#"
                INSERT INTO task
                  (
                    id,
                    source_id,
                    title,
                    body,
                    status,
                    completed_at,
                    priority,
                    due_at,
                    source_html_url,
                    tags,
                    parent_id,
                    project,
                    is_recurring,
                    created_at,
                    metadata
                  )
                VALUES
                  ($1, $2, $3, $4, $5::task_status, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                ON CONFLICT (source_id, kind) DO UPDATE
                SET
                  title = $3,
                  body = $4,
                  status = $5::task_status,
                  completed_at = $6,
                  priority = $7,
                  due_at = $8,
                  source_html_url = $9,
                  tags = $10,
                  parent_id = $11,
                  project = $12,
                  is_recurring = $13,
                  created_at = $14,
                  metadata = $15
                RETURNING
                  id
            "#,
                task.id.0,
                task.source_id,
                task.title,
                task.body,
                task.status.to_string() as _,
                task.completed_at
                    .map(|last_read_at| last_read_at.naive_utc()),
                priority as i32,
                Json(task.due_at.clone()) as Json<Option<DueDate>>,
                task.source_html_url.as_ref().map(|url| url.to_string()),
                &task.tags,
                task.parent_id.map(|id| id.0),
                task.project,
                task.is_recurring,
                task.created_at.naive_utc(),
                metadata as Json<TaskMetadata>,
            )
            .fetch_one(executor)
            .await
            .with_context(|| {
                format!(
                    "Failed to update task with source ID {} from storage",
                    task.source_id
                )
            })?,
        );

        Ok(Task { id, ..*task })
    }

    #[tracing::instrument(level = "debug", skip(self), ret)]
    async fn update_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        patch: &'b TaskPatch,
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

        query_builder.push(" WHERE id = ").push_bind(task_id.0);
        query_builder.push(
            r#"
                RETURNING
                  id,
                  source_id,
                  title,
                  body,
                  status,
                  completed_at,
                  priority,
                  due_at,
                  source_html_url,
                  tags,
                  parent_id,
                  project,
                  is_recurring,
                  created_at,
                  metadata,
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

        query_builder
            .push(" FROM task WHERE id = ")
            .push_bind(task_id.0);
        query_builder.push(r#") as "is_updated""#);

        let row: Option<UpdatedTaskRow> = query_builder
            .build_query_as::<UpdatedTaskRow>()
            .fetch_optional(executor)
            .await
            .context(format!("Failed to update task {} from storage", task_id))?;

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

#[derive(Debug, sqlx::FromRow)]
pub struct TaskRow {
    id: Uuid,
    source_id: String,
    title: String,
    body: String,
    status: PgTaskStatus,
    completed_at: Option<NaiveDateTime>,
    priority: i32,
    due_at: Json<Option<DueDate>>,
    source_html_url: Option<String>,
    tags: Vec<String>,
    parent_id: Option<Uuid>,
    project: String,
    is_recurring: bool,
    created_at: NaiveDateTime,
    metadata: Json<TaskMetadata>,
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
        let priority = TaskPriority::try_from(row.priority as u8)
            .with_context(|| format!("Failed to parse {} as TaskPriority", row.priority))?;
        let source_html_url = row
            .source_html_url
            .as_ref()
            .map(|url| {
                url.parse::<Uri>()
                    .map_err(|e| UniversalInboxError::InvalidUriData {
                        source: e,
                        output: url.clone(),
                    })
            })
            .transpose()?;

        Ok(Task {
            id: row.id.into(),
            source_id: row.source_id.to_string(),
            title: row.title.to_string(),
            body: row.body.to_string(),
            status,
            completed_at: row
                .completed_at
                .map(|completed_at| DateTime::<Utc>::from_utc(completed_at, Utc)),
            priority,
            due_at: row.due_at.0.clone(),
            source_html_url,
            tags: row.tags.clone(),
            parent_id: row.parent_id.map(|id| id.into()),
            project: row.project.to_string(),
            is_recurring: row.is_recurring,
            created_at: DateTime::<Utc>::from_utc(row.created_at, Utc),
            metadata: row.metadata.0.clone(),
        })
    }
}