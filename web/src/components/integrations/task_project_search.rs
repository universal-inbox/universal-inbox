#![allow(non_snake_case)]

use anyhow::Result;
use dioxus::prelude::*;

use http::Method;
use log::error;

use universal_inbox::task::{integrations::todoist::TODOIST_INBOX_PROJECT, ProjectSummary};

use crate::{
    components::floating_label_inputs::{FloatingLabelInputSearchSelect, Searchable},
    config::get_api_base_url,
    model::UniversalInboxUIModel,
    services::api::call_api,
};

#[component]
pub fn TaskProjectSearch(
    class: Option<String>,
    label: Option<String>,
    required: Option<bool>,
    disabled: Option<bool>,
    default_project_name: ReadOnlySignal<Option<String>>,
    selected_project: Signal<Option<ProjectSummary>>,
    ui_model: Signal<UniversalInboxUIModel>,
    filter_out_inbox: Option<bool>,
    on_select: EventHandler<ProjectSummary>,
) -> Element {
    let filter_out_inbox = filter_out_inbox.unwrap_or_default();
    let search_expression = use_signal(|| "".to_string());
    let mut search_results: Signal<Vec<ProjectSummary>> = use_signal(Vec::new);

    let _ = use_memo(move || {
        if search_results().is_empty() {
            return;
        }
        if let Some(default_project_name) = default_project_name() {
            *selected_project.write() = search_results()
                .iter()
                .find(|project| project.name == *default_project_name)
                .cloned();
        }
    });

    let _ = use_resource(move || async move {
        if ui_model.read().is_task_actions_enabled {
            let projects = search_projects(&search_expression(), ui_model, filter_out_inbox).await;
            *search_results.write() = projects;
        }
    });

    rsx! {
        FloatingLabelInputSearchSelect {
            name: "project-search-input".to_string(),
            class: class.unwrap_or_default(),
            label: label,
            required: required.unwrap_or_default(),
            disabled: disabled.unwrap_or_default(),
            value: selected_project,
            search_expression: search_expression,
            search_results: search_results,
            on_select: move |project| {
                on_select.call(project);
            },
        }
    }
}

async fn search_projects(
    search: &str,
    ui_model: Signal<UniversalInboxUIModel>,
    filter_out_inbox: bool,
) -> Vec<ProjectSummary> {
    let api_base_url = get_api_base_url().unwrap();
    let search_result: Result<Vec<ProjectSummary>> = call_api(
        Method::GET,
        &api_base_url,
        &format!("tasks/projects/search?matches={search}"),
        None::<i32>,
        Some(ui_model),
    )
    .await;

    match search_result {
        Ok(projects) => {
            if filter_out_inbox {
                projects
                    .into_iter()
                    .filter(|p| p.name != TODOIST_INBOX_PROJECT)
                    .collect()
            } else {
                projects
            }
        }
        Err(error) => {
            error!("Error searching projects: {error:?}");
            Vec::new()
        }
    }
}

impl Searchable for ProjectSummary {
    fn get_title(&self) -> String {
        self.name.clone()
    }

    fn get_id(&self) -> String {
        self.source_id.clone()
    }
}
