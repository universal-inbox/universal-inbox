#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::linear::{LinearIssue, LinearProject};

use crate::components::integrations::linear::icons::LinearProjectDefaultIcon;

#[component]
pub fn LinearIssueListItemSubtitle(linear_issue: ReadSignal<LinearIssue>) -> Element {
    let team = linear_issue().team;

    rsx! {
        div {
            class: "flex gap-2 text-xs text-base-content/50",

            if let Some(team_icon) = team.icon {
                span { "{team_icon} {team.name}" }
            } else {
                span { "{team.name}" }
            }

            if let Some(LinearProject { name, icon, .. }) = linear_issue().project {
                div {
                    class: "flex flex-row items-center gap-1",
                    if let Some(project_icon) = icon {
                        span { "{project_icon}" }
                    } else {
                        LinearProjectDefaultIcon { class: "w-3 h-3" }
                    }
                    span { "{name} #{linear_issue().identifier}" }
                }
            } else {
                span { "#{linear_issue().identifier}" }
            }
        }
    }
}
