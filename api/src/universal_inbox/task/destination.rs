//! Where a task goes, by id only. Every binding path (task creation, move, sink creation, stored
//! default projects) resolves its destination here: a context to the Todoist project it mirrors,
//! a stored default to a destination. A destination that does not resolve is an
//! [`UniversalInboxError::UnresolvedTaskDestination`]: nothing is ever created from a name.
//!
//! A stored default is always read through [`DefaultProject`], which knows what it means for its
//! task manager.

use sqlx::{Postgres, Transaction};

use universal_inbox::{
    context::{Context, ContextId, ContextStatus},
    integration_connection::{
        IntegrationConnection, config::IntegrationConnectionConfig,
        provider::IntegrationProviderKind,
    },
    task::{
        DefaultProject, DefaultProjectIssue, ProjectId, Task, TaskDestination, TaskProject,
        integrations::todoist::TODOIST_INBOX_PROJECT,
    },
    third_party::integrations::ticktick::TICKTICK_INBOX_PROJECT,
    user::UserId,
};

use crate::{
    repository::{
        Repository, context::ContextRepository,
        integration_connection::IntegrationConnectionRepository,
        user_preferences::UserPreferencesRepository,
    },
    universal_inbox::UniversalInboxError,
};

/// The context `context_id` of the user a task can be created in or moved to (see
/// [`Context::is_todoist_destination`]), with the Todoist project id it is bound to.
pub async fn resolve_todoist_context(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    context_id: ContextId,
) -> Result<(Context, ProjectId), UniversalInboxError> {
    resolve_context(
        repository,
        executor,
        user_id,
        context_id,
        Some(IntegrationProviderKind::Todoist),
    )
    .await
}

/// The context `context_id` of the user a task can be created in or moved to (see
/// [`Context::task_destination_provider`]), with the project id it is bound to: a Todoist
/// project, or a TickTick list (`inbox` for the inbox). A TickTick folder holds no task.
/// `task_manager`: the task manager the task lives in, if known; otherwise the context's own.
pub async fn resolve_context(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    context_id: ContextId,
    task_manager: Option<IntegrationProviderKind>,
) -> Result<(Context, ProjectId), UniversalInboxError> {
    let context = repository
        .get_context(executor, context_id)
        .await?
        .filter(|context| context.user_id == user_id)
        .ok_or_else(|| {
            UniversalInboxError::UnresolvedTaskDestination(format!(
                "Project {context_id} does not exist"
            ))
        })?;
    let name = &context.name;
    let bound_to = context.binding.as_ref().map(|binding| binding.provider);
    let Some(task_manager) = task_manager.or(bound_to) else {
        return Err(UniversalInboxError::UnresolvedTaskDestination(format!(
            "Project \"{name}\" is not synced from a task manager"
        )));
    };
    let reason = match (&context.binding, context.status) {
        (Some(binding), ContextStatus::Active)
            if context.task_destination_provider() == Some(task_manager) =>
        {
            let source_id = binding.source_id.clone();
            return Ok((context, source_id));
        }
        _ if bound_to != Some(task_manager) => {
            format!("Project \"{name}\" is not a {task_manager} project")
        }
        _ if context.is_ticktick_folder() => format!(
            "‘{name}’ is a TickTick folder, which holds lists, not tasks. Choose one of its lists."
        ),
        (_, ContextStatus::Archived) => format!("Project \"{name}\" is archived in {task_manager}"),
        _ => format!("Project \"{name}\" no longer exists in {task_manager}"),
    };
    Err(UniversalInboxError::UnresolvedTaskDestination(reason))
}

/// The destination of a stored default project. `None`: no default, the task manager's inbox.
///
/// A default that does not resolve fails loudly, it never falls back to the inbox: an
/// `Unresolved` default, a context gone, archived or not Todoist's. A legacy Todoist value not
/// migrated yet resolves by its project id only.
pub async fn resolve_default_destination(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    default_project: Option<&DefaultProject>,
) -> Result<Option<TaskDestination>, UniversalInboxError> {
    let context_id = match default_project {
        None => return Ok(None),
        Some(DefaultProject::TickTick(project)) => {
            return Ok(Some(TaskDestination::TickTickProject(
                project.source_id.clone(),
            )));
        }
        Some(DefaultProject::Context(context_id)) => *context_id,
        Some(DefaultProject::LegacyTodoist(project)) => repository
            .get_mirrored_context_id(
                executor,
                user_id,
                IntegrationProviderKind::Todoist,
                &project.source_id,
            )
            .await?
            .ok_or_else(|| stale_default(Some(&project.name)))?,
        Some(DefaultProject::Unresolved { name }) => {
            return Err(stale_default(name.as_deref()));
        }
    };
    resolve_todoist_context(repository, executor, user_id, context_id)
        .await
        .map_err(|err| match err {
            UniversalInboxError::UnresolvedTaskDestination(message) => {
                UniversalInboxError::UnresolvedTaskDestination(format!(
                    "{message} (your default project). Pick another in Settings."
                ))
            }
            err => err,
        })?;
    Ok(Some(TaskDestination::Context(context_id)))
}

fn stale_default(name: Option<&str>) -> UniversalInboxError {
    UniversalInboxError::UnresolvedTaskDestination(match name {
        Some(name) => {
            format!("Your default project \"{name}\" no longer exists. Pick another in Settings.")
        }
        None => "Your default project no longer exists. Pick another in Settings.".to_string(),
    })
}

/// The project a task synced from Linear or Slack starts in, from its integration's default
/// project: its context for Todoist, the inbox context without a default.
///
/// Lenient: a default that does not resolve seeds no context, the sink creation, which needs
/// it, fails loudly instead ([`sink_destination`]). Resolved once per sync and passed in the
/// task creation config.
pub async fn seed_task_project(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    default_project: Option<&DefaultProject>,
    task_manager: IntegrationProviderKind,
) -> Result<TaskProject, UniversalInboxError> {
    let project = |context_id, project_name: &str| TaskProject {
        context_id,
        project_name: project_name.to_string(),
    };
    let context_id = match default_project {
        None if task_manager == IntegrationProviderKind::Todoist => {
            let inbox = repository
                .get_inbox_context_id(executor, user_id, task_manager)
                .await?;
            return Ok(project(inbox, TODOIST_INBOX_PROJECT));
        }
        None => return Ok(project(None, TICKTICK_INBOX_PROJECT)),
        Some(DefaultProject::TickTick(ticktick_project)) => {
            return Ok(project(None, &ticktick_project.name));
        }
        Some(DefaultProject::Unresolved { name }) => {
            return Ok(project(None, name.as_deref().unwrap_or(NO_PROJECT)));
        }
        Some(DefaultProject::Context(context_id)) => Some(*context_id),
        Some(DefaultProject::LegacyTodoist(legacy)) => {
            match repository
                .get_mirrored_context_id(executor, user_id, task_manager, &legacy.source_id)
                .await?
            {
                Some(context_id) => Some(context_id),
                None => return Ok(project(None, &legacy.name)),
            }
        }
    };
    let context = match context_id {
        Some(context_id) => repository
            .get_context(executor, context_id)
            .await?
            .filter(|context| context.user_id == user_id),
        None => None,
    };
    Ok(match context {
        Some(context) => project(Some(context.id), &context.name),
        None => project(None, NO_PROJECT),
    })
}

/// Shown for a task whose project cannot be told
const NO_PROJECT: &str = "No project";

/// Where to create the sink item of `task` in `task_manager`: the context the task is bound to
/// for Todoist, else its integration's default project. `None`: the task manager's inbox. A
/// default that does not resolve is an error, never the inbox.
pub async fn sink_destination(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    task: &Task,
    task_manager: IntegrationProviderKind,
    default_project: Option<&DefaultProject>,
) -> Result<Option<TaskDestination>, UniversalInboxError> {
    if task_manager == IntegrationProviderKind::Todoist
        && let Some(context_id) = task.context_id
    {
        return Ok(Some(TaskDestination::Context(context_id)));
    }
    resolve_default_destination(repository, executor, task.user_id, default_project).await
}

/// The task manager of the user: the one a Linear or Slack default project is meant for when
/// the config does not name one (same fallback as the sink of a synced task).
pub async fn preferred_task_manager(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<IntegrationProviderKind, UniversalInboxError> {
    Ok(repository
        .get_user_preferences(executor, user_id)
        .await?
        .and_then(|preferences| preferences.default_task_manager_provider_kind)
        .unwrap_or(IntegrationProviderKind::Todoist))
}

/// Fill `default_project_issue` on each of the user's `connections`
pub async fn fill_default_project_issues(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    connections: &mut [IntegrationConnection],
) -> Result<(), UniversalInboxError> {
    let preferred = preferred_task_manager(repository, executor, user_id).await?;
    for connection in connections.iter_mut() {
        let Some((default_project, task_manager)) =
            connection.provider.config().default_project(preferred)
        else {
            connection.default_project_issue = None;
            continue;
        };
        let context = match default_project {
            DefaultProject::Context(context_id) => repository
                .get_context(executor, context_id)
                .await?
                .filter(|context| context.user_id == user_id),
            _ => None,
        };
        connection.default_project_issue =
            DefaultProjectIssue::of(&default_project, task_manager, context.as_ref());
    }
    Ok(())
}

/// A default project the user picks in the settings must be one they can pick in the picker:
/// one of their own active Todoist contexts, for a Todoist task manager. Only a changed default
/// is checked: saving another setting never fails on a default that went stale since.
pub async fn validate_default_project(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
    config: &IntegrationConnectionConfig,
    previous_config: Option<&IntegrationConnectionConfig>,
) -> Result<(), UniversalInboxError> {
    let preferred = preferred_task_manager(repository, executor, user_id).await?;
    let default_project = config.default_project(preferred);
    if previous_config.and_then(|previous| previous.default_project(preferred)) == default_project {
        return Ok(());
    }
    match default_project {
        Some((DefaultProject::Context(context_id), _)) => {
            resolve_todoist_context(repository, executor, user_id, context_id).await?;
        }
        // A context saved as the default of a TickTick task manager
        Some((DefaultProject::Unresolved { name: None }, task_manager)) => {
            return Err(UniversalInboxError::UnresolvedTaskDestination(format!(
                "A Todoist project cannot be the default project of {task_manager}"
            )));
        }
        _ => {}
    }
    Ok(())
}

/// Migrate every legacy Todoist default project of the user to its context (see
/// [`DefaultProject::migrated`]), in the transaction of the Todoist context sync. A rerun changes
/// nothing. Returns how many configs changed.
pub async fn migrate_todoist_default_projects(
    repository: &Repository,
    executor: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<usize, UniversalInboxError> {
    let preferred = preferred_task_manager(repository, executor, user_id).await?;
    let contexts = repository.list_contexts_for_user(executor, user_id).await?;
    let connections = repository
        .fetch_all_integration_connections(executor, user_id, None, true)
        .await?;
    let mut migrated = 0;
    for connection in connections {
        let mut config = connection.provider.config();
        let Some(new_default) = config
            .default_project(preferred)
            .and_then(|(default_project, _)| default_project.migrated(&contexts))
        else {
            continue;
        };
        if let Some(slot) = config.default_project_slot() {
            *slot.project = Some(new_default);
        }
        repository
            .update_integration_connection_config(executor, connection.id, config, user_id)
            .await?;
        migrated += 1;
    }
    Ok(migrated)
}
