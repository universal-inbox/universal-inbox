//! Writes to the context tree under its ownership rules (see [`universal_inbox::context`]): the
//! provider owns the name and the parent of a mirrored context, Universal Inbox those of every
//! other one. A context created under a mirrored parent is created in the provider first, then
//! bound to the project the provider returns.

use anyhow::{Context as _, anyhow};
use chrono::Utc;
use sqlx::{Acquire, Postgres, Transaction};
use tracing::error;
use uuid::Uuid;

use universal_inbox::{
    context::{
        Context, ContextBinding, ContextCreation, ContextId, ContextStatus, ContextWriteError,
        NoteId, UpstreamProvider, check_context_rename, check_context_reparent,
        plan_context_creation, upstream_creation_failed_message,
    },
    integration_connection::provider::IntegrationProviderKind,
    task::ProjectId,
    user::UserId,
};

use crate::{
    integrations::oauth2::AccessToken,
    observability::attr,
    repository::context::ContextRepository,
    universal_inbox::{UniversalInboxError, task::service::TaskService},
};

/// A project just created in the provider for a new context
struct UpstreamProject {
    provider: UpstreamProvider,
    source_id: ProjectId,
    access_token: AccessToken,
}

impl TaskService {
    /// Create the context `name` under `parent_id` (`None`: at the root) with an empty note.
    ///
    /// Under a mirrored parent, the project is created in the provider first and the context is
    /// bound to it: a provider failure stores nothing, and a failure to insert the context deletes
    /// the project again. When the sync has already mirrored the new project, that context is
    /// returned instead of a second one. If the caller's commit fails afterwards, the project
    /// stays in the provider and the next sync mirrors it.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn create_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        name: &str,
        parent_id: Option<ContextId>,
    ) -> Result<Context, UniversalInboxError> {
        let name = context_name(name)?;
        let parent = match parent_id {
            Some(parent_id) => Some(
                self.repository
                    .get_context(executor, parent_id)
                    .await?
                    .filter(|parent| parent.user_id == user_id)
                    .ok_or_else(|| {
                        UniversalInboxError::ItemNotFound(format!("Context {parent_id} not found"))
                    })?,
            ),
            None => None,
        };

        let upstream = match plan_context_creation(parent.as_ref()).map_err(ownership_error)? {
            ContextCreation::Local => None,
            ContextCreation::Upstream {
                provider,
                parent_source_id,
            } => Some(
                self.create_upstream_project(executor, user_id, provider, &name, &parent_source_id)
                    .await
                    .map_err(|source| UniversalInboxError::UpstreamWriteFailed {
                        message: upstream_creation_failed_message(&name, provider),
                        source: Box::new(source),
                    })?,
            ),
        };

        let binding = upstream
            .as_ref()
            .map(|upstream| {
                ContextBinding::new(upstream.provider.into(), upstream.source_id.clone(), true)
            })
            .transpose()
            .map_err(|err| anyhow!("Cannot bind the new context: {err}"))?;
        let now = Utc::now();
        let context = Box::new(Context {
            id: ContextId(Uuid::new_v4()),
            user_id,
            parent_id,
            name,
            binding,
            status: ContextStatus::Active,
            is_inbox: false,
            note_id: NoteId(Uuid::new_v4()),
            created_at: now,
            updated_at: now,
        });

        match (self.insert_in_savepoint(executor, context).await, upstream) {
            (stored, None) | (stored @ Ok(_), Some(_)) => stored,
            // The sync mirrored the new project first: its context is the one
            (Err(UniversalInboxError::AlreadyExists { .. }), Some(upstream)) => {
                self.get_mirrored_context(executor, user_id, &upstream)
                    .await
            }
            (Err(err), Some(upstream)) => {
                self.delete_upstream_project(user_id, &upstream).await;
                Err(err)
            }
        }
    }

    /// Rename the context `id` to `name` and move it under `parent_id` (`Some(None)`: to the
    /// root). An absent field, or a field set to its current value, is left as is. Only a
    /// context Universal Inbox owns is written, never under a parent the provider owns the
    /// children of.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_ID } = user_id.to_string(),
            { attr::CONTEXT_ID } = id.to_string()
        )
    )]
    pub async fn update_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        id: ContextId,
        name: Option<&str>,
        parent_id: Option<Option<ContextId>>,
    ) -> Result<Context, UniversalInboxError> {
        self.repository.lock_context_tree(executor, user_id).await?;
        let contexts = self
            .repository
            .list_contexts_for_user(executor, user_id)
            .await?;
        let find = |id: ContextId| {
            contexts
                .iter()
                .find(|context| context.id == id)
                .ok_or_else(|| UniversalInboxError::ItemNotFound(format!("Context {id} not found")))
        };
        let context = find(id)?;

        let name = name
            .map(context_name)
            .transpose()?
            .unwrap_or_else(|| context.name.clone());
        let parent_id = parent_id.unwrap_or(context.parent_id);
        let parent = parent_id.map(find).transpose()?;
        check_context_rename(context, &name).map_err(ownership_error)?;
        check_context_reparent(context, parent, &contexts).map_err(ownership_error)?;
        if name == context.name && parent_id == context.parent_id {
            return Ok(context.clone());
        }

        self.repository
            .update_owned_context(executor, id, &name, parent_id)
            .await
    }

    /// Store `context` with an empty note in a savepoint, so that a failed insert leaves the
    /// caller's transaction usable
    async fn insert_in_savepoint(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        context: Box<Context>,
    ) -> Result<Context, UniversalInboxError> {
        let mut savepoint = executor
            .begin()
            .await
            .context("Failed to open a savepoint while creating a context")?;
        match self
            .repository
            .create_context_with_note(&mut savepoint, context, String::new())
            .await
        {
            Ok((context, _)) => {
                savepoint
                    .commit()
                    .await
                    .context("Failed to release the savepoint while creating a context")?;
                Ok(*context)
            }
            Err(err) => {
                savepoint
                    .rollback()
                    .await
                    .context("Failed to roll back the savepoint while creating a context")?;
                Err(err)
            }
        }
    }

    async fn create_upstream_project(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        provider: UpstreamProvider,
        name: &str,
        parent_source_id: &ProjectId,
    ) -> Result<UpstreamProject, UniversalInboxError> {
        let (source_id, access_token) = match provider {
            UpstreamProvider::Todoist => {
                let access_token = self
                    .todoist_service
                    .find_access_token(executor, user_id)
                    .await?;
                let source_id = self
                    .todoist_service
                    .create_project(&access_token, name, parent_source_id)
                    .await?;
                (source_id, access_token)
            }
        };
        Ok(UpstreamProject {
            provider,
            source_id,
            access_token,
        })
    }

    /// Undo [`TaskService::create_upstream_project`]. Best effort: a failure is logged, the next
    /// sync then mirrors the project.
    async fn delete_upstream_project(&self, user_id: UserId, upstream: &UpstreamProject) {
        let deleted = match upstream.provider {
            UpstreamProvider::Todoist => {
                self.todoist_service
                    .delete_project(&upstream.access_token, &upstream.source_id)
                    .await
            }
        };
        if let Err(err) = deleted {
            error!(
                { attr::USER_ID } = user_id.to_string(),
                "Failed to delete the {} project {} created for a context that could not be \
                 stored: {err:?}",
                IntegrationProviderKind::from(upstream.provider),
                upstream.source_id
            );
        }
    }

    async fn get_mirrored_context(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        upstream: &UpstreamProject,
    ) -> Result<Context, UniversalInboxError> {
        let provider = IntegrationProviderKind::from(upstream.provider);
        let id = self
            .repository
            .get_mirrored_context_id(executor, user_id, provider, &upstream.source_id)
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "No context mirrors the {provider} project {}",
                    upstream.source_id
                )
            })?;
        self.repository
            .get_context(executor, id)
            .await?
            .ok_or_else(|| UniversalInboxError::ItemNotFound(format!("Context {id} not found")))
    }
}

fn context_name(name: &str) -> Result<String, UniversalInboxError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "A project needs a name".to_string(),
        });
    }
    Ok(name.to_string())
}

/// The message of `err` is shown to the user as is. A parent cycle is a request that can never
/// succeed (400); every other refusal is owed to the state of the tree (409).
fn ownership_error(err: ContextWriteError) -> UniversalInboxError {
    match err {
        ContextWriteError::ParentIsSelf { .. } | ContextWriteError::ParentIsDescendant { .. } => {
            UniversalInboxError::InvalidRequest(err.to_string())
        }
        _ => UniversalInboxError::Conflict(err.to_string()),
    }
}
