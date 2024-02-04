#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowRight, BsArrowUpRightSquare, BsCalendar2Event},
    Icon,
};

use universal_inbox::notification::integrations::linear::{
    LinearNotification, LinearProject, LinearProjectUpdate, LinearProjectUpdateHealthType,
};

use crate::components::{
    integrations::linear::{
        get_notification_type_label,
        icons::{LinearProjectHealtIcon, LinearProjectIcon},
    },
    CardWithHeaders, SmallCard, Tag, TagDisplay, UserWithAvatar,
};

#[component]
pub fn LinearProjectPreview<'a>(
    cx: Scope,
    linear_notification: &'a LinearNotification,
    linear_project: &'a LinearProject,
) -> Element {
    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            h2 {
                class: "flex items-center gap-2 text-lg",

                LinearProjectIcon { class: "h-5 w-5", linear_project: linear_project }

                if let Some(icon) = &linear_project.icon {
                    render! { span { "{icon}" } }
                }

                a {
                    href: "{linear_project.url}",
                    target: "_blank",
                    "{linear_project.name}"
                }
                a {
                    class: "flex-none",
                    href: "{linear_project.url}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }

            LinearProjectDetails {
                linear_notification: linear_notification,
                linear_project: linear_project
            }
        }
    }
}

#[component]
pub fn LinearProjectDetails<'a>(
    cx: Scope,
    linear_notification: &'a LinearNotification,
    linear_project: &'a LinearProject,
    card_class: Option<&'a str>,
) -> Element {
    let description = markdown::to_html(&linear_project.description);

    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{description}"
            }

            SmallCard {
                card_class: "{card_class.unwrap_or_default()}",
                span { class: "text-gray-400", "Reason:" }
                TagDisplay {
                    tag: Into::<Tag>::into(get_notification_type_label(&linear_notification.get_type()))
                }
            }

            if let Some(lead) = &linear_project.lead {
                render! {
                    SmallCard {
                        card_class: "{card_class.unwrap_or_default()}",
                        span { class: "text-gray-400", "Led by" }
                        UserWithAvatar {
                            user_name: lead.name.clone(),
                            avatar_url: lead.avatar_url.clone(),
                            initials_from: lead.name.clone(),
                        }
                    }
                }
            }

            SmallCard {
                card_class: "{card_class.unwrap_or_default()}",
                LinearProjectIcon { class: "h-5 w-5", linear_project: linear_project }
                "{linear_project.state}",
                if linear_project.progress > 0 {
                    render! {
                        div { class: "grow" }
                        div {
                            class: "h-5 w-5 radial-progress text-primary",
                            style: "--value:{linear_project.progress}; --thickness: 2px;"
                        }
                        "{linear_project.progress}%"
                    }
                }
            }

            if let Some(start_date) = linear_project.start_date {
                render! {
                    SmallCard {
                        card_class: "{card_class.unwrap_or_default()}",
                        Icon { class: "h-5 w-5 text-gray-400", icon: BsCalendar2Event }
                        span { "{start_date}" }
                        Icon { class: "h-5 w-5 text-gray-400", icon: BsArrowRight }
                        if let Some(target_date) = linear_project.target_date {
                            render! {
                                Icon { class: "h-5 w-5 text-gray-400", icon: BsCalendar2Event }
                                span { "{target_date}" }
                            }
                        }
                    }
                }
            }

            if let LinearNotification::ProjectNotification { project_update: Some(project_update), .. } = linear_notification {
                render! {
                    LinearProjectUpdateDetails { project_update: project_update }
                }
            }
        }
    }
}

#[component]
fn LinearProjectUpdateDetails<'a>(cx: Scope, project_update: &'a LinearProjectUpdate) -> Element {
    let update_body = markdown::to_html(&project_update.body);
    let updated_at = project_update
        .updated_at
        .format("%Y-%m-%d %H:%M")
        .to_string();
    let health_icon_style = match project_update.health {
        LinearProjectUpdateHealthType::OnTrack => "text-success",
        LinearProjectUpdateHealthType::AtRisk => "text-warning",
        LinearProjectUpdateHealthType::OffTrack => "text-error",
    };
    let headers = vec![render! {
        div {
            class: "flex flex-row items-center gap-2",
            LinearProjectHealtIcon { class: "h-3 w-3 {health_icon_style}" }
            span { "{project_update.health}" }
            span { class: "text-gray-400", "by" }
            span { "{project_update.user.name}" }
            span { class: "text-gray-400", "on" }
            span { " {updated_at}" }
        }
    }];

    render! {
        CardWithHeaders {
            headers: headers,

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{update_body}"
            }
        }
    }
}
