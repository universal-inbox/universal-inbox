#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::linear::{LinearLabel, LinearNotification};

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
pub fn LinearNotificationPreview(cx: Scope, linear_notification: LinearNotification) -> Element {
    match linear_notification {
        LinearNotification::IssueNotification { issue, .. } => render! {
            LinearIssuePreview { linear_notification: linear_notification, linear_issue: issue }
        },
        LinearNotification::ProjectNotification { project, .. } => render! {
            LinearProjectPreview { linear_notification: linear_notification, linear_project: project }
        },
    }
}
