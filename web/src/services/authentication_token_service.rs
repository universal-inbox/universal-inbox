use anyhow::Result;
use dioxus::prelude::*;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use log::error;
use reqwest::Method;
use url::Url;

use universal_inbox::auth::auth_token::{AuthenticationToken, TruncatedAuthenticationToken};

use crate::{
    model::{LoadState, UniversalInboxUIModel},
    services::{
        api::{call_api, call_api_and_notify},
        toast_service::ToastCommand,
    },
};

#[derive(Debug)]
pub enum AuthenticationTokenCommand {
    Refresh,
    CreateAuthenticationToken,
}

pub static AUTHENTICATION_TOKENS: AtomRef<Option<Vec<TruncatedAuthenticationToken>>> =
    AtomRef(|_| None);
pub static CREATED_AUTHENTICATION_TOKEN: AtomRef<LoadState<AuthenticationToken>> =
    AtomRef(|_| LoadState::None);

pub async fn authentication_token_service<'a>(
    mut rx: UnboundedReceiver<AuthenticationTokenCommand>,
    api_base_url: Url,
    authentication_tokens_ref: UseAtomRef<Option<Vec<TruncatedAuthenticationToken>>>,
    created_authentication_token_ref: UseAtomRef<LoadState<AuthenticationToken>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(AuthenticationTokenCommand::Refresh) => {
                if let Err(error) = refresh_authentication_tokens(
                    &authentication_tokens_ref,
                    &api_base_url,
                    &ui_model_ref,
                )
                .await
                {
                    error!("An error occurred while refreshing authentication tokens: {error:?}");
                }
            }
            Some(AuthenticationTokenCommand::CreateAuthenticationToken) => {
                *created_authentication_token_ref.write() = LoadState::Loading;

                let result: Result<AuthenticationToken> = call_api_and_notify(
                    Method::POST,
                    &api_base_url,
                    "users/me/authentication-tokens",
                    None::<i32>,
                    Some(ui_model_ref.clone()),
                    &toast_service,
                    "Creating API key...",
                    "API key successfully created",
                )
                .await;

                match result {
                    Ok(authentication_token) => {
                        *created_authentication_token_ref.write() =
                            LoadState::Loaded(authentication_token);
                    }
                    Err(error) => {
                        *created_authentication_token_ref.write() =
                            LoadState::Error(error.to_string());
                    }
                }
            }
            None => {}
        }
    }
}

async fn refresh_authentication_tokens(
    authentication_tokens_ref: &UseAtomRef<Option<Vec<TruncatedAuthenticationToken>>>,
    api_base_url: &Url,
    ui_model_ref: &UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    let new_authentication_tokens: Vec<TruncatedAuthenticationToken> = call_api(
        Method::GET,
        api_base_url,
        "users/me/authentication-tokens",
        // random type as we don't care about the body's type
        None::<i32>,
        Some(ui_model_ref.clone()),
    )
    .await?;

    authentication_tokens_ref
        .write()
        .replace(new_authentication_tokens);

    Ok(())
}
