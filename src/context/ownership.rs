//! Who owns the name and the parent of a context, and so which writes Universal Inbox accepts:
//! the provider owns those of a mirrored context, Universal Inbox those of every other one.

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use super::{Context, ContextId, ContextStatus};
use crate::{integration_connection::provider::IntegrationProviderKind, task::ProjectId};

/// Where a new context is created, decided by its parent
#[derive(Debug, PartialEq, Clone, Eq)]
pub enum ContextCreation {
    /// In Universal Inbox only: unbound, no provider is called
    Local,
    /// In the provider first, under `parent_source_id`, then bound to the project it returns
    Upstream {
        provider: UpstreamProvider,
        parent_source_id: ProjectId,
    },
}

/// A provider Universal Inbox creates projects in
#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum UpstreamProvider {
    Todoist,
}

impl From<UpstreamProvider> for IntegrationProviderKind {
    fn from(provider: UpstreamProvider) -> Self {
        match provider {
            UpstreamProvider::Todoist => IntegrationProviderKind::Todoist,
        }
    }
}

impl UpstreamProvider {
    fn of(provider: IntegrationProviderKind) -> Option<Self> {
        match provider {
            IntegrationProviderKind::Todoist => Some(UpstreamProvider::Todoist),
            _ => None,
        }
    }
}

/// The message shown when `provider` could not create the project `name`: nothing was stored
pub fn upstream_creation_failed_message(name: &str, provider: UpstreamProvider) -> String {
    let provider = IntegrationProviderKind::from(provider);
    format!(
        "Couldn’t create ‘{name}’ in {provider}, so nothing was created here. Please try again."
    )
}

/// A write to a context refused by its ownership. The message is shown to the user as is.
#[derive(Debug, PartialEq, Clone, Eq)]
pub enum ContextWriteError {
    MirroredName {
        name: String,
        provider: IntegrationProviderKind,
    },
    MirroredParent {
        name: String,
        provider: IntegrationProviderKind,
    },
    ArchivedMirroredParent {
        parent: String,
        provider: IntegrationProviderKind,
    },
    ArchivedParent {
        parent: String,
    },
    LocalUnderMirroredParent {
        name: String,
        parent: String,
        provider: IntegrationProviderKind,
    },
    UpstreamCreationUnsupported {
        parent: String,
        provider: IntegrationProviderKind,
    },
    ParentIsSelf {
        name: String,
    },
    ParentIsDescendant {
        name: String,
        parent: String,
    },
}

impl fmt::Display for ContextWriteError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ContextWriteError::MirroredName { name, provider } => write!(
                f,
                "‘{name}’ is synced from {provider}, which owns its name. Rename it in {provider} \
                 and the new name will show here after the next sync."
            ),
            ContextWriteError::MirroredParent { name, provider } => write!(
                f,
                "‘{name}’ is synced from {provider}, which owns where it sits. Move it in \
                 {provider} and the change will show here after the next sync."
            ),
            ContextWriteError::ArchivedMirroredParent { parent, provider } => write!(
                f,
                "‘{parent}’ is archived in {provider}, so nothing can be added under it. \
                 Unarchive it in {provider} first, or choose another parent."
            ),
            ContextWriteError::ArchivedParent { parent } => write!(
                f,
                "‘{parent}’ is archived, so nothing can be added under it. Choose another parent."
            ),
            ContextWriteError::LocalUnderMirroredParent {
                name,
                parent,
                provider,
            } => write!(
                f,
                "‘{name}’ exists only in Universal Inbox and can’t be moved under ‘{parent}’, \
                 which is synced from {provider}. Create a new project under ‘{parent}’ instead \
                 and it will be added to {provider} too."
            ),
            ContextWriteError::UpstreamCreationUnsupported { parent, provider } => write!(
                f,
                "‘{parent}’ is synced from {provider}, and Universal Inbox can’t add projects to \
                 {provider} yet. Create the project under ‘{parent}’ in {provider} and it will \
                 show here after the next sync."
            ),
            ContextWriteError::ParentIsSelf { name } => write!(
                f,
                "‘{name}’ can’t be moved under itself. Choose another parent."
            ),
            ContextWriteError::ParentIsDescendant { name, parent } => write!(
                f,
                "‘{name}’ can’t be moved under ‘{parent}’, because ‘{parent}’ is inside it. \
                 Choose another parent."
            ),
        }
    }
}

impl std::error::Error for ContextWriteError {}

/// Where a context is created under `parent` (`None`: at the root): the provider owns the
/// children of a mirrored parent. An archived parent takes no child.
pub fn plan_context_creation(
    parent: Option<&Context>,
) -> Result<ContextCreation, ContextWriteError> {
    let Some(parent) = parent else {
        return Ok(ContextCreation::Local);
    };
    let upstream = parent.upstream_parent_of();
    if parent.status == ContextStatus::Archived {
        return Err(match upstream {
            Some(binding) => ContextWriteError::ArchivedMirroredParent {
                parent: parent.name.clone(),
                provider: binding.provider,
            },
            None => ContextWriteError::ArchivedParent {
                parent: parent.name.clone(),
            },
        });
    }
    let Some(binding) = upstream else {
        return Ok(ContextCreation::Local);
    };
    match UpstreamProvider::of(binding.provider) {
        Some(provider) => Ok(ContextCreation::Upstream {
            provider,
            parent_source_id: binding.source_id.clone(),
        }),
        None => Err(ContextWriteError::UpstreamCreationUnsupported {
            parent: parent.name.clone(),
            provider: binding.provider,
        }),
    }
}

/// Rename `context` to `name`: only Universal Inbox's own contexts can be. Keeping the name is
/// not a write.
pub fn check_context_rename(context: &Context, name: &str) -> Result<(), ContextWriteError> {
    match context.mirrored_from() {
        Some(provider) if context.name != name => Err(ContextWriteError::MirroredName {
            name: context.name.clone(),
            provider,
        }),
        _ => Ok(()),
    }
}

/// Move `context` under `new_parent` (`None`: to the root). Only Universal Inbox's own contexts
/// can move, never under a parent the provider owns the children of, an archived one, or one of
/// their own descendants. `contexts` holds the ancestors of `new_parent`. Keeping the parent is
/// not a write.
pub fn check_context_reparent(
    context: &Context,
    new_parent: Option<&Context>,
    contexts: &[Context],
) -> Result<(), ContextWriteError> {
    if context.parent_id == new_parent.map(|parent| parent.id) {
        return Ok(());
    }
    if let Some(provider) = context.mirrored_from() {
        return Err(ContextWriteError::MirroredParent {
            name: context.name.clone(),
            provider,
        });
    }
    let Some(new_parent) = new_parent else {
        return Ok(());
    };
    if new_parent.id == context.id {
        return Err(ContextWriteError::ParentIsSelf {
            name: context.name.clone(),
        });
    }
    if let Some(binding) = new_parent.upstream_parent_of() {
        return Err(ContextWriteError::LocalUnderMirroredParent {
            name: context.name.clone(),
            parent: new_parent.name.clone(),
            provider: binding.provider,
        });
    }
    if new_parent.status == ContextStatus::Archived {
        return Err(ContextWriteError::ArchivedParent {
            parent: new_parent.name.clone(),
        });
    }
    let by_id: HashMap<ContextId, &Context> = contexts.iter().map(|c| (c.id, c)).collect();
    let mut seen: HashSet<ContextId> = HashSet::new();
    let mut current = Some(new_parent);
    while let Some(ancestor) = current
        && seen.insert(ancestor.id)
    {
        if ancestor.id == context.id {
            return Err(ContextWriteError::ParentIsDescendant {
                name: context.name.clone(),
                parent: new_parent.name.clone(),
            });
        }
        current = ancestor.parent_id.and_then(|id| by_id.get(&id).copied());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rstest::*;
    use uuid::Uuid;

    use super::*;
    use crate::{
        context::{Context, ContextBinding, NoteId, summarize_contexts},
        user::UserId,
    };

    const TODOIST: IntegrationProviderKind = IntegrationProviderKind::Todoist;

    fn context(name: &str, binding: Option<(IntegrationProviderKind, bool)>) -> Context {
        let now = Utc::now();
        Context {
            id: ContextId(Uuid::new_v4()),
            user_id: UserId(Uuid::new_v4()),
            parent_id: None,
            name: name.to_string(),
            binding: binding.map(|(provider, mirrored)| {
                ContextBinding::new(provider, format!("{name}-id").into(), mirrored).unwrap()
            }),
            status: ContextStatus::Active,
            is_inbox: false,
            note_id: NoteId(Uuid::new_v4()),
            created_at: now,
            updated_at: now,
        }
    }

    fn mirrored(name: &str) -> Context {
        context(name, Some((TODOIST, true)))
    }

    fn unbound(name: &str) -> Context {
        context(name, None)
    }

    fn with_status(mut context: Context, status: ContextStatus) -> Context {
        context.status = status;
        context
    }

    fn under(mut context: Context, parent: &Context) -> Context {
        context.parent_id = Some(parent.id);
        context
    }

    #[test]
    fn test_a_root_context_is_created_locally() {
        assert_eq!(plan_context_creation(None), Ok(ContextCreation::Local));
    }

    #[rstest]
    #[case::active(ContextStatus::Active)]
    #[case::tombstoned(ContextStatus::Tombstoned)]
    fn test_a_child_of_a_local_parent_is_created_locally(#[case] status: ContextStatus) {
        let parent = with_status(unbound("Home"), status);
        assert_eq!(
            plan_context_creation(Some(&parent)),
            Ok(ContextCreation::Local)
        );
    }

    #[test]
    fn test_a_child_of_a_tombstoned_mirrored_parent_is_created_locally() {
        let parent = with_status(mirrored("Gone"), ContextStatus::Tombstoned);
        assert_eq!(
            plan_context_creation(Some(&parent)),
            Ok(ContextCreation::Local)
        );
    }

    #[test]
    fn test_a_child_of_a_not_mirrored_bound_parent_is_created_locally() {
        let parent = context("Shared", Some((TODOIST, false)));
        assert_eq!(
            plan_context_creation(Some(&parent)),
            Ok(ContextCreation::Local)
        );
    }

    #[test]
    fn test_a_child_of_an_active_mirrored_parent_is_created_upstream() {
        let parent = mirrored("Work");
        assert_eq!(
            plan_context_creation(Some(&parent)),
            Ok(ContextCreation::Upstream {
                provider: UpstreamProvider::Todoist,
                parent_source_id: "Work-id".into(),
            })
        );
    }

    #[test]
    fn test_an_archived_parent_takes_no_child() {
        let parent = with_status(mirrored("Old"), ContextStatus::Archived);
        let err = plan_context_creation(Some(&parent)).unwrap_err();
        assert_eq!(
            err,
            ContextWriteError::ArchivedMirroredParent {
                parent: "Old".to_string(),
                provider: TODOIST
            }
        );
        assert_eq!(
            err.to_string(),
            "‘Old’ is archived in Todoist, so nothing can be added under it. Unarchive it in \
             Todoist first, or choose another parent."
        );

        let parent = with_status(unbound("Old"), ContextStatus::Archived);
        let err = plan_context_creation(Some(&parent)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "‘Old’ is archived, so nothing can be added under it. Choose another parent."
        );
    }

    #[test]
    fn test_a_failed_upstream_creation_names_the_provider() {
        assert_eq!(
            upstream_creation_failed_message("Launch", UpstreamProvider::Todoist),
            "Couldn’t create ‘Launch’ in Todoist, so nothing was created here. Please try again."
        );
    }

    #[test]
    fn test_a_child_of_a_ticktick_mirrored_parent_cannot_be_created() {
        let parent = context("Errands", Some((IntegrationProviderKind::TickTick, true)));
        let err = plan_context_creation(Some(&parent)).unwrap_err();
        assert_eq!(
            err,
            ContextWriteError::UpstreamCreationUnsupported {
                parent: "Errands".to_string(),
                provider: IntegrationProviderKind::TickTick
            }
        );
        assert_eq!(
            err.to_string(),
            "‘Errands’ is synced from TickTick, and Universal Inbox can’t add projects to \
             TickTick yet. Create the project under ‘Errands’ in TickTick and it will show here \
             after the next sync."
        );
    }

    #[rstest]
    #[case::active(ContextStatus::Active)]
    #[case::archived(ContextStatus::Archived)]
    #[case::tombstoned(ContextStatus::Tombstoned)]
    fn test_a_mirrored_context_cannot_be_renamed(#[case] status: ContextStatus) {
        let work = with_status(mirrored("Work"), status);
        let err = check_context_rename(&work, "Job").unwrap_err();
        assert_eq!(
            err.to_string(),
            "‘Work’ is synced from Todoist, which owns its name. Rename it in Todoist and the \
             new name will show here after the next sync."
        );
        assert_eq!(check_context_rename(&work, "Work"), Ok(()));
    }

    #[rstest]
    #[case::active(ContextStatus::Active)]
    #[case::archived(ContextStatus::Archived)]
    fn test_a_local_context_can_be_renamed(#[case] status: ContextStatus) {
        let home = with_status(unbound("Home"), status);
        assert_eq!(check_context_rename(&home, "House"), Ok(()));
    }

    #[rstest]
    #[case::active(ContextStatus::Active)]
    #[case::archived(ContextStatus::Archived)]
    #[case::tombstoned(ContextStatus::Tombstoned)]
    fn test_a_mirrored_context_cannot_be_moved_anywhere(#[case] status: ContextStatus) {
        let root = mirrored("Root");
        let local = unbound("Local");
        let work = under(with_status(mirrored("Work"), status), &root);
        let contexts = vec![root.clone(), local.clone(), work.clone()];
        let expected = ContextWriteError::MirroredParent {
            name: "Work".to_string(),
            provider: TODOIST,
        };

        assert_eq!(
            check_context_reparent(&work, None, &contexts),
            Err(expected.clone())
        );
        assert_eq!(
            check_context_reparent(&work, Some(&local), &contexts),
            Err(expected.clone())
        );
        assert_eq!(
            expected.to_string(),
            "‘Work’ is synced from Todoist, which owns where it sits. Move it in Todoist and \
             the change will show here after the next sync."
        );
        assert_eq!(
            check_context_reparent(&work, Some(&root), &contexts),
            Ok(())
        );
    }

    #[test]
    fn test_a_local_context_cannot_be_moved_under_a_mirrored_parent() {
        let work = mirrored("Work");
        let home = unbound("Home");
        let err =
            check_context_reparent(&home, Some(&work), std::slice::from_ref(&work)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "‘Home’ exists only in Universal Inbox and can’t be moved under ‘Work’, which is \
             synced from Todoist. Create a new project under ‘Work’ instead and it will be \
             added to Todoist too."
        );
    }

    #[test]
    fn test_a_local_context_moves_under_a_local_or_tombstoned_parent_or_to_the_root() {
        let garden = unbound("Garden");
        let gone = with_status(mirrored("Gone"), ContextStatus::Tombstoned);
        let home = under(unbound("Home"), &garden);
        let contexts = vec![garden.clone(), gone.clone(), home.clone()];

        assert_eq!(check_context_reparent(&home, None, &contexts), Ok(()));
        assert_eq!(
            check_context_reparent(&home, Some(&gone), &contexts),
            Ok(())
        );
        let shed = unbound("Shed");
        assert_eq!(
            check_context_reparent(&home, Some(&shed), &contexts),
            Ok(())
        );
    }

    #[test]
    fn test_a_local_context_cannot_be_moved_under_an_archived_parent() {
        let old = with_status(unbound("Old"), ContextStatus::Archived);
        let home = unbound("Home");
        assert_eq!(
            check_context_reparent(&home, Some(&old), std::slice::from_ref(&old)),
            Err(ContextWriteError::ArchivedParent {
                parent: "Old".to_string()
            })
        );
    }

    #[test]
    fn test_a_local_context_cannot_be_moved_under_itself_or_a_descendant() {
        let home = unbound("Home");
        let kitchen = under(unbound("Kitchen"), &home);
        let sink = under(unbound("Sink"), &kitchen);
        let contexts = vec![home.clone(), kitchen.clone(), sink.clone()];

        let itself = check_context_reparent(&home, Some(&home), &contexts).unwrap_err();
        assert_eq!(
            itself.to_string(),
            "‘Home’ can’t be moved under itself. Choose another parent."
        );
        let descendant = check_context_reparent(&home, Some(&sink), &contexts).unwrap_err();
        assert_eq!(
            descendant.to_string(),
            "‘Home’ can’t be moved under ‘Sink’, because ‘Sink’ is inside it. Choose another \
             parent."
        );
        assert_eq!(check_context_reparent(&sink, None, &contexts), Ok(()));
    }

    #[test]
    fn test_itself_is_told_by_id_never_by_name() {
        let home = unbound("Home");
        let other_home = unbound("Home");
        let contexts = vec![home.clone(), other_home.clone()];

        assert_eq!(
            check_context_reparent(&home, Some(&other_home), &contexts),
            Ok(())
        );
    }

    #[test]
    fn test_the_summary_tells_who_owns_the_name_and_parent() {
        let work = mirrored("Work");
        let gone = with_status(mirrored("Gone"), ContextStatus::Tombstoned);
        let home = unbound("Home");
        let summaries = summarize_contexts(&[work.clone(), gone.clone(), home.clone()]);

        assert_eq!(summaries[&work.id].mirrored_from, Some(TODOIST));
        assert_eq!(summaries[&gone.id].mirrored_from, Some(TODOIST));
        assert_eq!(summaries[&home.id].mirrored_from, None);
    }
}
