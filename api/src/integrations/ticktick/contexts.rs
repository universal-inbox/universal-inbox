//! TickTick folders and lists as contexts: the tree mirrored on each sync, and a list created in
//! a folder for a context created under it. The folder endpoints, the list creation and its
//! `groupId` are undocumented upstream: a failing folder read keeps the mirrored tree.

use std::{collections::HashSet, sync::atomic::Ordering};

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use tracing::warn;

use universal_inbox::{
    context::{Context as UiContext, ContextStatus, MirroredProject, UpstreamCreationFailure},
    task::{ProjectId, integrations::ticktick::TICKTICK_INBOX_PROJECT},
    third_party::integrations::ticktick::{
        TICKTICK_FOLDER_SOURCE_ID_PREFIX, TickTickList, TickTickProject, TickTickProjectGroup,
        ticktick_folder_source_id,
    },
    user::UserId,
};

use super::TickTickService;
use crate::{
    integrations::{oauth2::AccessToken, task::ContextMirrorSource},
    observability::attr,
    universal_inbox::{UniversalInboxError, task::context::UpstreamCreationError},
    utils::api::ApiClientError,
};

/// Request body for creating a list in a folder (`POST /open/v1/project`, undocumented for
/// `groupId`)
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TickTickCreateProjectRequest {
    name: String,
    group_id: String,
    kind: &'static str,
    view_mode: &'static str,
}

impl TickTickService {
    /// The user's TickTick folders (`GET /open/v1/project/group`, undocumented)
    pub async fn list_project_groups(
        &self,
        access_token: &AccessToken,
    ) -> Result<Vec<TickTickProjectGroup>, UniversalInboxError> {
        Ok(self
            .build_ticktick_client(access_token)?
            .get(self.endpoint(&["project", "group"])?)
            .await
            .context("Failed to list TickTick folders")?)
    }

    /// Create the TickTick list `name` in the folder bound to `folder_source_id` and return its
    /// id. Sent once, never retried. TickTick's unspecific server error is what it answers once
    /// the plan's list limit is reached: it is told apart, any other failure is not.
    pub async fn create_list_in_folder(
        &self,
        access_token: &AccessToken,
        name: &str,
        folder_source_id: &ProjectId,
    ) -> Result<ProjectId, UpstreamCreationError> {
        let group_id = folder_source_id
            .0
            .strip_prefix(TICKTICK_FOLDER_SOURCE_ID_PREFIX)
            .ok_or_else(|| anyhow!("{folder_source_id} is not a TickTick folder"))
            .map_err(UniversalInboxError::from)?;
        let request = TickTickCreateProjectRequest {
            name: name.to_string(),
            group_id: group_id.to_string(),
            kind: "TASK",
            view_mode: "list",
        };
        match self
            .build_ticktick_client_without_retries(access_token)?
            .post::<TickTickProject, _, _>(self.endpoint(&["project"])?, Some(&request))
            .await
        {
            Ok(project) => Ok(ProjectId(project.id)),
            Err(ApiClientError::NetworkError(err))
                if err.status()
                    == Some(reqwest_middleware::reqwest::StatusCode::INTERNAL_SERVER_ERROR) =>
            {
                Err(UpstreamCreationError {
                    failure: UpstreamCreationFailure::ListLimitReachedMaybe,
                    source: UniversalInboxError::Unexpected(anyhow!(
                        "TickTick could not create a list, its list limit may be reached: {err}"
                    )),
                })
            }
            Err(err) => Err(UniversalInboxError::Unexpected(anyhow!(
                "Failed to create a TickTick list: {err}"
            ))
            .into()),
        }
    }

    /// Delete the TickTick list `source_id`, to undo [`Self::create_list_in_folder`]. Never
    /// retried.
    pub async fn delete_list(
        &self,
        access_token: &AccessToken,
        source_id: &ProjectId,
    ) -> Result<(), UniversalInboxError> {
        self.build_ticktick_client_without_retries(access_token)?
            .delete_no_response(self.endpoint(&["project", &source_id.0])?)
            .await
            .context("Failed to delete a TickTick list")?;
        Ok(())
    }
}

#[async_trait]
impl ContextMirrorSource for TickTickService {
    /// The user's TickTick folders, lists and inbox, bound on the ids TickTick gives them. A
    /// folder holds lists, a list with no folder sits at the root. Empty without a TickTick
    /// access token.
    ///
    /// The folders are read from an undocumented endpoint: when it fails, the `mirrored` folders
    /// stand in for it, so a list keeps its place instead of moving to the root and back on the
    /// next sync. A list whose folder is not known then keeps its current folder.
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    async fn list_mirrored_projects(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        mirrored: &[UiContext],
    ) -> Result<Vec<MirroredProject>, UniversalInboxError> {
        let Some(access_token) = self.optional_access_token(executor, user_id).await? else {
            return Ok(vec![]);
        };

        // Fresh lists: a rename or a new list reaches the contexts on this sync, not when the
        // cache expires. The tasks synced next reuse what this fetch caches.
        self.projects_cache_index.fetch_add(1, Ordering::Relaxed);
        let lists = self.fetch_all_projects(user_id, &access_token).await?;
        let folders = self
            .list_project_groups(&access_token)
            .await
            .inspect_err(|err| {
                warn!(
                    { attr::USER_ID } = user_id.to_string(),
                    "Cannot read the TickTick folders, the mirrored ones keep their place: {err}"
                )
            })
            .ok()
            .map(|groups| {
                groups
                    .into_iter()
                    .map(|group| MirroredProject {
                        source_id: ticktick_folder_source_id(&group.id).into(),
                        name: group.name,
                        parent_source_id: None,
                        is_archived: false,
                        is_inbox: false,
                    })
                    .collect()
            });
        Ok(mirror_ticktick_tree(lists, folders, mirrored))
    }
}

/// The TickTick tree to mirror as contexts: the inbox, the folders, and each list under its
/// folder. `folders`: `None` when TickTick could not list them, the `mirrored` folders stand in
/// for them and a list whose folder is not among them keeps its mirrored folder.
fn mirror_ticktick_tree(
    lists: Vec<TickTickProject>,
    folders: Option<Vec<MirroredProject>>,
    mirrored: &[UiContext],
) -> Vec<MirroredProject> {
    let folders_read = folders.is_some();
    let folders = folders.unwrap_or_else(|| {
        mirrored
            .iter()
            .filter(|context| {
                context.is_ticktick_folder() && context.status != ContextStatus::Tombstoned
            })
            .filter_map(|context| {
                Some(MirroredProject {
                    source_id: context.binding.as_ref()?.source_id.clone(),
                    name: context.name.clone(),
                    parent_source_id: None,
                    is_archived: context.status == ContextStatus::Archived,
                    is_inbox: false,
                })
            })
            .collect()
    });
    let known_folders: HashSet<ProjectId> = folders
        .iter()
        .map(|folder| folder.source_id.clone())
        .collect();
    let mirrored_folder_of = |list_id: &str| {
        let list = mirrored.iter().find(|context| {
            context
                .binding
                .as_ref()
                .is_some_and(|binding| binding.source_id.0 == list_id)
        })?;
        let parent = mirrored
            .iter()
            .find(|context| Some(context.id) == list.parent_id)?;
        parent
            .binding
            .as_ref()
            .map(|binding| binding.source_id.clone())
            .filter(|source_id| parent.is_ticktick_folder() && known_folders.contains(source_id))
    };

    let mut projects = vec![MirroredProject {
        source_id: TickTickList::Inbox.source_id(),
        name: TICKTICK_INBOX_PROJECT.to_string(),
        parent_source_id: None,
        is_archived: false,
        is_inbox: true,
    }];
    let lists = lists.into_iter().map(|list| {
        let folder = list
            .group_id
            .as_deref()
            .map(|group_id| ProjectId(ticktick_folder_source_id(group_id)));
        let parent_source_id = match folder {
            Some(folder) if !folders_read && !known_folders.contains(&folder) => {
                mirrored_folder_of(&list.id)
            }
            folder => folder,
        };
        MirroredProject {
            source_id: list.id.into(),
            name: list.name,
            parent_source_id,
            is_archived: list.closed.unwrap_or(false),
            is_inbox: false,
        }
    });
    projects.extend(folders);
    projects.extend(lists);
    projects
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use universal_inbox::{
        context::{ContextBinding, ContextId, NoteId},
        integration_connection::provider::IntegrationProviderKind,
    };

    fn list(id: &str, name: &str, group_id: Option<&str>) -> TickTickProject {
        TickTickProject {
            id: id.to_string(),
            name: name.to_string(),
            color: None,
            group_id: group_id.map(str::to_string),
            sort_order: None,
            view_mode: None,
            closed: None,
        }
    }

    fn folder(group_id: &str, name: &str) -> MirroredProject {
        MirroredProject {
            source_id: ticktick_folder_source_id(group_id).into(),
            name: name.to_string(),
            parent_source_id: None,
            is_archived: false,
            is_inbox: false,
        }
    }

    fn mirrored(source_id: &str, name: &str, parent: Option<&UiContext>) -> UiContext {
        let now = Utc::now();
        UiContext {
            id: ContextId(Uuid::new_v4()),
            user_id: UserId(Uuid::new_v4()),
            parent_id: parent.map(|parent| parent.id),
            name: name.to_string(),
            binding: Some(
                ContextBinding::new(
                    IntegrationProviderKind::TickTick,
                    source_id.to_string().into(),
                    true,
                )
                .unwrap(),
            ),
            status: ContextStatus::Active,
            is_inbox: false,
            note_id: NoteId(Uuid::new_v4()),
            created_at: now,
            updated_at: now,
        }
    }

    fn placement(tree: &[MirroredProject]) -> Vec<(String, Option<String>, bool)> {
        tree.iter()
            .map(|project| {
                (
                    project.source_id.0.clone(),
                    project.parent_source_id.as_ref().map(|id| id.0.clone()),
                    project.is_archived,
                )
            })
            .collect()
    }

    #[test]
    fn test_folders_hold_their_lists_and_a_list_without_folder_sits_at_the_root() {
        let mut closed = list("l3", "Old", Some("f1"));
        closed.closed = Some(true);
        let tree = mirror_ticktick_tree(
            vec![
                list("l1", "Errands", Some("f1")),
                list("l2", "Universal Inbox", None),
                closed,
            ],
            Some(vec![folder("f1", "Personal")]),
            &[],
        );

        assert_eq!(
            placement(&tree),
            vec![
                ("inbox".to_string(), None, false),
                ("group:f1".to_string(), None, false),
                ("l1".to_string(), Some("group:f1".to_string()), false),
                ("l2".to_string(), None, false),
                ("l3".to_string(), Some("group:f1".to_string()), true),
            ]
        );
        assert!(tree[0].is_inbox);
        assert_eq!(tree[1].name, "Personal");
    }

    #[test]
    fn test_without_folders_the_mirrored_ones_keep_every_list_in_place() {
        let personal = mirrored("group:f1", "Personal", None);
        let work = mirrored("group:f2", "Work", None);
        let errands = mirrored("l1", "Errands", Some(&personal));
        let alan = mirrored("l2", "Alan", Some(&work));
        let tree = mirror_ticktick_tree(
            vec![
                // Its folder is known: placed by the list's own groupId
                list("l1", "Errands", Some("f1")),
                // Moved upstream to a folder not mirrored yet: keeps its mirrored folder
                list("l2", "Alan", Some("f3")),
                // No groupId: the list response says it sits at the root
                list("l4", "Universal Inbox", None),
            ],
            None,
            &[personal, work, errands, alan],
        );

        assert_eq!(
            placement(&tree),
            vec![
                ("inbox".to_string(), None, false),
                ("group:f1".to_string(), None, false),
                ("group:f2".to_string(), None, false),
                ("l1".to_string(), Some("group:f1".to_string()), false),
                ("l2".to_string(), Some("group:f2".to_string()), false),
                ("l4".to_string(), None, false),
            ]
        );
        assert_eq!(tree[1].name, "Personal");
    }

    #[test]
    fn test_without_folders_ever_mirrored_the_lists_are_flat() {
        let tree = mirror_ticktick_tree(vec![list("l1", "Errands", Some("f1"))], None, &[]);

        assert_eq!(
            placement(&tree),
            vec![
                ("inbox".to_string(), None, false),
                ("l1".to_string(), None, false),
            ]
        );
    }
}
