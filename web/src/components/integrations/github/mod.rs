#![allow(non_snake_case)]

use dioxus::prelude::*;

use universal_inbox::notification::integrations::github::{
    GithubActor, GithubBotSummary, GithubUserSummary,
};

use crate::components::UserWithAvatar;

pub mod config;
pub mod icons;
pub mod notification;
pub mod preview;

#[component]
pub fn GithubActorDisplay<'a>(
    cx: Scope,
    actor: &'a GithubActor,
    without_name: Option<bool>,
) -> Element {
    let (actor_display_name, actor_avatar_url) = match actor {
        GithubActor::User(GithubUserSummary {
            name,
            avatar_url,
            login,
        }) => (name.clone().unwrap_or(login.clone()), avatar_url.clone()),
        GithubActor::Bot(GithubBotSummary {
            login, avatar_url, ..
        }) => (login.clone(), avatar_url.clone()),
    };

    if without_name.unwrap_or_default() {
        render! {
            UserWithAvatar { avatar_url: Some(actor_avatar_url), initials_from: actor_display_name }
        }
    } else {
        render! {
            UserWithAvatar {
                user_name: actor_display_name.clone(),
                avatar_url: Some(actor_avatar_url),
                initials_from: actor_display_name
            }
        }
    }
}
