#![allow(non_snake_case)]

use dioxus::prelude::*;
use log::debug;

use crate::components::{
    authentication_tokens_card::AuthenticationTokensCard, oauth_clients_card::OAuthClientsCard,
    ui::PageHeader,
};

pub fn SecurityPage() -> Element {
    debug!("Rendering security page");

    rsx! {
        div {
            class: "flex-1 overflow-y-auto bg-ui-base-200",

            div {
                class: "px-5 pt-4 pb-10 max-w-3xl mx-auto flex flex-col gap-4 animate-detail-fade",

                PageHeader {
                    title: "Security".to_string(),
                    subtitle: Some("Review tokens and registered apps with access to your account.".to_string()),
                }

                AuthenticationTokensCard {}

                OAuthClientsCard {}
            }
        }
    }
}
