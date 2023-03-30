use anyhow::Result;
use dioxus::prelude::*;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use reqwest::Method;
use url::Url;

use universal_inbox::user::User;

use crate::{model::UniversalInboxUIModel, services::api::call_api};

#[derive(Debug)]
pub enum UserCommand {
    GetUser,
}

pub static CONNECTED_USER: AtomRef<Option<User>> = |_| None;

pub async fn user_service<'a>(
    mut rx: UnboundedReceiver<UserCommand>,
    api_base_url: Url,
    connected_user: UseAtomRef<Option<User>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) {
    loop {
        let msg = rx.next().await;
        if let Some(UserCommand::GetUser) = msg {
            let result: Result<User> = call_api(
                Method::GET,
                &api_base_url,
                "auth/user",
                None::<i32>,
                Some(ui_model_ref.clone()),
            )
            .await;

            if let Ok(user) = result {
                connected_user.write().replace(user);
            };
        }
    }
}
