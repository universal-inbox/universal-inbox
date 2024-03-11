#![allow(non_snake_case)]

use dioxus::prelude::*;
use dioxus_free_icons::{icons::bs_icons::BsArrowUpRightSquare, Icon};

use universal_inbox::{
    notification::{
        integrations::slack::SlackGroupDetails, NotificationMetadata, NotificationWithTask,
    },
    HasHtmlUrl,
};

use crate::components::integrations::slack::{icons::SlackNotificationIcon, SlackTeamDisplay};

#[component]
pub fn SlackGroupPreview<'a>(
    cx: Scope,
    notification: &'a NotificationWithTask,
    slack_group: &'a SlackGroupDetails,
) -> Element {
    let NotificationMetadata::Slack(ref slack_push_event_callback) = notification.metadata else {
        return None;
    };
    let channel_name = slack_group
        .channel
        .name
        .clone()
        .unwrap_or_else(|| slack_group.channel.id.to_string());

    render! {
        div {
            class: "flex flex-col w-full gap-2",

            div {
                class: "flex items-center gap-2",

                SlackTeamDisplay { team: &slack_group.team }
                a {
                    class: "text-xs text-gray-400",
                    href: "{slack_group.get_html_url()}",
                    target: "_blank",
                    "#{channel_name}"
                }
            }

            h2 {
                class: "flex items-center gap-2 text-lg",

                SlackNotificationIcon { class: "h-5 w-5", slack_push_event_callback: &slack_push_event_callback }
                a {
                    href: "{slack_group.get_html_url()}",
                    target: "_blank",
                    dangerous_inner_html: "{notification.title}"
                }
                a {
                    class: "flex-none",
                    href: "{slack_group.get_html_url()}",
                    target: "_blank",
                    Icon { class: "h-5 w-5 text-gray-400 p-1", icon: BsArrowUpRightSquare }
                }
            }
        }
    }
}
