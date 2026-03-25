#![allow(non_snake_case)]

use chrono::NaiveDate;
use dioxus::prelude::*;

use universal_inbox::third_party::integrations::linear::{
    LinearNotification, LinearProject, LinearProjectUpdate, LinearProjectUpdateHealthType,
};

use crate::components::{
    MessageHeader, UserWithAvatar,
    integrations::linear::{
        get_notification_type_label,
        icons::{LinearProjectHealtIcon, LinearProjectIcon},
    },
    markdown::Markdown,
    preview_card_header::PreviewCardHeader,
    ui::{Card, CardVariant, MetadataGrid, MetadataItem},
};

/// Compute today's relative position (0..=100) on the start→target timeline.
/// Returns `None` when either bound is missing or the range is degenerate.
fn today_marker_percent(start: Option<NaiveDate>, target: Option<NaiveDate>) -> Option<f64> {
    let (start, target) = match (start, target) {
        (Some(s), Some(t)) if t > s => (s, t),
        _ => return None,
    };
    let today = chrono::Utc::now().date_naive();
    let total = (target - start).num_days() as f64;
    let elapsed = (today - start).num_days() as f64;
    let pct = (elapsed / total) * 100.0;
    Some(pct.clamp(0.0, 100.0))
}

#[component]
pub fn LinearProjectPreview(
    linear_project: ReadSignal<LinearProject>,
    linear_notification: ReadSignal<Option<LinearNotification>>,
    expand_details: ReadSignal<bool>,
) -> Element {
    let _ = expand_details;
    let project_name = linear_project().name.clone();
    let lead = linear_project().lead.clone();
    let title = match linear_project().icon {
        Some(icon) => format!("{icon} {project_name}"),
        None => project_name,
    };

    rsx! {
        div {
            class: "flex flex-col w-full h-full",

            PreviewCardHeader {
                brand_icon: rsx! {
                    LinearProjectIcon { class: "size-4", linear_project }
                },
                title,
                subline: rsx! {
                    span { "Project" }
                    if let Some(lead) = lead {
                        span { class: "sep", "·" }
                        span { "Led by" }
                        UserWithAvatar {
                            user_name: lead.name.clone(),
                            avatar_url: lead.avatar_url.clone(),
                            display_name: true,
                            class: "text-[11px]",
                        }
                    }
                }
            }

            LinearProjectDetails { linear_project, linear_notification }
        }
    }
}

#[component]
pub fn LinearProjectDetails(
    linear_project: ReadSignal<LinearProject>,
    linear_notification: ReadSignal<Option<LinearNotification>>,
    dark_bg: Option<bool>,
) -> Element {
    let prose_style = if dark_bg.unwrap_or_default() {
        "prose-invert!"
    } else {
        ""
    };

    let progress = linear_project().progress.clamp(0, 100);
    let start_date = linear_project().start_date;
    let target_date = linear_project().target_date;
    let today_pct = today_marker_percent(start_date, target_date);

    let reason_label = linear_notification()
        .as_ref()
        .map(|n| get_notification_type_label(&n.get_type()).to_string());

    // Latest update from the notification (only present on project notifications).
    let latest_update = match linear_notification() {
        Some(LinearNotification::ProjectNotification {
            project_update: Some(update),
            ..
        }) => Some(update),
        _ => None,
    };

    rsx! {
        div {
            id: "notification-preview-details",
            class: "flex flex-col gap-2 w-full text-sm h-full overflow-y-auto scroll-y-auto p-3",

            Card {
                variant: CardVariant::Default,
                MetadataGrid {
                    if let Some(reason) = reason_label {
                        MetadataItem {
                            label: "Reason".to_string(),
                            value: rsx! { span { "{reason}" } },
                        }
                    }

                    MetadataItem {
                        label: "Progress".to_string(),
                        value: rsx! {
                            div { class: "preview-progress",
                                div {
                                    class: "preview-progress-fill",
                                    style: "width: {progress}%;",
                                }
                                if let Some(pct) = today_pct {
                                    div {
                                        class: "preview-progress-marker",
                                        style: "left: {pct}%;",
                                        title: "Today",
                                    }
                                }
                            }
                            span { class: "preview-progress-value", "{progress}%" }
                        },
                    }

                    if start_date.is_some() || target_date.is_some() {
                        MetadataItem {
                            label: "Timeline".to_string(),
                            value: rsx! {
                                span { class: "icon-[lucide--calendar] size-4" }
                                if let Some(start) = start_date {
                                    span { "{start}" }
                                } else {
                                    span { class: "preview-progress-placeholder", "—" }
                                }
                                span { class: "icon-[lucide--arrow-right] size-4" }
                                if let Some(target) = target_date {
                                    span { "{target}" }
                                } else {
                                    span { class: "preview-progress-placeholder", "—" }
                                }
                            },
                        }
                    }

                    MetadataItem {
                        label: "Status".to_string(),
                        value: rsx! {
                            LinearProjectIcon { class: "h-4 w-4", linear_project }
                            span { "{linear_project().state}" }
                        },
                    }
                }
            }

            Card {
                variant: CardVariant::Default,
                Markdown {
                    class: "{prose_style} prose prose-sm w-full max-w-full",
                    text: linear_project().description.clone()
                }
            }

            if let Some(project_update) = latest_update {
                LinearProjectUpdateDetails { project_update, dark_bg }
            }
        }
    }
}

#[component]
fn LinearProjectUpdateDetails(
    project_update: ReadSignal<LinearProjectUpdate>,
    dark_bg: Option<bool>,
) -> Element {
    let prose_style = if dark_bg.unwrap_or_default() {
        "prose-invert!"
    } else {
        ""
    };

    let health_icon_style = match project_update().health {
        LinearProjectUpdateHealthType::OnTrack => "text-success",
        LinearProjectUpdateHealthType::AtRisk => "text-warning",
        LinearProjectUpdateHealthType::OffTrack => "text-error",
    };

    rsx! {
        div {
            class: "preview-update-card",

            div {
                class: "preview-update-header",
                LinearProjectHealtIcon { class: "h-3 w-3 {health_icon_style}" }
                span { class: "preview-update-health", "{project_update().health}" }
                span { class: "preview-update-by", "Latest update" }
                MessageHeader {
                    user_name: project_update().user.name.clone(),
                    avatar_url: project_update().user.avatar_url.clone(),
                    display_name: true,
                    sent_at: Some(project_update().updated_at),
                    date_class: "preview-update-date".to_string(),
                }
            }

            Markdown {
                class: "{prose_style} prose prose-sm w-full max-w-full",
                text: project_update().body.clone()
            }
        }
    }
}
