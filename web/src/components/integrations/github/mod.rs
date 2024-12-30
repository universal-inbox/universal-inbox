#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::third_party::integrations::github::{
    GithubActor, GithubBotSummary, GithubUserSummary,
};

use crate::components::UserWithAvatar;

pub mod config;
pub mod icons;
pub mod notification_list_item;
pub mod preview;

#[component]
pub fn GithubActorDisplay(
    actor: ReadOnlySignal<GithubActor>,
    display_name: Option<bool>,
) -> Element {
    let display_name = display_name.unwrap_or_default();
    let (actor_display_name, actor_avatar_url) = match actor() {
        GithubActor::User(GithubUserSummary {
            name,
            avatar_url,
            login,
        }) => (name.clone().unwrap_or(login.clone()), avatar_url.clone()),
        GithubActor::Bot(GithubBotSummary {
            login, avatar_url, ..
        }) => (login.clone(), avatar_url.clone()),
    };

    rsx! {
        UserWithAvatar {
            user_name: actor_display_name.clone(),
            avatar_url: Some(actor_avatar_url),
            display_name: display_name,
        }
    }
}
