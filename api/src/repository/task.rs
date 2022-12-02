use std::sync::Arc;

use anyhow::{anyhow, Context};
use chrono::{DateTime, NaiveDateTime, Utc};
use duplicate::duplicate_item;
use http::Uri;
use sqlx::{pool::PoolConnection, types::Json, PgPool, Postgres, QueryBuilder, Transaction};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::universal_inbox::{UniversalInboxError, UpdateStatus};
use universal_inbox::task::{Task, TaskMetadata, TaskPatch, TaskPriority, TaskStatus};

pub struct TaskRepository {
    pub pool: Arc<PgPool>,
}

impl TaskRepository {
    pub fn new(pool: Arc<PgPool>) -> TaskRepository {
        TaskRepository { pool }
    }

    pub async fn connect(&self) -> Result<Arc<ConnectedTaskRepository>, UniversalInboxError> {
        let connection = self
            .pool
            .acquire()
            .await
            .context("Failed to connection to the database")?;
        Ok(Arc::new(ConnectedTaskRepository {
            executor: Arc::new(Mutex::new(connection)),
        }))
    }

    pub async fn begin(&self) -> Result<Arc<TransactionalTaskRepository>, UniversalInboxError> {
        let transaction = self
            .pool
            .begin()
            .await
            .context("Failed to begin database transaction")?;
        Ok(Arc::new(TransactionalTaskRepository {
            executor: Arc::new(Mutex::new(transaction)),
        }))
    }
}

pub struct ConnectedTaskRepository {
    pub executor: Arc<Mutex<PoolConnection<Postgres>>>,
}

pub struct TransactionalTaskRepository<'a> {
    pub executor: Arc<Mutex<Transaction<'a, Postgres>>>,
}

impl<'a> TransactionalTaskRepository<'a> {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn commit(self) -> Result<(), UniversalInboxError> {
        let transaction = Arc::try_unwrap(self.executor)
            .map_err(|_| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Cannot extract transaction to commit it as it has other references using it"
                ))
            })?
            .into_inner();
        Ok(transaction
            .commit()
            .await
            .context("Failed to commit database transaction")?)
    }
}

#[duplicate_item(repository; [ConnectedTaskRepository]; [TransactionalTaskRepository<'a>])]
impl<'a> repository {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_one(&self, id: Uuid) -> Result<Option<Task>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
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
                  due_at,
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
            id
        )
        .fetch_optional(&mut *executor)
        .await
        .with_context(|| format!("Failed to fetch task {} from storage", id))?;

        row.map(|task_row| task_row.try_into()).transpose()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_all(&self, status: TaskStatus) -> Result<Vec<Task>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;

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

        let records = query_builder
            .build_query_as::<TaskRow>()
            .fetch_all(&mut *executor)
            .await
            .context("Failed to fetch tasks from storage")?;

        records.iter().map(|r| r.try_into()).collect()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn create(&self, task: Box<Task>) -> Result<Box<Task>, UniversalInboxError> {
        let mut executor = self.executor.lock().await;
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
            task.id,
            task.source_id,
            task.title,
            task.body,
            task.status.to_string() as _,
            task.completed_at
                .map(|last_read_at| last_read_at.naive_utc()),
            priority as i32,
            task.due_at.map(|last_read_at| last_read_at.naive_utc()),
            task.source_html_url.as_ref().map(|url| url.to_string()),
            &task.tags,
            task.parent_id,
            task.project,
            task.is_recurring,
            task.created_at.naive_utc(),
            metadata as Json<TaskMetadata>,
        )
        .execute(&mut *executor)
        .await
        .map_err(|e| {
            let error_code = e
                .as_database_error()
                .and_then(|db_error| db_error.code().map(|code| code.to_string()));
            match error_code {
                // PG `unique_violation` error
                Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
                    source: e,
                    id: task.id,
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
    pub async fn update<'b>(
        &self,
        task_id: Uuid,
        patch: &'b TaskPatch,
    ) -> Result<UpdateStatus<Box<Task>>, UniversalInboxError> {
        if *patch == Default::default() {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Missing `status` field value to update task {task_id}"),
            });
        };

        let mut executor = self.executor.lock().await;

        let mut query_builder = QueryBuilder::new("UPDATE task SET");
        let mut separated = query_builder.separated(", ");
        if let Some(status) = patch.status {
            separated
                .push(" status = ")
                .push_bind_unseparated(status.to_string())
                .push_unseparated("::task_status");
        }

        query_builder.push(" WHERE id = ").push_bind(task_id);
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

        let mut separated = query_builder.separated(" AND ");
        if let Some(status) = patch.status {
            separated
                .push(" status::TEXT != ")
                .push_bind_unseparated(status.to_string());
        }

        query_builder
            .push(" FROM task WHERE id = ")
            .push_bind(task_id);
        query_builder.push(r#") as "is_updated""#);

        let record: Option<UpdatedTaskRow> = query_builder
            .build_query_as::<UpdatedTaskRow>()
            .fetch_optional(&mut *executor)
            .await
            .context(format!("Failed to update task {} from storage", task_id))?;

        if let Some(updated_task_row) = record {
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
}

#[derive(Debug, sqlx::FromRow)]
struct TaskRow {
    id: Uuid,
    source_id: String,
    title: String,
    body: String,
    status: PgTaskStatus,
    completed_at: Option<NaiveDateTime>,
    priority: i32,
    due_at: Option<NaiveDateTime>,
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
            id: row.id,
            source_id: row.source_id.to_string(),
            title: row.title.to_string(),
            body: row.body.to_string(),
            status,
            completed_at: row
                .completed_at
                .map(|completed_at| DateTime::<Utc>::from_utc(completed_at, Utc)),
            priority,
            due_at: row
                .due_at
                .map(|due_at| DateTime::<Utc>::from_utc(due_at, Utc)),
            source_html_url,
            tags: row.tags.clone(),
            parent_id: row.parent_id,
            project: row.project.to_string(),
            is_recurring: row.is_recurring,
            created_at: DateTime::<Utc>::from_utc(row.created_at, Utc),
            metadata: row.metadata.0.clone(),
        })
    }
}
