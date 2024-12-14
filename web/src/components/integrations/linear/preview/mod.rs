#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::linear::{LinearLabel, LinearNotification};

use crate::components::{
    integrations::linear::preview::{issue::LinearIssuePreview, project::LinearProjectPreview},
    Tag,
};

pub mod issue;
pub mod project;

impl From<LinearLabel> for Tag {
    fn from(linear_label: LinearLabel) -> Self {
        Tag::Colored {
            name: linear_label.name,
            color: linear_label.color.trim_start_matches('#').to_string(),
        }
    }
}

#[component]
pub fn LinearNotificationPreview(
    linear_notification: ReadOnlySignal<LinearNotification>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    match linear_notification() {
        LinearNotification::IssueNotification { issue, .. } => rsx! {
            LinearIssuePreview {
                linear_issue: issue,
                linear_notification: Some(linear_notification()),
                expand_details
            }
        },
        LinearNotification::ProjectNotification { project, .. } => rsx! {
            LinearProjectPreview {
                linear_project: project,
                linear_notification: Some(linear_notification()),
                expand_details,
            }
        },
    }
}
