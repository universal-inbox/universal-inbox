use anyhow::{Context as _, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Postgres, Transaction};
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    context::{
        AttachableKind, Attachment, AttachmentId, AttachmentKind, Context, ContextBinding,
        ContextError, ContextId, ContextStatus, ContextStatusChange, NOTE_BACKEND_LOCAL, Note,
        NoteBackend, NoteBackendBinding, NoteId,
    },
    integration_connection::provider::IntegrationProviderKind,
    task::ProjectId,
    user::UserId,
};

use crate::observability::attr;
use crate::universal_inbox::{UniversalInboxError, UpsertStatus};
use crate::utils::crypto::{aad, data_keyring};

use super::Repository;

const NOTE_BODY_AAD: &str = "note.body";

#[async_trait]
pub trait ContextRepository {
    /// The only way to insert a context or a note: both rows are inserted in the same
    /// transaction (`context.note_id` is a deferred foreign key, so a commit without the note
    /// fails). The note takes the id `context.note_id` and the timestamps of the context.
    async fn create_context_with_note(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context: Box<Context>,
        note_body: String,
    ) -> Result<(Box<Context>, Box<Note>), UniversalInboxError>;
    async fn get_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ContextId,
    ) -> Result<Option<Context>, UniversalInboxError>;
    /// Create or rename the context mirroring the upstream project `source_id`, found on its
    /// binding `(user_id, provider, source_id)`, never on its name.
    ///
    /// - absent: created, mirrored, flat and `Active`, with an empty note (through
    ///   `create_context_with_note`);
    /// - mirrored and the name differs: renamed, upstream owns the name;
    /// - mirrored and the name matches: `Untouched`, nothing is written;
    /// - not mirrored: `Untouched`, the name belongs to Universal Inbox.
    ///
    /// The status, parent and note are never changed.
    async fn upsert_mirrored_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        provider: IntegrationProviderKind,
        source_id: ProjectId,
        name: String,
    ) -> Result<UpsertStatus<Box<Context>>, UniversalInboxError>;
    async fn list_contexts_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<Context>, UniversalInboxError>;
    async fn get_note_for_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context_id: ContextId,
    ) -> Result<Option<Note>, UniversalInboxError>;
    /// The only way to change the status of a context, through [`ContextStatus::apply`]
    async fn update_context_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ContextId,
        change: ContextStatusChange,
    ) -> Result<Context, UniversalInboxError>;
    async fn create_attachment(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        attachment: Box<Attachment>,
    ) -> Result<Box<Attachment>, UniversalInboxError>;
    async fn list_attachments(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        attached_to: (AttachableKind, Uuid),
    ) -> Result<Vec<Attachment>, UniversalInboxError>;
}

#[async_trait]
impl ContextRepository for Repository {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::CONTEXT_ID } = context.id.to_string())
    )]
    async fn create_context_with_note(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context: Box<Context>,
        note_body: String,
    ) -> Result<(Box<Context>, Box<Note>), UniversalInboxError> {
        let note = Box::new(Note {
            id: context.note_id,
            context_id: context.id,
            body: note_body,
            backend: NoteBackend::Local,
            created_at: context.created_at,
            updated_at: context.updated_at,
        });
        let body_enc = encrypt_note_body(&note.body, note.id)?;

        if let Some(parent_id) = context.parent_id
            && !context_belongs_to_user(executor, parent_id.0, context.user_id).await?
        {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Parent context {parent_id} not found"),
            });
        }

        sqlx::query!(
            r#"
                INSERT INTO context
                  (
                    id,
                    user_id,
                    parent_id,
                    name,
                    binding_provider,
                    binding_source_id,
                    binding_mirrored,
                    status,
                    note_id,
                    created_at,
                    updated_at
                  )
                VALUES
                  ($1, $2, $3, $4, $5::integration_provider_kind, $6, $7, $8::context_status, $9, $10, $11)
            "#,
            context.id.0,
            context.user_id.0,
            context.parent_id.map(|id| id.0),
            context.name,
            context
                .binding
                .as_ref()
                .map(|binding| binding.provider.to_string()) as Option<String>,
            context
                .binding
                .as_ref()
                .map(|binding| binding.source_id.0.clone()),
            context.binding.as_ref().map(|binding| binding.mirrored),
            context.status.to_string() as _,
            context.note_id.0,
            context.created_at.naive_utc(),
            context.updated_at.naive_utc(),
        )
        .execute(&mut **executor)
        .await
        .map_err(|e| map_write_error(e, context.id.0, "context"))?;

        let (backend_kind, backend_binding) = note_backend_columns(&note.backend);
        sqlx::query!(
            r#"
                INSERT INTO note
                  (id, context_id, body_enc, backend_kind, backend_binding, created_at, updated_at)
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7)
            "#,
            note.id.0,
            note.context_id.0,
            body_enc,
            backend_kind,
            backend_binding as Option<serde_json::Value>,
            note.created_at.naive_utc(),
            note.updated_at.naive_utc(),
        )
        .execute(&mut **executor)
        .await
        .map_err(|e| map_write_error(e, note.id.0, "note"))?;

        Ok((context, note))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::CONTEXT_ID } = id.to_string())
    )]
    async fn get_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ContextId,
    ) -> Result<Option<Context>, UniversalInboxError> {
        let row = sqlx::query_as!(
            ContextRow,
            r#"
                SELECT
                  id,
                  user_id,
                  parent_id,
                  name,
                  binding_provider::TEXT as "binding_provider?",
                  binding_source_id,
                  binding_mirrored,
                  status::TEXT as "status!",
                  note_id,
                  created_at,
                  updated_at
                FROM context
                WHERE id = $1
            "#,
            id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .context("Failed to fetch context from storage")?;

        row.map(Context::try_from).transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn upsert_mirrored_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        provider: IntegrationProviderKind,
        source_id: ProjectId,
        name: String,
    ) -> Result<UpsertStatus<Box<Context>>, UniversalInboxError> {
        let binding = ContextBinding::new(provider, source_id.clone(), true).map_err(|err| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: err.to_string(),
            }
        })?;

        let existing = sqlx::query_as!(
            ContextRow,
            r#"
                SELECT
                  id,
                  user_id,
                  parent_id,
                  name,
                  binding_provider::TEXT as "binding_provider?",
                  binding_source_id,
                  binding_mirrored,
                  status::TEXT as "status!",
                  note_id,
                  created_at,
                  updated_at
                FROM context
                WHERE user_id = $1
                  AND binding_provider = $2::integration_provider_kind
                  AND binding_source_id = $3
                FOR UPDATE
            "#,
            user_id.0,
            provider.to_string() as _,
            source_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .context("Failed to fetch context by binding from storage")?
        .map(Context::try_from)
        .transpose()?;

        let Some(existing) = existing else {
            let now = Utc::now();
            let context = Box::new(Context {
                id: ContextId(Uuid::new_v4()),
                user_id,
                parent_id: None,
                name,
                binding: Some(binding),
                status: ContextStatus::Active,
                note_id: NoteId(Uuid::new_v4()),
                created_at: now,
                updated_at: now,
            });
            let (context, _) = self
                .create_context_with_note(executor, context, String::new())
                .await?;
            return Ok(UpsertStatus::Created(context));
        };

        let owned_by_upstream = existing
            .binding
            .as_ref()
            .is_some_and(|binding| binding.mirrored);
        if !owned_by_upstream || existing.name == name {
            return Ok(UpsertStatus::Untouched(Box::new(existing)));
        }

        sqlx::query!(
            r#"UPDATE context SET name = $1, updated_at = $2 WHERE id = $3"#,
            name,
            Utc::now().naive_utc(),
            existing.id.0
        )
        .execute(&mut **executor)
        .await
        .context("Failed to rename context in storage")?;

        let updated = self
            .get_context(executor, existing.id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::ItemNotFound(format!("Context {} not found", existing.id))
            })?;
        Ok(UpsertStatus::Updated {
            old: Box::new(existing),
            new: Box::new(updated),
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn list_contexts_for_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<Context>, UniversalInboxError> {
        let rows = sqlx::query_as!(
            ContextRow,
            r#"
                SELECT
                  id,
                  user_id,
                  parent_id,
                  name,
                  binding_provider::TEXT as "binding_provider?",
                  binding_source_id,
                  binding_mirrored,
                  status::TEXT as "status!",
                  note_id,
                  created_at,
                  updated_at
                FROM context
                WHERE user_id = $1
                ORDER BY created_at ASC, id ASC
            "#,
            user_id.0
        )
        .fetch_all(&mut **executor)
        .await
        .context("Failed to fetch contexts from storage")?;

        rows.into_iter().map(Context::try_from).collect()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::CONTEXT_ID } = context_id.to_string())
    )]
    async fn get_note_for_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context_id: ContextId,
    ) -> Result<Option<Note>, UniversalInboxError> {
        let row = sqlx::query!(
            r#"
                SELECT id, context_id, body_enc, backend_kind, backend_binding, created_at, updated_at
                FROM note
                WHERE context_id = $1
            "#,
            context_id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .context("Failed to fetch note from storage")?;

        row.map(|row| {
            let backend = if row.backend_kind == NOTE_BACKEND_LOCAL {
                NoteBackend::Local
            } else {
                NoteBackend::External(
                    NoteBackendBinding::new(
                        row.backend_kind,
                        row.backend_binding.unwrap_or(serde_json::Value::Null),
                    )
                    .map_err(|err| anyhow!("Invalid note backend in storage: {err}"))?,
                )
            };
            Ok(Note {
                id: row.id.into(),
                context_id: row.context_id.into(),
                body: decrypt_note_body(&row.body_enc, row.id)?,
                backend,
                created_at: row.created_at.and_utc(),
                updated_at: row.updated_at.and_utc(),
            })
        })
        .transpose()
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::CONTEXT_ID } = id.to_string(),
            { attr::CONTEXT_STATUS_CHANGE } = change.to_string()
        )
    )]
    async fn update_context_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: ContextId,
        change: ContextStatusChange,
    ) -> Result<Context, UniversalInboxError> {
        let current = sqlx::query_scalar!(
            r#"SELECT status::TEXT as "status!" FROM context WHERE id = $1 FOR UPDATE"#,
            id.0
        )
        .fetch_optional(&mut **executor)
        .await
        .context("Failed to fetch context status from storage")?
        .ok_or_else(|| UniversalInboxError::ItemNotFound(format!("Context {id} not found")))?;
        let current = parse_status(&current)?;

        let new_status = current.apply(change).map_err(|err: ContextError| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: err.to_string(),
            }
        })?;

        sqlx::query!(
            r#"UPDATE context SET status = $1::context_status, updated_at = $2 WHERE id = $3"#,
            new_status.to_string() as _,
            Utc::now().naive_utc(),
            id.0
        )
        .execute(&mut **executor)
        .await
        .context("Failed to update context status in storage")?;

        self.get_context(executor, id)
            .await?
            .ok_or_else(|| UniversalInboxError::ItemNotFound(format!("Context {id} not found")))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_attachment(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        attachment: Box<Attachment>,
    ) -> Result<Box<Attachment>, UniversalInboxError> {
        let (attached_kind, attached_id) = attachment.attached_to;
        let (context_id, task_id) = match attached_kind {
            AttachableKind::Context => (Some(attached_id), None),
            AttachableKind::Task => (None, Some(attached_id)),
        };
        let target_belongs_to_user = match attached_kind {
            AttachableKind::Context => {
                context_belongs_to_user(executor, attached_id, attachment.user_id).await?
            }
            AttachableKind::Task => {
                task_belongs_to_user(executor, attached_id, attachment.user_id).await?
            }
        };
        if !target_belongs_to_user {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("{attached_kind} {attached_id} not found"),
            });
        }

        sqlx::query!(
            r#"
                INSERT INTO attachment
                  (id, user_id, context_id, task_id, kind, url, title, provider, position, created_at, updated_at)
                VALUES
                  ($1, $2, $3, $4, $5::attachment_kind, $6, $7, $8::integration_provider_kind, $9, $10, $11)
            "#,
            attachment.id.0,
            attachment.user_id.0,
            context_id,
            task_id,
            attachment.kind.to_string() as _,
            attachment.url.as_str(),
            attachment.title,
            attachment.provider.map(|provider| provider.to_string()) as Option<String>,
            attachment.position,
            attachment.created_at.naive_utc(),
            attachment.updated_at.naive_utc(),
        )
        .execute(&mut **executor)
        .await
        .map_err(|e| map_write_error(e, attachment.id.0, "attachment"))?;

        Ok(attachment)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_attachments(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        attached_to: (AttachableKind, Uuid),
    ) -> Result<Vec<Attachment>, UniversalInboxError> {
        let (attached_kind, attached_id) = attached_to;
        let rows = match attached_kind {
            AttachableKind::Context => {
                sqlx::query_as!(
                    AttachmentRow,
                    r#"
                        SELECT
                          id,
                          user_id,
                          context_id,
                          task_id,
                          kind::TEXT as "kind!",
                          url,
                          title,
                          provider::TEXT as "provider?",
                          position,
                          created_at,
                          updated_at
                        FROM attachment
                        WHERE context_id = $1
                        ORDER BY position ASC, id ASC
                    "#,
                    attached_id
                )
                .fetch_all(&mut **executor)
                .await
            }
            AttachableKind::Task => {
                sqlx::query_as!(
                    AttachmentRow,
                    r#"
                        SELECT
                          id,
                          user_id,
                          context_id,
                          task_id,
                          kind::TEXT as "kind!",
                          url,
                          title,
                          provider::TEXT as "provider?",
                          position,
                          created_at,
                          updated_at
                        FROM attachment
                        WHERE task_id = $1
                        ORDER BY position ASC, id ASC
                    "#,
                    attached_id
                )
                .fetch_all(&mut **executor)
                .await
            }
        }
        .context("Failed to fetch attachments from storage")?;

        rows.into_iter().map(Attachment::try_from).collect()
    }
}

/// The database does not tie the `user_id` of a row to the one of the context or task it points
/// at: the repository does, for the rows it inserts.
async fn context_belongs_to_user(
    executor: &mut Transaction<'_, Postgres>,
    context_id: Uuid,
    user_id: UserId,
) -> Result<bool, UniversalInboxError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM context WHERE id = $1 AND user_id = $2) as "exists!""#,
        context_id,
        user_id.0
    )
    .fetch_one(&mut **executor)
    .await
    .context("Failed to check the owner of the context")?)
}

async fn task_belongs_to_user(
    executor: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    user_id: UserId,
) -> Result<bool, UniversalInboxError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM task WHERE id = $1 AND user_id = $2) as "exists!""#,
        task_id,
        user_id.0
    )
    .fetch_one(&mut **executor)
    .await
    .context("Failed to check the owner of the task")?)
}

fn encrypt_note_body(body: &str, id: NoteId) -> Result<Vec<u8>, UniversalInboxError> {
    data_keyring()?.encrypt(body.as_bytes(), &aad(NOTE_BODY_AAD, id.0))
}

/// Decrypt `note.body_enc` of the note `id`
fn decrypt_note_body(body_enc: &[u8], id: Uuid) -> Result<String, UniversalInboxError> {
    data_keyring()?.decrypt_string(body_enc, &aad(NOTE_BODY_AAD, id))
}

/// `note.backend_kind` and `note.backend_binding` of a backend
fn note_backend_columns(backend: &NoteBackend) -> (String, Option<serde_json::Value>) {
    match backend {
        NoteBackend::Local => (NOTE_BACKEND_LOCAL.to_string(), None),
        NoteBackend::External(binding) => {
            (binding.kind().to_string(), Some(binding.binding().clone()))
        }
    }
}

fn map_write_error(e: sqlx::Error, id: Uuid, entity: &str) -> UniversalInboxError {
    let error_code = e
        .as_database_error()
        .and_then(|db_error| db_error.code().map(|code| code.to_string()));
    match error_code {
        // PG `unique_violation` error
        Some(x) if x == *"23505" => UniversalInboxError::AlreadyExists {
            source: Some(e),
            id,
        },
        // PG `check_violation` error
        Some(x) if x == *"23514" => UniversalInboxError::InvalidInputData {
            source: Some(e),
            user_error: format!("Submitted {entity} is invalid"),
        },
        _ => UniversalInboxError::Unexpected(anyhow!(
            "Failed to insert new {entity} into storage: {e}"
        )),
    }
}

fn parse_status(status: &str) -> Result<ContextStatus, UniversalInboxError> {
    status
        .parse::<ContextStatus>()
        .map_err(|err| UniversalInboxError::InvalidEnumData {
            source: err,
            output: status.to_string(),
        })
}

fn parse_provider(provider: &str) -> Result<IntegrationProviderKind, UniversalInboxError> {
    provider.parse::<IntegrationProviderKind>().map_err(|err| {
        UniversalInboxError::InvalidEnumData {
            source: err,
            output: provider.to_string(),
        }
    })
}

struct ContextRow {
    id: Uuid,
    user_id: Uuid,
    parent_id: Option<Uuid>,
    name: String,
    binding_provider: Option<String>,
    binding_source_id: Option<String>,
    binding_mirrored: Option<bool>,
    status: String,
    note_id: Uuid,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl TryFrom<ContextRow> for Context {
    type Error = UniversalInboxError;

    fn try_from(row: ContextRow) -> Result<Self, Self::Error> {
        // The binding columns are all set or all NULL (`context_binding_all_or_none`)
        let binding = match (
            row.binding_provider,
            row.binding_source_id,
            row.binding_mirrored,
        ) {
            (Some(provider), Some(source_id), Some(mirrored)) => Some(
                ContextBinding::new(parse_provider(&provider)?, source_id.into(), mirrored)
                    .map_err(|err| anyhow!("Invalid context binding in storage: {err}"))?,
            ),
            (None, None, None) => None,
            _ => {
                return Err(UniversalInboxError::Unexpected(anyhow!(
                    "Partial context binding in storage for context {}",
                    row.id
                )));
            }
        };

        Ok(Context {
            id: row.id.into(),
            user_id: row.user_id.into(),
            parent_id: row.parent_id.map(Into::into),
            name: row.name,
            binding,
            status: parse_status(&row.status)?,
            note_id: row.note_id.into(),
            created_at: DateTime::<Utc>::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::<Utc>::from_naive_utc_and_offset(row.updated_at, Utc),
        })
    }
}

struct AttachmentRow {
    id: Uuid,
    user_id: Uuid,
    context_id: Option<Uuid>,
    task_id: Option<Uuid>,
    kind: String,
    url: String,
    title: Option<String>,
    provider: Option<String>,
    position: i32,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl TryFrom<AttachmentRow> for Attachment {
    type Error = UniversalInboxError;

    fn try_from(row: AttachmentRow) -> Result<Self, Self::Error> {
        // Exactly one target is set (`attachment_has_one_target`)
        let attached_to = match (row.context_id, row.task_id) {
            (Some(context_id), None) => (AttachableKind::Context, context_id),
            (None, Some(task_id)) => (AttachableKind::Task, task_id),
            _ => {
                return Err(UniversalInboxError::Unexpected(anyhow!(
                    "Attachment {} has no or two targets in storage",
                    row.id
                )));
            }
        };
        let kind = row.kind.parse::<AttachmentKind>().map_err(|err| {
            UniversalInboxError::InvalidEnumData {
                source: err,
                output: row.kind.clone(),
            }
        })?;
        let url = Url::parse(&row.url).map_err(|err| UniversalInboxError::InvalidUrlData {
            source: err,
            output: row.url.clone(),
        })?;

        Ok(Attachment {
            id: AttachmentId(row.id),
            user_id: row.user_id.into(),
            attached_to,
            kind,
            url,
            title: row.title,
            provider: row.provider.as_deref().map(parse_provider).transpose()?,
            position: row.position,
            created_at: DateTime::<Utc>::from_naive_utc_and_offset(row.created_at, Utc),
            updated_at: DateTime::<Utc>::from_naive_utc_and_offset(row.updated_at, Utc),
        })
    }
}
