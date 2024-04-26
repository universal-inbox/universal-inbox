#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::debug;

use crate::components::{
    authentication_tokens_card::AuthenticationTokensCard, user_profile_card::UserProfileCard,
};

pub fn UserProfilePage() -> Element {
    debug!("Rendering user profile page");

    rsx! {
        div {
            class: "h-full mx-auto flex flex-row px-4",

            div {
                class: "flex flex-col h-full w-full overflow-auto scroll-auto gap-4 p-8",

                UserProfileCard {}
                AuthenticationTokensCard {}
            }
        }
    }
}
