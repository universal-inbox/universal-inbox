//! Async project picker, shared by every integration config that lets the
//! user pin a "default project" for synced/created tasks.
//!
//! Wraps [`UISearchSelect`] with the `use_resource` + debounce + signal
//! plumbing so call sites only need to provide an `on_change` and (optionally)
//! a `provider_kind` to filter against.

#![allow(non_snake_case)]

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use log::error;
use url::Url;

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind, task::ProjectSummary,
};

use crate::components::ui::{SEARCH_DEBOUNCE_MS, UISearchSelect, UISelectOption, search_projects};

#[component]
pub fn ProjectSearchField(
    api_base_url: ReadSignal<Url>,
    selected_project: Signal<Option<ProjectSummary>>,
    provider_kind: ReadSignal<Option<IntegrationProviderKind>>,
    on_change: EventHandler<Option<ProjectSummary>>,
    name: String,
    #[props(default = "Search a project…".to_string())] placeholder: String,
    #[props(default = false)] disabled: bool,
    #[props(default)] width: Option<String>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut options = use_signal(Vec::<UISelectOption<ProjectSummary>>::new);

    let _resource = use_resource(move || async move {
        let q = query();
        let kind = provider_kind();
        loading.set(true);
        TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
        if query() != q {
            return;
        }
        match search_projects(&api_base_url(), &q, kind).await {
            Ok(results) => options.set(results),
            Err(err) => {
                error!("Failed to search projects: {err:?}");
                options.set(Vec::new());
            }
        }
        loading.set(false);
    });

    // `UISearchSelect` resolves the trigger label by looking up `value` inside
    // `options`. Server-filtered options start empty, so we pin the current
    // selection as a synthetic option to make the saved value render on load.
    let display_options = use_memo(move || {
        let mut opts = options();
        if let Some(sel) = selected_project().as_ref()
            && !opts.iter().any(|o| &o.value == sel)
        {
            let label = sel.name.clone();
            opts.insert(0, UISelectOption::new(sel.clone(), label));
        }
        opts
    });

    rsx! {
        UISearchSelect::<ProjectSummary> {
            value: selected_project,
            options: display_options(),
            on_change: move |project: Option<ProjectSummary>| {
                on_change.call(project);
            },
            on_query: move |q: String| { query.set(q); },
            loading: loading(),
            placeholder,
            search_placeholder: "Type to search projects…".to_string(),
            empty_hint: "Type to search remote projects.".to_string(),
            allow_clear: true,
            disabled,
            width,
            name,
        }
    }
}
