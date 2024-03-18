#![allow(non_snake_case)]

use anyhow::Result;
use dioxus::prelude::*;
use fermi::UseAtomRef;
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
pub fn TaskProjectSearch<'a>(
    cx: Scope,
    class: Option<&'a str>,
    label: Option<&'a str>,
    required: Option<bool>,
    default_project_name: Option<String>,
    selected_project: UseState<Option<ProjectSummary>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    filter_out_inbox: Option<bool>,
    on_select: EventHandler<'a, ProjectSummary>,
) -> Element {
    let filter_out_inbox = filter_out_inbox.unwrap_or_default();
    let search_expression = use_state(cx, || "".to_string());
    let search_results: &UseState<Vec<ProjectSummary>> = use_state(cx, Vec::new);

    let _ = use_memo(
        cx,
        (&search_results.clone(), &default_project_name.clone()),
        |(search_results, default_project_name)| {
            to_owned![selected_project];

            if let Some(default_project_name) = default_project_name {
                if selected_project.is_none() {
                    if let Some(project) = search_results
                        .iter()
                        .find(|project| project.name == *default_project_name)
                    {
                        selected_project.set(Some(project.clone()));
                    }
                }
            }
        },
    );

    use_future(cx, &search_expression.clone(), |search_expression| {
        to_owned![search_results];
        to_owned![ui_model_ref];

        async move {
            let projects =
                search_projects(&search_expression.current(), ui_model_ref, filter_out_inbox).await;
            search_results.set(projects);
        }
    });

    render! {
        FloatingLabelInputSearchSelect {
            name: "project-search-input".to_string(),
            class: "{class.unwrap_or_default()}",
            label: *label,
            required: required.unwrap_or_default(),
            value: selected_project.clone(),
            search_expression: search_expression.clone(),
            search_results: search_results.clone(),
            on_select: move |project| {
                on_select.call(project);
            },
        }
    }
}

async fn search_projects(
    search: &str,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    filter_out_inbox: bool,
) -> Vec<ProjectSummary> {
    let api_base_url = get_api_base_url().unwrap();
    let search_result: Result<Vec<ProjectSummary>> = call_api(
        Method::GET,
        &api_base_url,
        &format!("tasks/projects/search?matches={search}"),
        None::<i32>,
        Some(ui_model_ref),
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
