use anyhow::Result;
use dioxus::prelude::*;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use reqwest::Method;
use url::Url;

use universal_inbox::{
    auth::CloseSessionResponse,
    user::{Credentials, RegisterUserParameters, User},
};

use crate::{model::UniversalInboxUIModel, services::api::call_api, utils::redirect_to};

pub enum UserCommand {
    GetUser,
    RegisterUser(RegisterUserParameters),
    Login(Credentials),
    Logout,
}

pub static CONNECTED_USER: AtomRef<Option<User>> = AtomRef(|_| None);

pub async fn user_service<'a>(
    mut rx: UnboundedReceiver<UserCommand>,
    api_base_url: Url,
    connected_user: UseAtomRef<Option<User>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(UserCommand::GetUser) => {
                let result: Result<User> = call_api(
                    Method::GET,
                    &api_base_url,
                    "users/me",
                    None::<i32>,
                    Some(ui_model_ref.clone()),
                )
                .await;

                if let Ok(user) = result {
                    connected_user.write().replace(user);
                };
            }
            Some(UserCommand::RegisterUser(parameters)) => {
                let result: Result<User> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users",
                    Some(parameters),
                    Some(ui_model_ref.clone()),
                )
                .await;

                match result {
                    Ok(user) => {
                        connected_user.write().replace(user);
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
                    }
                };
            }
            Some(UserCommand::Login(credentials)) => {
                ui_model_ref.write().error_message = None;
                let result: Result<User> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users/me",
                    Some(credentials),
                    Some(ui_model_ref.clone()),
                )
                .await;

                match result {
                    Ok(user) => {
                        connected_user.write().replace(user);
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
                    }
                };
            }
            Some(UserCommand::Logout) => {
                let result: Result<CloseSessionResponse> = call_api(
                    Method::DELETE,
                    &api_base_url,
                    "auth/session",
                    None::<i32>,
                    Some(ui_model_ref.clone()),
                )
                .await;

                if let Ok(CloseSessionResponse { logout_url }) = result {
                    let _ = redirect_to(logout_url.as_str());
                };
            }
            None => {}
        }
    }
}
