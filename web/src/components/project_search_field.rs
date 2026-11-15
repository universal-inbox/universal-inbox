//! Async project picker, shared by the task planning modal and every integration config that
//! lets the user pin a "default project" for synced/created tasks.
//!
//! A project is always picked by id, never typed: for Todoist the user picks a context (a
//! mirrored Todoist project, shown with its path), for TickTick a TickTick project. No project is
//! ever created from here.
//!
//! Wraps [`UISearchSelect`] with the `use_resource` + debounce + signal plumbing so call sites
//! only provide the stored value, an `on_change` and the task manager.

#![allow(non_snake_case)]

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use log::error;
use url::Url;

use universal_inbox::{
    context::{CONTEXT_PATH_SEPARATOR, ContextStatus, ContextSummary},
    integration_connection::provider::IntegrationProviderKind,
    task::{DefaultProject, ProjectReference, ProjectSummary, TaskDestination},
};

use crate::components::ui::{
    ContextPath, SEARCH_DEBOUNCE_MS, UISearchSelect, UISelectOption, search_contexts,
    search_ticktick_projects,
};

/// A project the user can pick: a context (Todoist) or a TickTick project, with what it takes
/// to display it. Two choices are equal when they point to the same project.
#[derive(Clone, Debug)]
pub struct ProjectChoice {
    pub reference: ProjectReference,
    pub label: String,
    /// From the root project to this one; empty for a TickTick project
    pub path: Vec<String>,
    pub archived: bool,
}

impl PartialEq for ProjectChoice {
    fn eq(&self, other: &Self) -> bool {
        self.reference == other.reference
    }
}

impl ProjectChoice {
    pub fn from_context(context: &ContextSummary) -> Self {
        Self {
            reference: ProjectReference::Context {
                context_id: context.id,
            },
            label: context.name.clone(),
            path: context.path.clone(),
            archived: context.status == ContextStatus::Archived,
        }
    }

    pub fn from_ticktick_project(project: ProjectSummary) -> Self {
        Self {
            label: project.name.clone(),
            reference: ProjectReference::Project(project),
            path: vec![],
            archived: false,
        }
    }
}

/// Where a task created in or moved to `reference` goes, read for `task_manager` (see
/// [`DefaultProject`]): a context for Todoist, a TickTick project for TickTick. `None` when it
/// cannot be told here: a legacy Todoist default (`{source_id, name}`, migrated after the next
/// Todoist sync) is never sent as a TickTick project.
pub fn reference_destination(
    reference: &ProjectReference,
    task_manager: IntegrationProviderKind,
) -> Option<TaskDestination> {
    DefaultProject::new(reference, task_manager).destination()
}

fn choice_option(choice: ProjectChoice) -> UISelectOption<ProjectChoice> {
    let label = choice.label.clone();
    let search_text = choice.path.join(CONTEXT_PATH_SEPARATOR);
    UISelectOption::new(choice, label).with_search_text(search_text)
}

#[component]
pub fn ProjectSearchField(
    api_base_url: ReadSignal<Url>,
    /// The stored project, as an integration config keeps it
    value: ReadSignal<Option<ProjectReference>>,
    /// The context the task is bound to, when it may not be listed (an archived context is
    /// never listed but still displayed)
    #[props(default)]
    bound_context: ReadSignal<Option<ContextSummary>>,
    provider_kind: ReadSignal<Option<IntegrationProviderKind>>,
    on_change: EventHandler<Option<ProjectChoice>>,
    name: String,
    #[props(default = "Pick a project…".to_string())] placeholder: String,
    #[props(default = false)] disabled: bool,
    #[props(default = true)] allow_clear: bool,
    #[props(default)] width: Option<String>,
    /// Shown under the field, which renders as invalid
    #[props(default)]
    error: Option<String>,
    /// Without a value, pick the Todoist inbox (by its flag) once the projects are loaded
    #[props(default = false)]
    select_inbox_when_empty: bool,
) -> Element {
    let mut query = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut failed = use_signal(|| false);
    let mut options = use_signal(Vec::<UISelectOption<ProjectChoice>>::new);
    // Every listed context, to display the stored value whatever the query
    let mut known_contexts = use_signal(Vec::<ContextSummary>::new);
    let is_ticktick = move || provider_kind() == Some(IntegrationProviderKind::TickTick);

    let _resource = use_resource(move || async move {
        let q = query();
        let ticktick = is_ticktick();
        loading.set(true);
        TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
        if query() != q {
            return;
        }
        let results = if ticktick {
            search_ticktick_projects(&api_base_url(), &q)
                .await
                .map(|projects| {
                    projects
                        .into_iter()
                        .map(ProjectChoice::from_ticktick_project)
                        .collect::<Vec<_>>()
                })
        } else {
            search_contexts(&api_base_url(), &q).await.map(|contexts| {
                if q.is_empty() {
                    known_contexts.set(contexts.clone());
                }
                contexts
                    .iter()
                    .map(ProjectChoice::from_context)
                    .collect::<Vec<_>>()
            })
        };
        match results {
            Ok(choices) => {
                failed.set(false);
                options.set(choices.into_iter().map(choice_option).collect());
            }
            Err(err) => {
                error!("Failed to search projects: {err:?}");
                failed.set(true);
                options.set(Vec::new());
            }
        }
        loading.set(false);
    });

    // The stored value as a choice: found among the listed contexts or the bound one, else
    // nothing (a stale default renders empty, with its error)
    let mut selected = use_signal(|| None::<ProjectChoice>);
    use_effect(move || {
        let choice = match value() {
            Some(ProjectReference::Context { context_id }) => bound_context()
                .filter(|context| context.id == context_id)
                .or_else(|| {
                    known_contexts()
                        .into_iter()
                        .find(|context| context.id == context_id)
                })
                .map(|context| ProjectChoice::from_context(&context)),
            Some(ProjectReference::Project(project)) if is_ticktick() => {
                Some(ProjectChoice::from_ticktick_project(project))
            }
            _ => None,
        };
        selected.set(choice);
    });

    use_effect(move || {
        if select_inbox_when_empty
            && value().is_none()
            && !is_ticktick()
            && let Some(inbox) = known_contexts().iter().find(|context| context.is_inbox)
        {
            on_change.call(Some(ProjectChoice::from_context(inbox)));
        }
    });

    let render_value = use_callback(|opt: UISelectOption<ProjectChoice>| {
        rsx! { ContextPath {
            path: opt.value.path.clone(),
            label: opt.label,
            archived: opt.value.archived,
        } }
    });
    let render_option = use_callback(|(opt, q): (UISelectOption<ProjectChoice>, String)| {
        rsx! { ContextPath { path: opt.value.path.clone(), label: opt.label, query: q } }
    });
    let render_empty = use_callback(move |q: String| {
        if failed() {
            return rsx! { div { "Couldn't load projects. Try again." } };
        }
        if q.is_empty() && !is_ticktick() {
            // A synced account always has its inbox: nothing listed means not synced yet
            return rsx! {
                span { class: "loading loading-spinner loading-sm", "aria-hidden": "true" }
                div { "Syncing your Todoist projects…" }
            };
        }
        if q.is_empty() {
            // The TickTick inbox is not in its project list: none means only the inbox
            return rsx! {
                div { "No TickTick projects yet." }
                div { class: "text-ui-base-muted", "New tasks go to your TickTick inbox." }
            };
        }
        let task_manager = if is_ticktick() { "TickTick" } else { "Todoist" };
        rsx! {
            span { class: "icon-[lucide--search-x] size-5 opacity-50", "aria-hidden": "true" }
            div { "No project matches \"{q}\"." }
            div { class: "text-ui-base-muted",
                "Create it in {task_manager}; it appears here after the next sync."
            }
        }
    });

    // Picking a project clears the error at once (optimistic): the saved config is checked
    // again when it is read back
    let mut picked = use_signal(|| false);
    let error = error.filter(|_| !picked());
    let has_error = error.is_some();
    rsx! {
        div {
            class: "flex flex-col gap-1",
            UISearchSelect::<ProjectChoice> {
                value: selected,
                // The search results only: the saved value is shown in the trigger, never listed
                // (an archived project is never offered), so a query matching nothing shows the
                // "No project matches" message even with a value set
                options: options(),
                value_option: selected().map(choice_option),
                invalid: has_error,
                on_change: move |choice: Option<ProjectChoice>| {
                    picked.set(true);
                    on_change.call(choice);
                },
                on_query: move |q: String| { query.set(q); },
                loading: loading(),
                placeholder,
                search_placeholder: "Search projects…".to_string(),
                render_empty,
                render_value,
                render_option,
                allow_clear,
                disabled,
                width,
                name,
            }
            if let Some(error) = error {
                span { class: "text-[11px] text-ui-error", "{error}" }
            }
        }
    }
}
