use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{types::Json, Postgres, QueryBuilder, Transaction};
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    task::{
        service::TaskPatch, DueDate, Task, TaskId, TaskMetadata, TaskPriority, TaskSourceKind,
        TaskStatus, TaskSummary,
    },
    user::UserId,
};

use crate::universal_inbox::{UniversalInboxError, UpdateStatus};

use super::Repository;

#[async_trait]
pub trait TaskRepository {
    async fn get_one_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: TaskId,
    ) -> Result<Option<Task>, UniversalInboxError>;
    async fn does_task_exist<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: TaskId,
    ) -> Result<bool, UniversalInboxError>;
    async fn get_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        ids: Vec<TaskId>,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn fetch_all_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: TaskStatus,
        user_id: UserId,
    ) -> Result<Vec<Task>, UniversalInboxError>;
    async fn search_tasks<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        matches: &'b str,
        user_id: UserId,
    ) -> Result<Vec<TaskSummary>, UniversalInboxError>;
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
        for_user_id: UserId,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError>;
}

#[async_trait]
impl TaskRepository for Repository {
    #[tracing::instrument(level = "debug", skip(self, executor))]
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
                  metadata as "metadata: Json<TaskMetadata>",
                  user_id
                FROM task
                WHERE id = $1
            "#,
            id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .with_context(|| format!("Failed to fetch task {id} from storage"))?;

        row.map(|task_row| task_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn does_task_exist<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: TaskId,
    ) -> Result<bool, UniversalInboxError> {
        let count: Option<i64> =
            sqlx::query_scalar!(r#"SELECT count(*) FROM task WHERE id = $1"#, id.0)
                .fetch_one(&mut **executor)
                .await
                .with_context(|| format!("Failed to check if task {id} exists",))?;

        if let Some(1) = count {
            return Ok(true);
        }
        return Ok(false);
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
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
                  metadata as "metadata: Json<TaskMetadata>",
                  user_id
                FROM task
                WHERE id = any($1)
            "#,
            &uuids[..]
        )
        .fetch_all(&mut **executor)
        .await
        .with_context(|| format!("Failed to fetch tasks {uuids:?} from storage"))?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn fetch_all_tasks<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        status: TaskStatus,
        user_id: UserId,
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
                  metadata,
                  user_id
                FROM
                  task
                WHERE
                  status::TEXT =
            "#,
        );
        query_builder.push_bind(status.to_string());
        query_builder
            .push(" AND task.user_id = ")
            .push_bind(user_id.0);

        let rows = query_builder
            .build_query_as::<TaskRow>()
            .fetch_all(&mut **executor)
            .await
            .context("Failed to fetch tasks from storage")?;

        rows.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn search_tasks<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        matches: &'b str,
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
                  id,
                  source_id,
                  title,
                  body,
                  priority,
                  due_at as "due_at: Json<Option<DueDate>>",
                  tags,
                  project
                FROM
                  task,
                  to_tsquery('english', $1) query
                WHERE
                  query @@ title_body_project_tags_tsv
                  AND status::TEXT = 'Active'
                  AND user_id = $2
                ORDER BY ts_rank_cd(title_body_project_tags_tsv, query) DESC
                LIMIT 10;
            "#,
            ts_query,
            user_id.0
        )
        .fetch_all(&mut **executor)
        .await
        .context("Failed to search tasks from storage")?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<TaskSummary>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self, executor, task), fields(task_id = task.id.to_string()))]
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
                    metadata,
                    user_id
                  )
                VALUES
                  ($1, $2, $3, $4, $5::task_status, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
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
                  metadata as "metadata: Json<TaskMetadata>",
                  user_id
            "#,
            status.to_string() as _,
            completed_at,
            &active_source_task_ids[..],
            kind.to_string(),
        )
        .fetch_all(&mut **executor)
        .await
        .context("Failed to update stale task status from storage")?;

        rows.iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Task>, UniversalInboxError>>()
    }

    #[tracing::instrument(level = "debug", skip(self, executor, task), fields(task_id = task.id.to_string()))]
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
                    metadata,
                    user_id
                  )
                VALUES
                  ($1, $2, $3, $4, $5::task_status, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
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
                  metadata = $15,
                  user_id = $16
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
                task.user_id.0
            )
            .fetch_one(&mut **executor)
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

    #[tracing::instrument(level = "debug", skip(self, executor))]
    async fn update_task<'a, 'b>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        task_id: TaskId,
        patch: &'b TaskPatch,
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

        query_builder
            .push(" WHERE ")
            .separated(" AND ")
            .push(" id = ")
            .push_bind_unseparated(task_id.0)
            .push(" user_id = ")
            .push_bind_unseparated(for_user_id.0);

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
                  user_id,
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
            .context(format!("Failed to update task {task_id} from storage"))?;

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
    user_id: Uuid,
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
                url.parse::<Url>()
                    .map_err(|e| UniversalInboxError::InvalidUrlData {
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
                .map(|completed_at| DateTime::from_naive_utc_and_offset(completed_at, Utc)),
            priority,
            due_at: row.due_at.0.clone(),
            source_html_url,
            tags: row.tags.clone(),
            parent_id: row.parent_id.map(|id| id.into()),
            project: row.project.to_string(),
            is_recurring: row.is_recurring,
            created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
            metadata: row.metadata.0.clone(),
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
