#![allow(non_snake_case)]

use dioxus::prelude::*;
use slack_morphism::prelude::*;
use url::Url;

use universal_inbox::notification::integrations::slack::{
    SlackMessageDetails, SlackMessageSenderDetails,
};

use crate::components::UserWithAvatar;

pub mod config;
pub mod icons;
pub mod notification;
pub mod notification_list_item;
pub mod preview;
pub mod task_list_item;

#[component]
pub fn SlackMessageActorDisplay(
    slack_message: ReadOnlySignal<SlackMessageDetails>,
    display_name: Option<bool>,
) -> Element {
    let display_name = display_name.unwrap_or_default();
    let (user_name, user_avatar) = match slack_message().sender {
        SlackMessageSenderDetails::User(user) => get_user_name_and_avatar(&user),
        SlackMessageSenderDetails::Bot(bot) => {
            let avatar_url = bot.icons.as_ref().and_then(|icons| {
                icons.resolutions.iter().find_map(|res| {
                    if res.0 == 24 || res.0 == 34 {
                        res.1.parse::<Url>().ok()
                    } else {
                        None
                    }
                })
            });
            (bot.name.clone(), avatar_url)
        }
    };

    rsx! {
        UserWithAvatar {
            user_name: user_name,
            avatar_url: user_avatar,
            display_name: display_name,
        }
    }
}

#[component]
pub fn SlackUserDisplay(user: ReadOnlySignal<SlackUser>, display_name: Option<bool>) -> Element {
    let display_name = display_name.unwrap_or_default();
    let (user_name, user_avatar) = get_user_name_and_avatar(&user());

    rsx! {
        UserWithAvatar {
            user_name: user_name,
            avatar_url: user_avatar,
            display_name: display_name
        }
    }
}

#[component]
pub fn SlackTeamDisplay(team: ReadOnlySignal<SlackTeamInfo>) -> Element {
    let (team_name, team_avatar) = get_team_name_and_avatar(&team());

    rsx! {
        UserWithAvatar {
            user_name: team_name,
            avatar_url: team_avatar,
        }
    }
}

fn get_team_name_and_avatar(team: &SlackTeamInfo) -> (String, Option<Url>) {
    let team_name = team.name.clone().unwrap_or("Unknown team".to_string());
    let team_avatar = match &team.icon {
        Some(SlackIcon {
            images: Some(SlackIconImages { resolutions }),
            ..
        }) => resolutions.iter().find_map(|res| {
            if res.0 == 24 || res.0 == 32 || res.0 == 34 || res.0 == 44 {
                res.1.parse::<Url>().ok()
            } else {
                None
            }
        }),
        _ => None,
    };
    (team_name, team_avatar)
}

fn get_user_name_and_avatar(user: &SlackUser) -> (String, Option<Url>) {
    let user_name = user.real_name.clone().unwrap_or("Unknown user".to_string());
    let user_avatar = match &user.profile {
        Some(SlackUserProfile {
            icon:
                Some(SlackIcon {
                    images: Some(SlackIconImages { resolutions }),
                    ..
                }),
            ..
        }) => resolutions.iter().find_map(|res| {
            if res.0 == 24 || res.0 == 32 || res.0 == 34 || res.0 == 44 {
                res.1.parse::<Url>().ok()
            } else {
                None
            }
        }),
        _ => None,
    };
    (user_name, user_avatar)
}
