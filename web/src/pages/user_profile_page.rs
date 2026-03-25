#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::debug;

use crate::components::{
    auth_methods_card::AuthMethodsCard, ui::PageHeader, user_profile_card::UserProfileCard,
};

pub fn UserProfilePage() -> Element {
    debug!("Rendering user profile page");

    rsx! {
        div {
            class: "flex-1 overflow-y-auto bg-ui-base-200",

            div {
                class: "px-5 pt-4 pb-10 max-w-3xl mx-auto flex flex-col gap-4 animate-detail-fade",

                PageHeader {
                    title: "Profile".to_string(),
                    subtitle: Some("Update your personal details and the methods you use to sign in.".to_string()),
                }

                UserProfileCard {}

                AuthMethodsCard {}
            }
        }
    }
}
