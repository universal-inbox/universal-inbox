#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowRight, BsArrowUpRightSquare, BsCalendar2Event},
    Icon,
};

use universal_inbox::third_party::integrations::linear::{
    LinearNotification, LinearProject, LinearProjectUpdate, LinearProjectUpdateHealthType,
};

use crate::components::{
    integrations::linear::{
        get_notification_type_label,
        icons::{LinearProjectHealtIcon, LinearProjectIcon},
    },
    markdown::Markdown,
    CardWithHeaders, CollapseCard, SmallCard, Tag, TagDisplay, UserWithAvatar,
};

#[component]
pub fn LinearProjectPreview(
    linear_project: ReadOnlySignal<LinearProject>,
    linear_notification: ReadOnlySignal<Option<LinearNotification>>,
    expand_details: ReadOnlySignal<bool>,
) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 w-full",

            h2 {
                class: "flex items-center gap-2 text-lg",

                LinearProjectIcon { class: "h-5 w-5", linear_project: linear_project }

                if let Some(icon) = linear_project().icon {
                    span { "{icon}" }
                }

                a {
                    class: "flex items-center",
                    href: "{linear_project().url}",
                    target: "_blank",
                    "{linear_project().name}"
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
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
    let (cards_style, proses_style) = if dark_bg.unwrap_or_default() {
        ("bg-neutral text-neutral-content", "!prose-invert")
    } else {
        ("bg-base-200 text-base-content", "dark:prose-invert")
    };

    rsx! {
        div {
            class: "flex flex-col gap-2 w-full",

            CollapseCard {
                class: "{cards_style}",
                header: rsx! { span { class: "text-gray-400 ", "Description" } },
                opened: expand_details(),
                Markdown {
                    class: "{proses_style} w-full max-w-full",
                    text: linear_project().description.clone()
                }
            }

            if let Some(linear_notification) = linear_notification() {
                SmallCard {
                    card_class: "{cards_style}",
                    span { class: "text-gray-400", "Reason:" }
                    TagDisplay {
                        tag: Into::<Tag>::into(get_notification_type_label(&linear_notification.get_type()))
                    }
                }
            }

            if let Some(lead) = linear_project().lead {
                SmallCard {
                    card_class: "{cards_style}",
                    span { class: "text-gray-400", "Led by" }
                    UserWithAvatar {
                        user_name: lead.name.clone(),
                        avatar_url: lead.avatar_url.clone(),
                        display_name: true,
                    }
                }
            }

            SmallCard {
                card_class: "{cards_style}",
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
                    card_class: "{cards_style}",
                    Icon { class: "h-5 w-5 text-gray-400", icon: BsCalendar2Event }
                    span { "{start_date}" }
                    Icon { class: "h-5 w-5 text-gray-400", icon: BsArrowRight }
                    if let Some(target_date) = linear_project().target_date {
                        Icon { class: "h-5 w-5 text-gray-400", icon: BsCalendar2Event }
                        span { "{target_date}" }
                    }
                }
            }

            if let Some(LinearNotification::ProjectNotification { project_update: Some(project_update), .. }) = linear_notification() {
                LinearProjectUpdateDetails { project_update }
            }
        }
    }
}

#[component]
fn LinearProjectUpdateDetails(project_update: ReadOnlySignal<LinearProjectUpdate>) -> Element {
    let updated_at = project_update()
        .updated_at
        .format("%Y-%m-%d %H:%M")
        .to_string();
    let health_icon_style = match project_update().health {
        LinearProjectUpdateHealthType::OnTrack => "text-success",
        LinearProjectUpdateHealthType::AtRisk => "text-warning",
        LinearProjectUpdateHealthType::OffTrack => "text-error",
    };
    let headers = vec![rsx! {
        div {
            class: "flex flex-row items-center gap-2",
            LinearProjectHealtIcon { class: "h-3 w-3 {health_icon_style}" }
            span { "{project_update().health}" }
            span { class: "text-gray-400", "by" }
            UserWithAvatar {
                user_name: project_update().user.name.clone(),
                avatar_url: project_update().user.avatar_url.clone(),
                display_name: true,
            }
            span { class: "text-gray-400", "on" }
            span { " {updated_at}" }
        }
    }];

    rsx! {
        CardWithHeaders {
            headers: headers,

            Markdown {
                class: "w-full max-w-full",
                text: project_update().body.clone()
            }
        }
    }
}
