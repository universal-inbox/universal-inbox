#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::bs_icons::{BsArrowRight, BsArrowUpRightSquare, BsCalendar2Event},
};

use universal_inbox::third_party::integrations::linear::{
    LinearNotification, LinearProject, LinearProjectUpdate, LinearProjectUpdateHealthType,
};

use crate::components::{
    CardWithHeaders, CollapseCard, MessageHeader, SmallCard, Tag, TagDisplay, UserWithAvatar,
    integrations::linear::{
        get_notification_type_label,
        icons::{LinearProjectHealtIcon, LinearProjectIcon},
    },
    markdown::Markdown,
};

#[component]
pub fn LinearProjectPreview(
    linear_project: ReadOnlySignal<LinearProject>,
    linear_notification: ReadOnlySignal<Option<LinearNotification>>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 w-full h-full",

            h3 {
                class: "flex items-center gap-2 text-base",

                LinearProjectIcon { class: "h-5 w-5", linear_project: linear_project }

                if let Some(icon) = linear_project().icon {
                    span { "{icon}" }
                }

                a {
                    class: "flex items-center",
                    href: "{linear_project().url}",
                    target: "_blank",
                    "{linear_project().name}"
                    Icon { class: "h-5 w-5 min-w-5 text-base-content/50 p-1", icon: BsArrowUpRightSquare }
                }
            }

            LinearProjectDetails { linear_project, linear_notification, expand_details }
        }
    }
}

#[component]
pub fn LinearProjectDetails(
    linear_project: ReadOnlySignal<LinearProject>,
    linear_notification: ReadOnlySignal<Option<LinearNotification>>,
    expand_details: ReadOnlySignal<bool>,
    dark_bg: Option<bool>,
) -> Element {
    let (card_style, header_style, prose_style) = if dark_bg.unwrap_or_default() {
        (
            "bg-neutral text-neutral-content text-sm",
            "text-neutral-content/75",
            "prose-invert!",
        )
    } else {
        (
            "bg-base-200 text-base-content text-sm",
            "text-base-content/50",
            "",
        )
    };

    rsx! {
        div {
            id: "notification-preview-details",
            class: "flex flex-col gap-2 w-full text-sm h-full overflow-y-auto scroll-y-auto",

            CollapseCard {
                id: "linear-project-details",
                class: "{card_style}",
                header: rsx! { span { class: "{header_style}", "Description" } },
                opened: expand_details(),
                Markdown {
                    class: "{prose_style} prose prose-sm w-full max-w-full",
                    text: linear_project().description.clone()
                }
            }

            if let Some(linear_notification) = linear_notification() {
                SmallCard {
                    card_class: "{card_style}",
                    span { class: "{header_style}", "Reason:" }
                    TagDisplay {
                        tag: Into::<Tag>::into(get_notification_type_label(&linear_notification.get_type()))
                    }
                }
            }

            if let Some(lead) = linear_project().lead {
                SmallCard {
                    card_class: "{card_style}",
                    span { class: "{header_style}", "Led by" }
                    UserWithAvatar {
                        user_name: lead.name.clone(),
                        avatar_url: lead.avatar_url.clone(),
                        display_name: true,
                    }
                }
            }

            SmallCard {
                card_class: "{card_style}",
                LinearProjectIcon { class: "h-5 w-5", linear_project }
                "{linear_project().state}",
                if linear_project().progress > 0 {
                    div { class: "grow" }
                    div {
                        class: "h-5 w-5 radial-progress text-primary",
                        style: "--value:{linear_project().progress}; --thickness: 2px;"
                    }
                    "{linear_project().progress}%"
                }
            }

            if let Some(start_date) = linear_project().start_date {
                SmallCard {
                    card_class: "{card_style}",
                    Icon { class: "h-5 w-5 {header_style}", icon: BsCalendar2Event }
                    span { "{start_date}" }
                    Icon { class: "h-5 w-5 {header_style}", icon: BsArrowRight }
                    if let Some(target_date) = linear_project().target_date {
                        Icon { class: "h-5 w-5 {header_style}", icon: BsCalendar2Event }
                        span { "{target_date}" }
                    }
                }
            }

            if let Some(LinearNotification::ProjectNotification { project_update: Some(project_update), .. }) = linear_notification() {
                LinearProjectUpdateDetails { project_update, dark_bg }
            }
        }
    }
}

#[component]
fn LinearProjectUpdateDetails(
    project_update: ReadOnlySignal<LinearProjectUpdate>,
    dark_bg: Option<bool>,
) -> Element {
    let (card_style, header_style, prose_style) = if dark_bg.unwrap_or_default() {
        (
            "bg-base-200 text-base-content text-sm",
            "text-base-content/50",
            "prose-invert!",
        )
    } else {
        (
            "bg-neutral text-neutral-content text-sm",
            "text-neutral-content/75",
            "dark:prose-invert",
        )
    };

    let health_icon_style = match project_update().health {
        LinearProjectUpdateHealthType::OnTrack => "text-success",
        LinearProjectUpdateHealthType::AtRisk => "text-warning",
        LinearProjectUpdateHealthType::OffTrack => "text-error",
    };
    let headers = vec![rsx! {
        div {
            class: "flex flex-row flex-wrap items-center gap-2 {header_style}",
            LinearProjectHealtIcon { class: "h-3 w-3 {health_icon_style}" }
            span { "{project_update().health}" }
            span { class: "text-xs", "by" }
            MessageHeader {
                user_name: project_update().user.name.clone(),
                avatar_url: project_update().user.avatar_url.clone(),
                display_name: true,
                sent_at: project_update().updated_at,
            }
        }
    }];

    rsx! {
        CardWithHeaders {
            card_class: "{card_style}",
            headers: headers,

            Markdown {
                class: "{prose_style} prose prose-sm w-full max-w-full",
                text: project_update().body.clone()
            }
        }
    }
}
