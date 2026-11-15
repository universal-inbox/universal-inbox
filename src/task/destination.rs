//! Where a task goes, always by id: the destination of a task, the stored default project of an
//! integration config, and what a default turns into once its task manager is known.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    context::{Context, ContextId, ContextStatus},
    integration_connection::provider::IntegrationProviderKind,
    task::ProjectSummary,
};

/// Where a task is created or moved to, always by id. A name is never a destination.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, JsonSchema)]
#[serde(tag = "type", content = "id")]
pub enum TaskDestination {
    /// A context mirroring a Todoist project
    Context(ContextId),
    /// A TickTick project, by its TickTick id: TickTick projects are not mirrored as contexts
    /// yet
    TickTickProject(super::ProjectId),
}

impl fmt::Display for TaskDestination {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TaskDestination::Context(id) => write!(f, "context {id}"),
            TaskDestination::TickTickProject(id) => write!(f, "TickTick project {id}"),
        }
    }
}

/// How a default project is stored in an integration config (Todoist and TickTick
/// `default_project`, Linear and Slack `target_project`): the three JSON shapes
/// `{"context_id": ...}`, `{"unresolved": {...}}` and `{"source_id": ..., "name": ...}`.
///
/// Its meaning depends on the task manager it is meant for: read it through [`DefaultProject`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(untagged)]
pub enum ProjectReference {
    Context { context_id: ContextId },
    Unresolved { unresolved: ProjectSummary },
    Project(ProjectSummary),
}

impl From<ContextId> for ProjectReference {
    fn from(context_id: ContextId) -> Self {
        ProjectReference::Context { context_id }
    }
}

/// A stored default project, read for the task manager it is meant for. Every reader of a
/// default goes through it, so a stored shape means one thing only.
#[derive(Debug, PartialEq, Clone, Eq)]
pub enum DefaultProject {
    /// A Todoist project, by its context
    Context(ContextId),
    /// A TickTick project, by its TickTick id
    TickTick(ProjectSummary),
    /// A Todoist project stored before contexts existed (`{source_id, name}`): migrated to its
    /// context after the next Todoist context sync, by its project id
    LegacyTodoist(ProjectSummary),
    /// A default that cannot be used: a legacy Todoist value the migration could not match,
    /// or a context stored for TickTick. It never resolves by itself.
    Unresolved { name: Option<String> },
}

impl DefaultProject {
    /// `reference` read for `task_manager` (Todoist or TickTick)
    pub fn new(reference: &ProjectReference, task_manager: IntegrationProviderKind) -> Self {
        let for_ticktick = task_manager == IntegrationProviderKind::TickTick;
        match reference {
            ProjectReference::Context { context_id } if !for_ticktick => {
                DefaultProject::Context(*context_id)
            }
            ProjectReference::Context { .. } => DefaultProject::Unresolved { name: None },
            ProjectReference::Project(project) if for_ticktick => {
                DefaultProject::TickTick(project.clone())
            }
            ProjectReference::Project(project) => DefaultProject::LegacyTodoist(project.clone()),
            ProjectReference::Unresolved { unresolved } => DefaultProject::Unresolved {
                name: Some(unresolved.name.clone()),
            },
        }
    }

    /// Where a task created with this default goes, when it is known without a lookup. A
    /// legacy Todoist value needs its context (see [`Self::migrated`]); an unresolved one has
    /// none.
    pub fn destination(&self) -> Option<TaskDestination> {
        match self {
            DefaultProject::Context(context_id) => Some(TaskDestination::Context(*context_id)),
            DefaultProject::TickTick(project) => {
                Some(TaskDestination::TickTickProject(project.source_id.clone()))
            }
            DefaultProject::LegacyTodoist(_) | DefaultProject::Unresolved { .. } => None,
        }
    }

    /// The value to store instead of a legacy Todoist default, found among the user's
    /// `contexts`: the context bound to its project id, else the only usable context with its
    /// exact name, else `Unresolved`. Never creates anything. `None`: nothing to migrate.
    pub fn migrated(&self, contexts: &[Context]) -> Option<ProjectReference> {
        let DefaultProject::LegacyTodoist(project) = self else {
            return None;
        };
        let by_id = contexts.iter().find(|context| {
            context.is_todoist_bound()
                && context.status != ContextStatus::Tombstoned
                && context
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.source_id == project.source_id)
        });
        let by_name = || {
            let mut named = contexts
                .iter()
                .filter(|context| context.is_todoist_destination() && context.name == project.name);
            match (named.next(), named.next()) {
                (Some(context), None) => Some(context),
                _ => None,
            }
        };
        Some(match by_id.or_else(by_name) {
            Some(context) => ProjectReference::Context {
                context_id: context.id,
            },
            None => ProjectReference::Unresolved {
                unresolved: project.clone(),
            },
        })
    }
}

/// Why a default project cannot be used. Computed when the integration connection is read,
/// never stored.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum DefaultProjectIssue {
    /// The project is gone from the provider. `name` is the last known name, if any.
    Missing {
        provider: IntegrationProviderKind,
        name: Option<String>,
    },
    /// The project is archived in the provider, which accepts no new task in it
    Archived {
        provider: IntegrationProviderKind,
        name: String,
    },
}

impl DefaultProjectIssue {
    /// The issue of `default` for `provider`, `context` being the context it points to (among
    /// the user's), if any. A legacy Todoist value has none: it is migrated after the next sync.
    pub fn of(
        default: &DefaultProject,
        provider: IntegrationProviderKind,
        context: Option<&Context>,
    ) -> Option<Self> {
        match default {
            DefaultProject::Unresolved { name } => Some(DefaultProjectIssue::Missing {
                provider,
                name: name.clone(),
            }),
            DefaultProject::Context(_) => match context {
                None => Some(DefaultProjectIssue::Missing {
                    provider,
                    name: None,
                }),
                Some(context) => match context.status {
                    ContextStatus::Active => None,
                    ContextStatus::Archived => Some(DefaultProjectIssue::Archived {
                        provider,
                        name: context.name.clone(),
                    }),
                    ContextStatus::Tombstoned => Some(DefaultProjectIssue::Missing {
                        provider,
                        name: Some(context.name.clone()),
                    }),
                },
            },
            // TickTick projects are not mirrored: a TickTick id is checked when it is used
            DefaultProject::TickTick(_) | DefaultProject::LegacyTodoist(_) => None,
        }
    }
}

impl fmt::Display for DefaultProjectIssue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DefaultProjectIssue::Missing {
                provider,
                name: Some(name),
            } => write!(
                f,
                "Your default project \"{name}\" no longer exists in {provider}. Pick another."
            ),
            DefaultProjectIssue::Missing {
                provider,
                name: None,
            } => write!(
                f,
                "Your default project no longer exists in {provider}. Pick another."
            ),
            DefaultProjectIssue::Archived { provider, name } => write!(
                f,
                "Your default project \"{name}\" is archived in {provider}. Pick another."
            ),
        }
    }
}

/// The project a task is bound to: its context (`None` for a TickTick project, which has no
/// context yet) and the name kept in the deprecated `project` column, display only.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct TaskProject {
    pub context_id: Option<ContextId>,
    pub project_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context::{ContextBinding, NoteId},
        task::{TaskCreation, TaskPlanning, service::TaskPatch},
        user::UserId,
    };
    use chrono::Utc;
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use uuid::Uuid;

    fn summary(source_id: &str, name: &str) -> ProjectSummary {
        ProjectSummary {
            source_id: source_id.into(),
            name: name.to_string(),
        }
    }

    fn context(source_id: &str, name: &str, status: ContextStatus) -> Context {
        Context {
            id: ContextId(Uuid::new_v4()),
            user_id: UserId(Uuid::nil()),
            parent_id: None,
            name: name.to_string(),
            binding: Some(
                ContextBinding::new(IntegrationProviderKind::Todoist, source_id.into(), true)
                    .unwrap(),
            ),
            status,
            is_inbox: false,
            note_id: NoteId(Uuid::new_v4()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_destination_serialization() {
        assert_eq!(
            serde_json::to_value(TaskDestination::Context(ContextId(Uuid::nil()))).unwrap(),
            serde_json::json!({ "type": "Context", "id": Uuid::nil() })
        );
        assert_eq!(
            serde_json::from_value::<TaskDestination>(
                serde_json::json!({ "type": "TickTickProject", "id": "6f1a" })
            )
            .unwrap(),
            TaskDestination::TickTickProject("6f1a".into())
        );
    }

    #[test]
    fn test_a_project_name_is_never_a_destination() {
        assert!(serde_json::from_str::<TaskPatch>(r#"{"project_name": "Work"}"#).is_err());
        assert!(
            serde_json::from_str::<TaskCreation>(
                r#"{"title": "t", "priority": 4, "project_name": "Work"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<TaskPlanning>(
                r#"{"project_name": "Work", "priority": 4, "due_at": null}"#
            )
            .is_err()
        );
    }

    #[test]
    fn test_project_reference_reads_every_stored_shape() {
        assert_eq!(
            serde_json::from_value::<ProjectReference>(
                serde_json::json!({ "source_id": "2203306141", "name": "Work" })
            )
            .unwrap(),
            ProjectReference::Project(summary("2203306141", "Work"))
        );
        assert_eq!(
            serde_json::from_value::<ProjectReference>(
                serde_json::json!({ "context_id": Uuid::nil() })
            )
            .unwrap(),
            ProjectReference::Context {
                context_id: ContextId(Uuid::nil())
            }
        );
        let unresolved = ProjectReference::Unresolved {
            unresolved: summary("1", "Old"),
        };
        assert_eq!(
            serde_json::from_value::<ProjectReference>(serde_json::to_value(&unresolved).unwrap())
                .unwrap(),
            unresolved
        );
    }

    /// A stored shape means one thing for each task manager: a `{source_id, name}` is a
    /// TickTick project for TickTick and a legacy value for Todoist, never a TickTick
    /// destination for Todoist
    #[rstest]
    #[case::todoist_context(
        ProjectReference::Context { context_id: ContextId(Uuid::nil()) },
        IntegrationProviderKind::Todoist,
        DefaultProject::Context(ContextId(Uuid::nil())),
        Some(TaskDestination::Context(ContextId(Uuid::nil())))
    )]
    #[case::ticktick_project(
        ProjectReference::Project(summary("tt1", "Work")),
        IntegrationProviderKind::TickTick,
        DefaultProject::TickTick(summary("tt1", "Work")),
        Some(TaskDestination::TickTickProject("tt1".into()))
    )]
    #[case::legacy_todoist(
        ProjectReference::Project(summary("2222", "Work")),
        IntegrationProviderKind::Todoist,
        DefaultProject::LegacyTodoist(summary("2222", "Work")),
        None
    )]
    #[case::context_for_ticktick(
        ProjectReference::Context { context_id: ContextId(Uuid::nil()) },
        IntegrationProviderKind::TickTick,
        DefaultProject::Unresolved { name: None },
        None
    )]
    #[case::unresolved(
        ProjectReference::Unresolved { unresolved: summary("1", "Old") },
        IntegrationProviderKind::Todoist,
        DefaultProject::Unresolved { name: Some("Old".to_string()) },
        None
    )]
    fn test_a_stored_default_is_read_for_its_task_manager(
        #[case] reference: ProjectReference,
        #[case] task_manager: IntegrationProviderKind,
        #[case] expected: DefaultProject,
        #[case] destination: Option<TaskDestination>,
    ) {
        let default = DefaultProject::new(&reference, task_manager);
        assert_eq!(default, expected);
        assert_eq!(default.destination(), destination);
    }

    #[test]
    fn test_a_legacy_default_migrates_by_id_then_unique_name() {
        let work = context("2222", "Work", ContextStatus::Active);
        let other = context("3333", "Other", ContextStatus::Active);
        let contexts = vec![work.clone(), other.clone()];
        let migrated = |source_id: &str, name: &str| {
            DefaultProject::LegacyTodoist(summary(source_id, name)).migrated(&contexts)
        };

        // By id, even when the project was renamed since
        assert_eq!(migrated("2222", "Renamed"), Some(work.id.into()));
        // By a unique exact name when the id is unknown
        assert_eq!(migrated("gone", "Other"), Some(other.id.into()));
        // Neither: unresolved, never created
        assert_eq!(
            migrated("gone", "Nowhere"),
            Some(ProjectReference::Unresolved {
                unresolved: summary("gone", "Nowhere")
            })
        );
        // Only legacy values migrate
        assert_eq!(DefaultProject::Context(work.id).migrated(&contexts), None);
    }

    #[test]
    fn test_an_ambiguous_or_archived_name_does_not_migrate() {
        let contexts = vec![
            context("1", "Work", ContextStatus::Active),
            context("2", "Work", ContextStatus::Active),
            context("3", "Old", ContextStatus::Archived),
        ];
        for name in ["Work", "Old"] {
            assert_eq!(
                DefaultProject::LegacyTodoist(summary("gone", name)).migrated(&contexts),
                Some(ProjectReference::Unresolved {
                    unresolved: summary("gone", name)
                })
            );
        }
    }

    #[test]
    fn test_default_project_issue() {
        let provider = IntegrationProviderKind::Todoist;
        let archived = context("1", "Old", ContextStatus::Archived);
        let default = DefaultProject::Context(archived.id);
        assert_eq!(
            DefaultProjectIssue::of(&default, provider, Some(&archived)),
            Some(DefaultProjectIssue::Archived {
                provider,
                name: "Old".to_string()
            })
        );
        assert_eq!(
            DefaultProjectIssue::of(&default, provider, None),
            Some(DefaultProjectIssue::Missing {
                provider,
                name: None
            })
        );
        let active = context("2", "Work", ContextStatus::Active);
        assert_eq!(
            DefaultProjectIssue::of(&DefaultProject::Context(active.id), provider, Some(&active)),
            None
        );
        assert_eq!(
            DefaultProjectIssue::of(
                &DefaultProject::LegacyTodoist(summary("1", "W")),
                provider,
                None
            ),
            None
        );
    }
}
