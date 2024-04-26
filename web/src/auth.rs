#![allow(non_snake_case)]

use log::{debug, error};

use anyhow::{Context, Result};
use dioxus::prelude::*;

use gloo_utils::errors::JsError;
use openidconnect::{
    AccessToken, AuthorizationCode, ClientId, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl,
};
use reqwest::{Client, Method};
use url::Url;

use universal_inbox::{
    auth::{
        openidconnect::OpenidConnectProvider, AuthIdToken, AuthorizeSessionResponse,
        SessionAuthValidationParameters,
    },
    FrontAuthenticationConfig,
};

use crate::{
    components::spinner::Spinner,
    model::{AuthenticationState, UniversalInboxUIModel},
    route::Route,
    services::api::call_api,
    utils::{current_location, get_local_storage, redirect_to},
};

#[component]
#[allow(unused_variables)]
pub fn AuthPage(query: String) -> Element {
    rsx! {
        div {
            class: "h-full flex justify-center items-center overflow-hidden",

            Spinner {}
            "Authenticating..."
        }
    }
}

#[component]
pub fn Authenticated(
    authentication_config: FrontAuthenticationConfig,
    api_base_url: Url,
    mut ui_model: Signal<UniversalInboxUIModel>,
    children: Element,
) -> Element {
    let mut error = use_signal(|| None::<anyhow::Error>);
    let current_url = current_location().unwrap();
    let nav = use_navigator();
    let history = WebHistory::<Route>::default();
    // Workaround for Dioxus 0.4.1 bug: https://github.com/DioxusLabs/dioxus/issues/1511
    let local_storage = get_local_storage().unwrap();
    let auth_code = if let FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow {
        oidc_redirect_url,
        ..
    } = &authentication_config
    {
        if current_url.path() == oidc_redirect_url.path() {
            local_storage
                .get_item("auth-oidc-callback-code")
                .unwrap()
                .and_then(|code| (!code.is_empty()).then_some(code))
        } else {
            None
        }
    } else {
        None
    };
    // end workaround

    // If we are on the authentication redirection URL with an authentication code,
    // we should exchange it for an access token and authentication state is not unknown anymore
    if auth_code.is_some() && ui_model.read().authentication_state == AuthenticationState::Unknown {
        ui_model.write().authentication_state = AuthenticationState::FetchingAccessToken;
    }
    let authentication_state = ui_model.read().authentication_state.clone();

    let auth_config = authentication_config.clone();
    let _ = use_resource(move || {
        to_owned![auth_code];
        to_owned![auth_config];
        to_owned![api_base_url];

        async move {
            let authentication_state = ui_model.read().authentication_state.clone();
            if authentication_state == AuthenticationState::Authenticated
                || authentication_state == AuthenticationState::Unknown
            {
                debug!("auth: Already authenticated or unknown, skipping authentication");
                return;
            }

            match auth_config {
                FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow {
                    oidc_issuer_url,
                    oidc_client_id,
                    oidc_redirect_url,
                    ..
                } => {
                    if let Err(auth_error) = authenticate_pkce_flow(
                        ui_model,
                        &api_base_url,
                        auth_code,
                        &oidc_issuer_url,
                        &oidc_client_id,
                        &oidc_redirect_url,
                    )
                    .await
                    {
                        *error.write() = Some(auth_error);
                    }
                }
                FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow { .. } => {
                    if let Err(auth_error) =
                        authenticate_authorization_code_flow(ui_model, &api_base_url).await
                    {
                        *error.write() = Some(auth_error);
                    }
                }
                FrontAuthenticationConfig::Local => {}
            }
        }
    });

    if let Some(error) = &*error.read() {
        error!("An error occured while authenticating: {:?}", error);
        return rsx! {
            "The authentication has failed, please contact the support"
        };
    }

    debug!("auth: Authentication state: {authentication_state:?}");
    match authentication_state {
        AuthenticationState::Authenticated => {
            if let FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow {
                oidc_redirect_url,
                ..
            } = &authentication_config
            {
                if current_url.path() == oidc_redirect_url.path() {
                    debug!("auth: Authenticated, redirecting to /");
                    needs_update();
                    nav.replace(Route::NotificationsPage {});
                    return None;
                }
            }
            rsx! { { children } }
        }
        AuthenticationState::Unknown => {
            rsx! { { children } }
        }
        value => {
            if authentication_config == FrontAuthenticationConfig::Local {
                if history.current_route() != (Route::LoginPage {})
                    && history.current_route() != (Route::SignupPage {})
                    && history.current_route() != (Route::PasswordResetPage {})
                {
                    nav.replace(Route::LoginPage {});
                    needs_update();
                    None
                } else {
                    rsx! { Outlet::<Route> {} }
                }
            } else {
                rsx! {
                    div {
                        class: "h-full flex justify-center items-center overflow-hidden",

                        Spinner {}
                        "{value.label()}"
                    }
                }
            }
        }
    }
}

async fn authenticate_authorization_code_flow(
    mut ui_model: Signal<UniversalInboxUIModel>,
    api_base_url: &Url,
) -> Result<()> {
    debug!("auth: Authenticating with Authorization code flow (server flow)");
    debug!("auth: Not authenticated, redirecting to login");
    let auth_url = get_authorization_code_flow_auth_url(api_base_url)
        .await?
        .to_string();
    ui_model.write().authentication_state = AuthenticationState::RedirectingToAuthProvider;
    debug!("auth: Redirecting to auth provider: {auth_url}");
    redirect_to(&auth_url)
}

// - verify if there is an existing access token in the local storage, and if so, check if it
// - if there is no access token or if invalid, redirect to the auth provider
// - if called on the auth callback URL with a `code` query parameter, fetch the access token and create
// an authenticated session against the API
async fn authenticate_pkce_flow(
    mut ui_model: Signal<UniversalInboxUIModel>,
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
        ui_model.write().authentication_state = AuthenticationState::FetchingAccessToken;
        let (access_token, auth_id_token) =
            fetch_access_token(oidc_provider, AuthorizationCode::new(auth_code)).await?;
        create_authenticated_session(api_base_url, &access_token, &auth_id_token, ui_model).await
    } else {
        debug!("auth: Not authenticated, redirecting to login");
        ui_model.write().authentication_state = AuthenticationState::RedirectingToAuthProvider;
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
    mut ui_model: Signal<UniversalInboxUIModel>,
) -> Result<()> {
    debug!("auth: Creating authenticated session");
    ui_model.write().authentication_state = AuthenticationState::VerifyingAccessToken;
    let is_authenticated =
        is_session_authenticated(api_base_url, access_token, auth_id_token).await?;
    ui_model.write().authentication_state = if is_authenticated {
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
