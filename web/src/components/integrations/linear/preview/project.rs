#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::bs_icons::{BsArrowRight, BsArrowUpRightSquare, BsCalendar2Event},
    Icon,
};

use universal_inbox::notification::integrations::linear::LinearProject;

use crate::components::{
    integrations::linear::icons::LinearProjectIcon, SmallCard, UserWithAvatar,
};

#[component]
pub fn LinearProjectPreview<'a>(cx: Scope, linear_project: &'a LinearProject) -> Element {
    render! {
        div {
            class: "flex flex-col gap-2 w-full",

            h2 {
                class: "flex items-center gap-2 text-lg",

                LinearProjectIcon { class: "h-5 w-5", linear_project: linear_project }

                if let Some(icon) = &linear_project.icon {
                    render! { img { class: "h-5 w-5", src: "{icon}" } }
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

            LinearProjectDetails { linear_project: linear_project }
        }
    }
}

#[component]
pub fn LinearProjectDetails<'a>(
    cx: Scope,
    linear_project: &'a LinearProject,
    card_class: Option<&'a str>,
) -> Element {
    let description = markdown::to_html(&linear_project.description);

    render! {
        div {
            class: "flex flex-col gap-2 w-full",

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

            p {
                class: "w-full prose prose-sm dark:prose-invert",
                dangerous_inner_html: "{description}"
            }
        }
    }
}
