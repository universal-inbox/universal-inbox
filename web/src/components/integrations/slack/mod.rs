#![allow(non_snake_case)]

use dioxus::prelude::*;
use slack_morphism::prelude::*;
use url::Url;

use universal_inbox::third_party::integrations::slack::SlackMessageSenderDetails;

use crate::components::UserWithAvatar;

pub mod config;
pub mod icons;
pub mod notification_list_item;
pub mod preview;
pub mod task_list_item;

#[component]
pub fn SlackMessageActorDisplay(
    sender: ReadOnlySignal<SlackMessageSenderDetails>,
    display_name: Option<bool>,
) -> Element {
    let display_name = display_name.unwrap_or_default();
    let (user_name, avatar_url) = get_sender_name_and_avatar(&sender());

    rsx! { UserWithAvatar { user_name, avatar_url, display_name } }
}

#[component]
pub fn SlackUserDisplay(
    user: ReadOnlySignal<SlackUserProfile>,
    display_name: Option<bool>,
) -> Element {
    let display_name = display_name.unwrap_or_default();
    let (user_name, avatar_url) = get_user_name_and_avatar(&user());

    rsx! { UserWithAvatar { user_name, avatar_url, display_name } }
}

#[component]
pub fn SlackTeamDisplay(team: ReadOnlySignal<SlackTeamInfo>) -> Element {
    let (team_name, avatar_url) = get_team_name_and_avatar(&team());

    rsx! { UserWithAvatar { user_name: team_name, avatar_url } }
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

fn get_user_name_and_avatar(user: &SlackUserProfile) -> (String, Option<Url>) {
    let user_name = match user {
        SlackUserProfile {
            display_name: Some(name),
            ..
        } if !name.is_empty() => name.clone(),
        SlackUserProfile {
            real_name: Some(name),
            ..
        } => name.clone(),
        _ => "Unknown user".to_string(),
    };
    let user_avatar = match &user.icon {
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
    (user_name, user_avatar)
}

fn get_bot_name_and_avatar(bot: &SlackBotInfo) -> (String, Option<Url>) {
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

fn get_sender_name_and_avatar(sender: &SlackMessageSenderDetails) -> (String, Option<Url>) {
    match sender {
        SlackMessageSenderDetails::User(user) => get_user_name_and_avatar(user),
        SlackMessageSenderDetails::Bot(bot) => get_bot_name_and_avatar(bot),
    }
}
