#![allow(non_snake_case)]

use log::{debug, error};

use anyhow::{Context, Result};
use dioxus::prelude::*;
use dioxus_router::prelude::*;
use fermi::UseAtomRef;
use gloo_utils::errors::JsError;
use openidconnect::{
    AccessToken, AuthorizationCode, ClientId, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl,
};
use reqwest::{Client, Method};
use url::Url;

use universal_inbox::auth::{
    openidconnect::OpenidConnectProvider, AuthIdToken, AuthorizeSessionResponse,
    SessionAuthValidationParameters,
};

use crate::{
    components::spinner::Spinner,
    model::{AuthenticationState, UniversalInboxUIModel},
    route::Route,
    services::api::call_api,
    utils::{current_location, get_local_storage, redirect_to},
};

#[derive(Props)]
pub struct AuthenticatedProps<'a> {
    issuer_url: Url,
    client_id: Option<Option<String>>,
    redirect_url: Url,
    api_base_url: Url,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    children: Element<'a>,
}

#[inline_props]
#[allow(unused_variables)]
pub fn AuthPage(cx: Scope, query: String) -> Element {
    render!(div {
        class: "h-full flex justify-center items-center overflow-hidden",

        Spinner {}
        "Authenticating..."
    })
}

pub fn Authenticated<'a>(cx: Scope<'a, AuthenticatedProps<'a>>) -> Element<'a> {
    let ui_model_ref = cx.props.ui_model_ref.clone();
    let error = use_state(cx, || None::<anyhow::Error>);
    let current_url = current_location().unwrap();
    let api_base_url = &cx.props.api_base_url;
    let issuer_url = &cx.props.issuer_url;
    let client_id = &cx.props.client_id;
    let redirect_url = &cx.props.redirect_url;
    // Workaround for Dioxus 0.4.1 bug: https://github.com/DioxusLabs/dioxus/issues/1511
    let local_storage = get_local_storage().unwrap();
    let auth_code = if current_url.path() == redirect_url.path() {
        local_storage
            .get_item("auth-oidc-callback-code")
            .unwrap()
            .and_then(|code| (!code.is_empty()).then_some(code))
    } else {
        None
    };
    // end workaround
    let nav = use_navigator(cx);

    // If we are on the authentication redirection URL with an authentication code,
    // we should exchange it for an access token and authentication state is not unknown anymore
    if auth_code.is_some()
        && ui_model_ref.read().authentication_state == AuthenticationState::Unknown
    {
        ui_model_ref.write_silent().authentication_state = AuthenticationState::FetchingAccessToken;
    }
    let authentication_state = ui_model_ref.read().authentication_state.clone();

    use_future(cx, &authentication_state, |_| {
        to_owned![ui_model_ref];
        to_owned![auth_code];
        to_owned![api_base_url];
        to_owned![issuer_url];
        to_owned![client_id];
        to_owned![redirect_url];
        to_owned![error];

        async move {
            if let Err(auth_error) = authenticate(
                ui_model_ref,
                auth_code,
                &api_base_url,
                &issuer_url,
                client_id,
                &redirect_url,
            )
            .await
            {
                error.set(Some(auth_error));
            }
        }
    });

    if let Some(error) = error.current().as_ref() {
        error!("An error occured while authenticating: {:?}", error);
        return render! {
            "The authentication has failed, please contact the support"
        };
    }

    match authentication_state {
        AuthenticationState::Authenticated
            if current_url.path() == cx.props.redirect_url.path() =>
        {
            debug!("auth: Authenticated, redirecting to /");
            cx.needs_update();
            nav.replace(Route::NotificationsPage {});
            None
        }
        AuthenticationState::Authenticated | AuthenticationState::Unknown => {
            render!(&cx.props.children)
        }
        value => render!(div {
            class: "h-full flex justify-center items-center overflow-hidden",

            Spinner {}
            "{value.label()}"
        }),
    }
}

// - Assert the session is currently authenticated or unknown
// - if not, verify if there is an existing access token in the local storage, and if so, check if it
// - if there is no access token or if invalid, redirect to the auth provider
// - if called on the auth callback URL with a `code` query parameter, fetch the access token and create
// an authenticated session against the API
async fn authenticate(
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    auth_code: Option<String>,
    api_base_url: &Url,
    issuer_url: &Url,
    client_id: Option<Option<String>>,
    redirect_url: &Url,
) -> Result<()> {
    let authentication_state = ui_model_ref.read().authentication_state.clone();
    if authentication_state == AuthenticationState::Authenticated
        || authentication_state == AuthenticationState::Unknown
    {
        return Ok(());
    }

    // Authorization code flow with PKCE (ie. handled by the client)
    if let Some(Some(client_id)) = client_id {
        debug!("auth: Authenticating with Authorization code PKCE flow");
        let oidc_provider = OpenidConnectProvider::build(
            IssuerUrl::new(issuer_url.to_string())?,
            ClientId::new(client_id),
            None,
            RedirectUrl::new(redirect_url.to_string())?,
        )
        .await?;

        if let Some(auth_code) = auth_code {
            // We are on the auth callback URL with a code from the auth provider, so we can fetch the access token
            ui_model_ref.write_silent().authentication_state =
                AuthenticationState::FetchingAccessToken;
            let (access_token, auth_id_token) =
                fetch_access_token(oidc_provider, AuthorizationCode::new(auth_code)).await?;
            create_authenticated_session(
                api_base_url,
                &access_token,
                &auth_id_token,
                ui_model_ref.clone(),
            )
            .await
        } else {
            debug!("auth: Not authenticated, redirecting to login");
            ui_model_ref.write_silent().authentication_state =
                AuthenticationState::RedirectingToAuthProvider;
            // let auth_url = build_auth_url(client).await?.to_string();
            let auth_url = build_auth_url(oidc_provider).await?.to_string();
            debug!("auth: Redirecting to auth provider: {auth_url}");
            redirect_to(&auth_url)
        }
    }
    // Authorization code flow (ie. handled by the server)
    else {
        debug!("auth: Authenticating with Authorization code flow (server flow)");
        debug!("auth: Not authenticated, redirecting to login");
        let auth_url = get_authorization_code_flow_auth_url(api_base_url)
            .await?
            .to_string();
        ui_model_ref.write_silent().authentication_state =
            AuthenticationState::RedirectingToAuthProvider;
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
    };
    let response = Client::new()
        .request(Method::POST, session_url.clone())
        .bearer_auth(access_token.secret())
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
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    debug!("auth: Creating authenticated session");
    ui_model_ref.write_silent().authentication_state = AuthenticationState::VerifyingAccessToken;
    let is_authenticated =
        is_session_authenticated(api_base_url, access_token, auth_id_token).await?;
    ui_model_ref.write().authentication_state = if is_authenticated {
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
