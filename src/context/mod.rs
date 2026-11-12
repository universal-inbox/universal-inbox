use std::{
    collections::{HashMap, HashSet},
    fmt,
    str::FromStr,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::provider::IntegrationProviderKind, task::ProjectId, user::UserId,
};

/// A named place work belongs to. Contexts form a tree and each has exactly one [`Note`].
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Context {
    pub id: ContextId,
    pub user_id: UserId,
    pub parent_id: Option<ContextId>,
    pub name: String,
    /// `None`: a UI-owned context, with no upstream. Universal Inbox owns its `name` and `parent_id`.
    pub binding: Option<ContextBinding>,
    pub status: ContextStatus,
    /// Always present: the note is created with the context.
    pub note_id: NoteId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Link between a context and the upstream project it mirrors. It is made on the provider's
/// `source_id`, never on the project name.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(try_from = "ContextBindingData")]
pub struct ContextBinding {
    /// A task service: Todoist or TickTick (see [`ContextBinding::new`])
    pub provider: IntegrationProviderKind,
    /// The provider's project id
    pub source_id: ProjectId,
    /// `true`: the upstream owns the `name` and `parent_id` of the context
    pub mirrored: bool,
}

/// Deserialization shape of [`ContextBinding`], validated through [`ContextBinding::new`]
#[derive(Deserialize)]
struct ContextBindingData {
    provider: IntegrationProviderKind,
    source_id: ProjectId,
    mirrored: bool,
}

impl TryFrom<ContextBindingData> for ContextBinding {
    type Error = ContextError;

    fn try_from(data: ContextBindingData) -> Result<Self, Self::Error> {
        ContextBinding::new(data.provider, data.source_id, data.mirrored)
    }
}

impl ContextBinding {
    pub fn new(
        provider: IntegrationProviderKind,
        source_id: ProjectId,
        mirrored: bool,
    ) -> Result<Self, ContextError> {
        if !matches!(
            provider,
            IntegrationProviderKind::Todoist | IntegrationProviderKind::TickTick
        ) {
            return Err(ContextError::InvalidBindingProvider(provider));
        }
        if source_id.0.is_empty() {
            return Err(ContextError::EmptyBindingSourceId);
        }

        Ok(Self {
            provider,
            source_id,
            mirrored,
        })
    }
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum ContextError {
    InvalidBindingProvider(IntegrationProviderKind),
    EmptyBindingSourceId,
    InvalidStatusChange {
        from: ContextStatus,
        change: ContextStatusChange,
    },
    InvalidNoteBackendKind,
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ContextError::InvalidBindingProvider(provider) => {
                write!(f, "A context cannot be bound to {provider}")
            }
            ContextError::EmptyBindingSourceId => {
                write!(f, "A context binding needs a source id")
            }
            ContextError::InvalidStatusChange { from, change } => {
                write!(f, "A {from} context cannot be changed with {change}")
            }
            ContextError::InvalidNoteBackendKind => {
                write!(f, "An external note backend cannot be of kind Local")
            }
        }
    }
}

impl std::error::Error for ContextError {}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!)]
    pub enum ContextStatus {
        Active,
        Archived,
        Tombstoned,
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!)]
    pub enum ContextStatusChange {
        Archive,
        Unarchive,
        /// The upstream project is gone: the context stays, with everything UI-owned
        Tombstone,
        /// The upstream project came back
        Restore,
    }
}

impl ContextStatus {
    /// The only way to change the status of a context.
    ///
    /// | from       | Archive  | Unarchive | Tombstone  | Restore |
    /// |------------|----------|-----------|------------|---------|
    /// | Active     | Archived | -         | Tombstoned | -       |
    /// | Archived   | -        | Active    | Tombstoned | -       |
    /// | Tombstoned | -        | -         | -          | Active  |
    pub fn apply(self, change: ContextStatusChange) -> Result<ContextStatus, ContextError> {
        match (self, change) {
            (ContextStatus::Active, ContextStatusChange::Archive) => Ok(ContextStatus::Archived),
            (ContextStatus::Archived, ContextStatusChange::Unarchive) => Ok(ContextStatus::Active),
            (ContextStatus::Active | ContextStatus::Archived, ContextStatusChange::Tombstone) => {
                Ok(ContextStatus::Tombstoned)
            }
            (ContextStatus::Tombstoned, ContextStatusChange::Restore) => Ok(ContextStatus::Active),
            (from, change) => Err(ContextError::InvalidStatusChange { from, change }),
        }
    }
}

/// An upstream project as the context mirror sees it: what upstream owns of a mirrored context.
#[derive(Debug, PartialEq, Clone, Eq)]
pub struct MirroredProject {
    pub source_id: ProjectId,
    pub name: String,
    /// The upstream parent project, `None` for a root project
    pub parent_source_id: Option<ProjectId>,
    pub is_archived: bool,
}

/// Why a project's upstream parent was dropped and the project mirrored at the root
#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum RootedParent {
    /// The parent project is not in the upstream response
    Dangling,
    /// The project is part of a parent cycle
    Cycle,
}

/// The parent `source_id` of each project `source_id`, `None` at the root
pub type MirroredParents = HashMap<ProjectId, Option<ProjectId>>;

/// The parent `source_id` of each project, resolved on `source_id` only, never on a name.
///
/// A parent missing from `projects` or a parent cycle puts the project at the root and is
/// reported in the second value. The order of `projects` does not matter.
pub fn resolve_mirrored_parents(
    projects: &[MirroredProject],
) -> (MirroredParents, Vec<(ProjectId, RootedParent)>) {
    let upstream: HashMap<&ProjectId, Option<&ProjectId>> = projects
        .iter()
        .map(|project| (&project.source_id, project.parent_source_id.as_ref()))
        .collect();
    let mut rooted: Vec<(ProjectId, RootedParent)> = vec![];
    let mut in_cycle: HashSet<&ProjectId> = HashSet::new();
    for project in projects {
        // Walk the ancestors: coming back to a project already on the path closes a cycle
        let mut path: Vec<&ProjectId> = vec![&project.source_id];
        let mut current = &project.source_id;
        while let Some(Some(parent)) = upstream.get(current) {
            if let Some(start) = path.iter().position(|id| id == parent) {
                in_cycle.extend(path[start..].iter().copied());
                break;
            }
            if in_cycle.contains(parent) {
                break;
            }
            path.push(parent);
            current = parent;
        }
    }

    let parents = projects
        .iter()
        .map(|project| {
            let parent = match &project.parent_source_id {
                _ if in_cycle.contains(&project.source_id) => {
                    rooted.push((project.source_id.clone(), RootedParent::Cycle));
                    None
                }
                Some(parent) if !upstream.contains_key(parent) => {
                    rooted.push((project.source_id.clone(), RootedParent::Dangling));
                    None
                }
                parent => parent.clone(),
            };
            (project.source_id.clone(), parent)
        })
        .collect();
    (parents, rooted)
}

/// The markdown note of a context. Universal Inbox owns its body.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Note {
    pub id: NoteId,
    pub context_id: ContextId,
    pub body: String,
    pub backend: NoteBackend,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(tag = "type", content = "content")]
pub enum NoteBackend {
    Local,
    External(NoteBackendBinding),
}

/// Where an external note lives. Opaque to the storage so that adding a backend needs no migration.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
#[serde(try_from = "NoteBackendBindingData")]
pub struct NoteBackendBinding {
    kind: String,
    binding: serde_json::Value,
}

/// Deserialization shape of [`NoteBackendBinding`], validated through [`NoteBackendBinding::new`]
#[derive(Deserialize)]
struct NoteBackendBindingData {
    kind: String,
    binding: serde_json::Value,
}

impl TryFrom<NoteBackendBindingData> for NoteBackendBinding {
    type Error = ContextError;

    fn try_from(data: NoteBackendBindingData) -> Result<Self, Self::Error> {
        NoteBackendBinding::new(data.kind, data.binding)
    }
}

impl NoteBackendBinding {
    pub fn new(kind: String, binding: serde_json::Value) -> Result<Self, ContextError> {
        if kind == NOTE_BACKEND_LOCAL {
            return Err(ContextError::InvalidNoteBackendKind);
        }
        Ok(Self { kind, binding })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn binding(&self) -> &serde_json::Value {
        &self.binding
    }
}

/// Value of `note.backend_kind` for a local note
pub const NOTE_BACKEND_LOCAL: &str = "Local";

/// A link attached to a context or to a task.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub struct Attachment {
    pub id: AttachmentId,
    pub user_id: UserId,
    pub attached_to: (AttachableKind, Uuid),
    pub kind: AttachmentKind,
    pub url: Url,
    pub title: Option<String>,
    pub provider: Option<IntegrationProviderKind>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!)]
    pub enum AttachableKind {
        Context,
        Task,
    }
}

macro_attr! {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Eq, Hash, EnumFromStr!, EnumDisplay!)]
    pub enum AttachmentKind {
        Url,
        ThirdPartyItem,
    }
}

macro_rules! uuid_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Hash, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(uuid: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(uuid)?))
            }
        }
    };
}

uuid_newtype!(ContextId);
uuid_newtype!(NoteId);
uuid_newtype!(AttachmentId);

#[cfg(test)]
mod tests {
    use super::*;

    mod status {
        use super::*;
        use rstest::*;

        const STATUSES: [ContextStatus; 3] = [
            ContextStatus::Active,
            ContextStatus::Archived,
            ContextStatus::Tombstoned,
        ];
        const CHANGES: [ContextStatusChange; 4] = [
            ContextStatusChange::Archive,
            ContextStatusChange::Unarchive,
            ContextStatusChange::Tombstone,
            ContextStatusChange::Restore,
        ];

        #[rstest]
        #[case(
            ContextStatus::Active,
            ContextStatusChange::Archive,
            ContextStatus::Archived
        )]
        #[case(
            ContextStatus::Active,
            ContextStatusChange::Tombstone,
            ContextStatus::Tombstoned
        )]
        #[case(
            ContextStatus::Archived,
            ContextStatusChange::Unarchive,
            ContextStatus::Active
        )]
        #[case(
            ContextStatus::Archived,
            ContextStatusChange::Tombstone,
            ContextStatus::Tombstoned
        )]
        #[case(
            ContextStatus::Tombstoned,
            ContextStatusChange::Restore,
            ContextStatus::Active
        )]
        fn test_valid_transitions(
            #[case] from: ContextStatus,
            #[case] change: ContextStatusChange,
            #[case] expected: ContextStatus,
        ) {
            assert_eq!(from.apply(change), Ok(expected));
        }

        #[test]
        fn test_every_status_is_reachable_and_leavable() {
            for status in STATUSES {
                let reachable = STATUSES
                    .iter()
                    .any(|from| CHANGES.iter().any(|c| from.apply(*c) == Ok(status)));
                let leavable = CHANGES
                    .iter()
                    .any(|c| status.apply(*c).is_ok_and(|to| to != status));
                assert!(reachable, "{status} is not reachable");
                assert!(leavable, "{status} cannot be left");
            }
        }

        #[test]
        fn test_every_other_change_is_rejected() {
            let valid = [
                (ContextStatus::Active, ContextStatusChange::Archive),
                (ContextStatus::Active, ContextStatusChange::Tombstone),
                (ContextStatus::Archived, ContextStatusChange::Unarchive),
                (ContextStatus::Archived, ContextStatusChange::Tombstone),
                (ContextStatus::Tombstoned, ContextStatusChange::Restore),
            ];
            for from in STATUSES {
                for change in CHANGES {
                    if !valid.contains(&(from, change)) {
                        assert_eq!(
                            from.apply(change),
                            Err(ContextError::InvalidStatusChange { from, change })
                        );
                    }
                }
            }
        }
    }

    mod binding {
        use super::*;
        use rstest::*;

        #[rstest]
        #[case(IntegrationProviderKind::Todoist)]
        #[case(IntegrationProviderKind::TickTick)]
        fn test_task_services_can_be_bound(#[case] provider: IntegrationProviderKind) {
            let binding = ContextBinding::new(provider, "6Q4fGPwp957gjxwr".into(), true).unwrap();
            assert_eq!(binding.provider, provider);
            assert_eq!(binding.source_id, ProjectId("6Q4fGPwp957gjxwr".to_string()));
            assert!(binding.mirrored);
        }

        #[rstest]
        #[case(IntegrationProviderKind::Github)]
        #[case(IntegrationProviderKind::Slack)]
        #[case(IntegrationProviderKind::Linear)]
        fn test_other_providers_cannot_be_bound(#[case] provider: IntegrationProviderKind) {
            assert_eq!(
                ContextBinding::new(provider, "1".into(), false),
                Err(ContextError::InvalidBindingProvider(provider))
            );
        }

        #[test]
        fn test_empty_source_id_is_rejected() {
            assert_eq!(
                ContextBinding::new(IntegrationProviderKind::Todoist, "".into(), false),
                Err(ContextError::EmptyBindingSourceId)
            );
        }
    }

    #[test]
    fn test_deserialization_goes_through_the_validated_constructors() {
        assert!(
            serde_json::from_value::<ContextBinding>(
                serde_json::json!({ "provider": "Github", "source_id": "1", "mirrored": true })
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<NoteBackendBinding>(
                serde_json::json!({ "kind": "Local", "binding": {} })
            )
            .is_err()
        );
        let binding =
            ContextBinding::new(IntegrationProviderKind::Todoist, "1".into(), true).unwrap();
        let round_trip: ContextBinding =
            serde_json::from_value(serde_json::to_value(&binding).unwrap()).unwrap();
        assert_eq!(round_trip, binding);
    }

    #[test]
    fn test_external_note_backend_cannot_be_local() {
        assert_eq!(
            NoteBackendBinding::new("Local".to_string(), serde_json::json!({})),
            Err(ContextError::InvalidNoteBackendKind)
        );
        assert!(NoteBackendBinding::new("Obsidian".to_string(), serde_json::json!({})).is_ok());
    }

    mod mirrored_parents {
        use super::*;

        fn project(source_id: &str, parent: Option<&str>) -> MirroredProject {
            MirroredProject {
                source_id: ProjectId(source_id.to_string()),
                name: source_id.to_string(),
                parent_source_id: parent.map(|id| ProjectId(id.to_string())),
                is_archived: false,
            }
        }

        fn parent_of(
            parents: &HashMap<ProjectId, Option<ProjectId>>,
            source_id: &str,
        ) -> Option<String> {
            parents[&ProjectId(source_id.to_string())]
                .as_ref()
                .map(|id| id.0.clone())
        }

        #[test]
        fn test_a_tree_resolves_in_any_order() {
            let mut projects = vec![
                project("A", None),
                project("B", Some("A")),
                project("C", Some("B")),
                project("D", Some("C")),
            ];
            let (parents, rooted) = resolve_mirrored_parents(&projects);
            projects.reverse();
            let (reversed, _) = resolve_mirrored_parents(&projects);

            assert_eq!(parents, reversed);
            assert_eq!(parent_of(&parents, "A"), None);
            assert_eq!(parent_of(&parents, "B"), Some("A".to_string()));
            assert_eq!(parent_of(&parents, "D"), Some("C".to_string()));
            assert!(rooted.is_empty());
        }

        #[test]
        fn test_cycles_and_dangling_parents_go_to_the_root() {
            let projects = vec![
                project("X", Some("Y")),
                project("Y", Some("X")),
                project("S", Some("S")),
                project("Z", Some("X")),
                project("O", Some("gone")),
            ];
            let (parents, mut rooted) = resolve_mirrored_parents(&projects);
            rooted.sort_by(|a, b| a.0.0.cmp(&b.0.0));

            assert_eq!(parent_of(&parents, "X"), None);
            assert_eq!(parent_of(&parents, "Y"), None);
            assert_eq!(parent_of(&parents, "S"), None);
            // Under a cycle member, not in the cycle
            assert_eq!(parent_of(&parents, "Z"), Some("X".to_string()));
            assert_eq!(parent_of(&parents, "O"), None);
            assert_eq!(
                rooted,
                vec![
                    (ProjectId("O".to_string()), RootedParent::Dangling),
                    (ProjectId("S".to_string()), RootedParent::Cycle),
                    (ProjectId("X".to_string()), RootedParent::Cycle),
                    (ProjectId("Y".to_string()), RootedParent::Cycle),
                ]
            );
        }
    }
}
