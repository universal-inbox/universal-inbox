use anyhow::Result;
use dioxus::prelude::*;

use futures_util::StreamExt;
use log::error;
use reqwest::Method;
use url::Url;

use universal_inbox::{SuccessResponse, auth::oauth2::AuthorizedOAuth2Client};

use crate::{
    model::UniversalInboxUIModel,
    services::{
        api::{call_api, call_api_and_notify},
        toast_service::ToastCommand,
    },
};

#[derive(Debug)]
pub enum OAuth2ClientCommand {
    Refresh,
    RevokeClient(String),
}

pub static OAUTH2_AUTHORIZED_CLIENTS: GlobalSignal<Option<Vec<AuthorizedOAuth2Client>>> =
    Signal::global(|| None);

pub async fn oauth2_client_service(
    mut rx: UnboundedReceiver<OAuth2ClientCommand>,
    api_base_url: Url,
    authorized_clients: Signal<Option<Vec<AuthorizedOAuth2Client>>>,
    ui_model: Signal<UniversalInboxUIModel>,
    toast_service: Coroutine<ToastCommand>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(OAuth2ClientCommand::Refresh) => {
                if let Err(error) =
                    refresh_authorized_clients(authorized_clients, &api_base_url, ui_model).await
                {
                    error!(
                        "An error occurred while refreshing authorized OAuth2 clients: {error:?}"
                    );
                }
            }
            Some(OAuth2ClientCommand::RevokeClient(client_id)) => {
                let result: Result<SuccessResponse> = call_api_and_notify(
                    Method::DELETE,
                    &api_base_url,
                    &format!("users/me/oauth2-authorized-clients/{client_id}"),
                    None::<i32>,
                    Some(ui_model),
                    &toast_service,
                    "Revoking OAuth2 client...",
                    "OAuth2 client authorization revoked",
                )
                .await;

                if let Err(error) = result {
                    error!("An error occurred while revoking OAuth2 client: {error:?}");
                } else if let Err(error) =
                    refresh_authorized_clients(authorized_clients, &api_base_url, ui_model).await
                {
                    error!(
                        "An error occurred while refreshing authorized OAuth2 clients after revoke: {error:?}"
                    );
                }
            }
            None => {}
        }
    }
}

async fn refresh_authorized_clients(
    mut authorized_clients: Signal<Option<Vec<AuthorizedOAuth2Client>>>,
    api_base_url: &Url,
    ui_model: Signal<UniversalInboxUIModel>,
) -> Result<()> {
    let new_authorized_clients: Vec<AuthorizedOAuth2Client> = call_api(
        Method::GET,
        api_base_url,
        "users/me/oauth2-authorized-clients",
        None::<i32>,
        Some(ui_model),
    )
    .await?;

    *authorized_clients.write() = Some(new_authorized_clients);

    Ok(())
}
