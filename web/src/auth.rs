#![allow(non_snake_case)]

use log::{debug, error};

use anyhow::{Context, Result};
use dioxus::prelude::dioxus_core::needs_update;
use dioxus::prelude::*;
use gloo_utils::errors::JsError;
use openidconnect::{
    AccessToken, AuthorizationCode, ClientId, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl,
};
use reqwest::{Client, Method};
use url::Url;

use universal_inbox::{
    FrontAuthenticationConfig,
    auth::{
        AuthIdToken, AuthorizeSessionResponse, SessionAuthValidationParameters,
        openidconnect::OpenidConnectProvider,
    },
};

use crate::{
    components::loading::Loading,
    model::{AuthenticationState, UI_MODEL},
    route::Route,
    services::{api::call_api, user_service::UserCommand},
    utils::{current_location, get_local_storage, redirect_to},
};

#[component]
#[allow(unused_variables)]
pub fn AuthPage(query: String) -> Element {
    rsx! { Loading { label: "Authenticating..." } }
}

#[component]
pub fn Authenticated(
    authentication_configs: Vec<FrontAuthenticationConfig>,
    api_base_url: Url,
    children: Element,
) -> Element {
    let user_service = use_coroutine_handle::<UserCommand>();
    let mut error = use_signal(|| None::<anyhow::Error>);
    let current_url = current_location().unwrap();
    let nav = use_navigator();
    let oidc_auth_code_pkce_flow_config =
        authentication_configs
            .iter()
            .find_map(|auth_config| match auth_config {
                FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow(config) => {
                    Some(config.clone())
                }
                _ => None,
            });
    let is_oidc_redirect_url = oidc_auth_code_pkce_flow_config
        .as_ref()
        .map(|config| config.oidc_redirect_url.path() == current_url.path());
    // Workaround for Dioxus 0.4.1 bug: https://github.com/DioxusLabs/dioxus/issues/1511
    let auth_code = use_memo(move || {
        let local_storage = get_local_storage().unwrap();
        if is_oidc_redirect_url.unwrap_or_default() {
            let auth_code = local_storage
                .get_item("auth-oidc-callback-code")
                .unwrap()
                .and_then(|code| (!code.is_empty()).then_some(code));
            // If we are on the authentication redirection URL with an authentication code,
            // we should exchange it for an access token and authentication state is not unknown anymore
            if auth_code.is_some()
                && UI_MODEL.peek().authentication_state == AuthenticationState::Unknown
            {
                UI_MODEL.write().authentication_state = AuthenticationState::FetchingAccessToken;
            }
            auth_code
        } else {
            None
        }
    })();
    // end workaround

    let auth_configs = authentication_configs.clone();
    let _resource = use_resource(move || {
        to_owned![auth_code];
        to_owned![auth_configs];
        to_owned![api_base_url];
        to_owned![oidc_auth_code_pkce_flow_config];

        async move {
            let authentication_state = UI_MODEL.read().authentication_state;
            if authentication_state == AuthenticationState::Unknown {
                user_service.send(UserCommand::GetUser);
                debug!("auth: Unknown authentication state, triggering API call");
                return;
            }
            if authentication_state == AuthenticationState::Authenticated {
                return;
            }
            if authentication_state == AuthenticationState::RedirectingToAuthProvider {
                debug!("auth: Already authenticating, skipping authentication");
                return;
            }

            if auth_configs.len() == 1 {
                if let Some(auth_config) = auth_configs.first() {
                    match auth_config {
                        FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow(config) => {
                            if let Err(auth_error) = authenticate_pkce_flow(
                                &api_base_url,
                                auth_code,
                                &config.oidc_issuer_url,
                                &config.oidc_client_id,
                                &config.oidc_redirect_url,
                            )
                            .await
                            {
                                *error.write() = Some(auth_error);
                            }
                        }
                        FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow(_) => {
                            if let Err(auth_error) =
                                authenticate_authorization_code_flow(&api_base_url).await
                            {
                                *error.write() = Some(auth_error);
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                // If OpenIDConnect authorization code PKCE flow is enabled and we have an auth_code
                // we must continue the flow to exchange the auth_code against an access token
                // Not starting the flow here (ie. `auth_code.is_none()`) because it should have been
                // started from the login page
                if let Some(oidc_auth_code_pkce_flow_config) = oidc_auth_code_pkce_flow_config
                    && auth_code.is_some()
                    && let Err(auth_error) = authenticate_pkce_flow(
                        &api_base_url,
                        auth_code,
                        &oidc_auth_code_pkce_flow_config.oidc_issuer_url,
                        &oidc_auth_code_pkce_flow_config.oidc_client_id,
                        &oidc_auth_code_pkce_flow_config.oidc_redirect_url,
                    )
                    .await
                {
                    *error.write() = Some(auth_error);
                }
            }
        }
    });

    if let Some(error) = &*error.read() {
        error!("An error occured while authenticating: {:?}", error);
        return rsx! {
            "The authentication has failed, please contact the support"
        };
    }

    let authentication_state = UI_MODEL.read().authentication_state;
    debug!("auth: Authentication state: {authentication_state:?}");
    match authentication_state {
        AuthenticationState::Authenticated => {
            if is_oidc_redirect_url.unwrap_or_default() {
                debug!("auth: Authenticated, redirecting to /");
                needs_update();
                nav.replace(Route::NotificationsPage {});
                return rsx! {};
            }
            rsx! { { children } }
        }
        AuthenticationState::Unknown => {
            debug!("auth: Unknown authentication state, doing nothing");
            rsx! { Loading { label: "Loading Universal Inbox..." } }
        }
        value => {
            if (authentication_configs.len() == 1
                && authentication_configs.as_slice() == [FrontAuthenticationConfig::Local])
                || authentication_configs.len() > 1
            {
                if history().current_route() != *"/login"
                    && history().current_route() != *"/signup"
                    && history().current_route() != *"/passkey-login"
                    && history().current_route() != *"/passkey-signup"
                    && history().current_route() != *"/password-reset"
                {
                    debug!("auth: Not authenticated, redirecting to the login page");
                    nav.replace(Route::LoginPage {});
                    needs_update();
                    rsx! {}
                } else {
                    debug!("auth: Not authenticated, loading authentication page");
                    rsx! { Outlet::<Route> {} }
                }
            } else {
                debug!("auth: Not authenticated, no authentication page to load");
                rsx! { Loading { label: "{value.label()}" } }
            }
        }
    }
}

pub async fn authenticate_authorization_code_flow(api_base_url: &Url) -> Result<()> {
    debug!("auth: Authenticating with Authorization code flow (server flow)");
    debug!("auth: Not authenticated, redirecting to login");
    let auth_url = get_authorization_code_flow_auth_url(api_base_url)
        .await?
        .to_string();
    UI_MODEL.write().authentication_state = AuthenticationState::RedirectingToAuthProvider;
    debug!("auth: Redirecting to auth provider: {auth_url}");
    redirect_to(&auth_url)
}

// - verify if there is an existing access token in the local storage, and if so, check if it
// - if there is no access token or if invalid, redirect to the auth provider
// - if called on the auth callback URL with a `code` query parameter, fetch the access token and create
// an authenticated session against the API
async fn authenticate_pkce_flow(
    api_base_url: &Url,
    auth_code: Option<String>,
    issuer_url: &Url,
    client_id: &str,
    redirect_url: &Url,
) -> Result<()> {
    debug!("auth: Authenticating with Authorization code PKCE flow");
    let oidc_provider = OpenidConnectProvider::build(
        IssuerUrl::new(issuer_url.to_string())?,
        ClientId::new(client_id.to_string()),
        None,
        RedirectUrl::new(redirect_url.to_string())?,
    )
    .await?;

    if let Some(auth_code) = auth_code {
        // We are on the auth callback URL with a code from the auth provider, so we can fetch the access token
        UI_MODEL.write().authentication_state = AuthenticationState::FetchingAccessToken;
        let (access_token, auth_id_token) =
            fetch_access_token(oidc_provider, AuthorizationCode::new(auth_code)).await?;
        create_authenticated_session(api_base_url, &access_token, &auth_id_token).await
    } else {
        debug!("auth: Not authenticated, redirecting to login");
        UI_MODEL.write().authentication_state = AuthenticationState::RedirectingToAuthProvider;
        // let auth_url = build_auth_url(client).await?.to_string();
        let auth_url = build_auth_url(oidc_provider).await?.to_string();
        debug!("auth: Redirecting to auth provider: {auth_url}");
        redirect_to(&auth_url)
    }
}

/// Generate the PKCE challenge and build an authorization URL (at the auth provider) to redirect the user to
/// Store the PKCE code verifier and the nonce in the local storage for later verification
pub async fn build_auth_url(oidc_provider: OpenidConnectProvider) -> Result<Url> {
    let (pkce_challenge, pkce_code_verifier) = PkceCodeChallenge::new_random_sha256();

    let local_storage = get_local_storage()?;
    local_storage
        .set_item("auth-oidc-pkce-code-verifier", pkce_code_verifier.secret())
        .map_err(|err| JsError::try_from(err).unwrap())?;

    let (auth_url, _csrf_token, nonce) =
        oidc_provider.build_authorization_code_pkce_flow_auth_url(pkce_challenge);

    local_storage
        .set_item("auth-oidc-nonce", nonce.secret())
        .map_err(|err| JsError::try_from(err).unwrap())?;

    Ok(auth_url)
}

/// Fetch the access token from the auth provider using the given code
async fn fetch_access_token(
    oidc_provider: OpenidConnectProvider,
    auth_code: AuthorizationCode,
) -> Result<(AccessToken, AuthIdToken)> {
    let local_storage = get_local_storage()?;

    // Retrieve variables generated before redirecting to the auth provider
    let pkce_code_verifier = PkceCodeVerifier::new(
        local_storage
            .get_item("auth-oidc-pkce-code-verifier")
            .map_err(|err| JsError::try_from(err).unwrap())?
            .context("Unable to retrieve `pkce-verifier` value from local storage")?,
    );
    let nonce = Nonce::new(
        local_storage
            .get_item("auth-oidc-nonce")
            .map_err(|err| JsError::try_from(err).unwrap())?
            .context("Unable to retrieve `nonce` value from local storage")?,
    );

    debug!("auth: Requesting access token to auth provider");
    let (access_token, id_token) = oidc_provider
        .fetch_access_token(auth_code, nonce, Some(pkce_code_verifier))
        .await?;
    debug!("auth: Got a valid access token from auth provider");

    local_storage
        .remove_item("auth-oidc-pkce-code-verifier")
        .map_err(|err| JsError::try_from(err).unwrap())?;
    local_storage
        .remove_item("auth-oidc-nonce")
        .map_err(|err| JsError::try_from(err).unwrap())?;

    Ok((access_token, id_token.to_string().into()))
}

async fn is_session_authenticated(
    api_base_url: &Url,
    access_token: &AccessToken,
    auth_id_token: &AuthIdToken,
) -> Result<bool> {
    let session_url = api_base_url.join("auth/session")?;
    let body = SessionAuthValidationParameters {
        auth_id_token: auth_id_token.clone(),
        access_token: access_token.clone(),
    };
    let response = Client::new()
        .request(Method::POST, session_url.clone())
        .fetch_credentials_include()
        .json(&body)
        .send()
        .await?;
    Ok(response.status().is_success())
}

async fn create_authenticated_session(
    api_base_url: &Url,
    access_token: &AccessToken,
    auth_id_token: &AuthIdToken,
) -> Result<()> {
    debug!("auth: Creating authenticated session");
    UI_MODEL.write().authentication_state = AuthenticationState::VerifyingAccessToken;
    let is_authenticated =
        is_session_authenticated(api_base_url, access_token, auth_id_token).await?;
    UI_MODEL.write().authentication_state = if is_authenticated {
        AuthenticationState::Authenticated
    } else {
        AuthenticationState::NotAuthenticated
    };
    Ok(())
}

async fn get_authorization_code_flow_auth_url(api_base_url: &Url) -> Result<Url> {
    let response: AuthorizeSessionResponse = call_api(
        Method::GET,
        api_base_url,
        "auth/session/authorize",
        None::<i32>,
        None,
    )
    .await?;
    debug!("auth: Got auth URL from server: {:?}", response);
    Ok(response.authorization_url)
}
