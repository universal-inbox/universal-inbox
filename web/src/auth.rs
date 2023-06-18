use log::{debug, error};

use anyhow::{anyhow, Context, Result};
use dioxus::prelude::*;
use dioxus_router::Redirect;
use fermi::UseAtomRef;
use gloo_utils::errors::JsError;
use openidconnect::{
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest::async_http_client,
    url::Url,
    AccessToken, AccessTokenHash, AdditionalClaims, AuthDisplay, AuthPrompt, AuthorizationCode,
    ClientId, CsrfToken, ErrorResponse, GenderClaim, IssuerUrl, JsonWebKey, JsonWebKeyType,
    JsonWebKeyUse, JweContentEncryptionAlgorithm, JwsSigningAlgorithm, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RevocableToken, TokenIntrospectionResponse, TokenResponse,
    TokenType,
};
use reqwest::{Client, Method};

use universal_inbox::auth::{AuthIdToken, SessionAuthValidationParameters};

use crate::{
    components::spinner::spinner,
    model::{AuthenticationState, UniversalInboxUIModel},
    utils::{current_location, get_local_storage, redirect_to},
};

#[derive(Props)]
pub struct AuthenticatedProps<'a> {
    issuer_url: Url,
    client_id: String,
    redirect_url: Url,
    session_url: Url,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    children: Element<'a>,
}

pub fn authenticated<'a>(cx: Scope<'a, AuthenticatedProps<'a>>) -> Element<'a> {
    let ui_model_ref = cx.props.ui_model_ref.clone();
    let error = use_state(cx, || None::<anyhow::Error>);
    let current_url = current_location().unwrap();
    let session_url = &cx.props.session_url;
    let issuer_url = &cx.props.issuer_url;
    let client_id = &cx.props.client_id;
    let redirect_url = &cx.props.redirect_url;
    let auth_code = (current_url.path() == redirect_url.path())
        .then(|| {
            current_url
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string())
        })
        .flatten();

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
        to_owned![session_url];
        to_owned![issuer_url];
        to_owned![client_id];
        to_owned![redirect_url];
        to_owned![error];

        async move {
            if let Err(auth_error) = authenticate(
                ui_model_ref,
                auth_code,
                &session_url,
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
        return cx.render(rsx!(
            "The authentication has failed, please contact the support"
        ));
    }

    cx.render(match authentication_state {
        AuthenticationState::Authenticated
            if current_url.path() == cx.props.redirect_url.path() =>
        {
            debug!("auth: Authenticated, redirecting to /");
            cx.needs_update();
            rsx!(Redirect { to: "/" })
        }
        AuthenticationState::Authenticated | AuthenticationState::Unknown => {
            rsx!(&cx.props.children)
        }
        value => rsx!(div {
            class: "h-full flex justify-center items-center overflow-hidden",

            self::spinner {}
            "{value.label()}"
        }),
    })
}

// - Assert the session is currently authenticated or unknown
// - if not, verify if there is an existing access token in the local storage, and if so, check if it
// - if there is no access token or if invalid, redirect to the auth provider
// - if called on the auth callback URL with a `code` query parameter, fetch the access token and create
// an authenticated session against the API
async fn authenticate(
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
    auth_code: Option<String>,
    session_url: &Url,
    issuer_url: &Url,
    client_id: String,
    redirect_url: &Url,
) -> Result<()> {
    let authentication_state = ui_model_ref.read().authentication_state.clone();
    if authentication_state == AuthenticationState::Authenticated
        || authentication_state == AuthenticationState::Unknown
    {
        return Ok(());
    }

    // Issuer URL must strictly be equal to the one found in the auth provider
    // metadata. For now clearly the trailing slash added by Url.to_string().
    let issuer_url_string = issuer_url.as_str().trim_end_matches('/').to_string();
    // Use OpenID Connect Discovery to fetch the provider metadata.
    let provider_metadata =
        CoreProviderMetadata::discover_async(IssuerUrl::new(issuer_url_string)?, async_http_client)
            .await?;

    // Create an OpenID Connect client by specifying the client ID
    let client =
        CoreClient::from_provider_metadata(provider_metadata, ClientId::new(client_id), None)
            // Set the URL the user will be redirected to after the authorization process.
            .set_redirect_uri(RedirectUrl::new(redirect_url.to_string())?);

    if let Some(auth_code) = auth_code {
        // We are on the auth callback URL with a code from the auth provider, so we can fetch the access token
        ui_model_ref.write_silent().authentication_state = AuthenticationState::FetchingAccessToken;
        let (access_token, auth_id_token) =
            fetch_access_token(client, AuthorizationCode::new(auth_code)).await?;
        create_authenticated_session(
            session_url,
            &access_token,
            &auth_id_token,
            ui_model_ref.clone(),
        )
        .await
    } else {
        debug!("auth: Not authenticated, redirecting to login");
        ui_model_ref.write_silent().authentication_state =
            AuthenticationState::RedirectingToAuthProvider;
        let auth_url = build_auth_url(client).await?.to_string();
        redirect_to(&auth_url)
    }
}

// Generate the PKCE challenge and build an authorization URL (at the auth provider) to redirect the user to
#[allow(clippy::type_complexity)]
pub async fn build_auth_url<AC, AD, GC, JE, JS, JT, JU, K, P, TE, TR, TT, TIR, RT, TRE>(
    client: openidconnect::Client<AC, AD, GC, JE, JS, JT, JU, K, P, TE, TR, TT, TIR, RT, TRE>,
) -> Result<Url>
where
    AC: AdditionalClaims,
    AD: AuthDisplay,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    P: AuthPrompt,
    TE: ErrorResponse + 'static,
    TR: TokenResponse<AC, GC, JE, JS, JT, TT>,
    TT: TokenType + 'static,
    TIR: TokenIntrospectionResponse<TT>,
    RT: RevocableToken,
    TRE: ErrorResponse + 'static,
{
    let (pkce_challenge, pkce_code_verifier) = PkceCodeChallenge::new_random_sha256();

    let local_storage = get_local_storage()?;
    local_storage
        .set_item("auth-oidc-pkce-code-verifier", pkce_code_verifier.secret())
        .map_err(|err| JsError::try_from(err).unwrap())?;

    let (auth_url, _csrf_token, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        // Set the desired scopes.
        .add_scope(openidconnect::Scope::new("profile".to_string()))
        .add_scope(openidconnect::Scope::new("email".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    local_storage
        .set_item("auth-oidc-nonce", nonce.secret())
        .map_err(|err| JsError::try_from(err).unwrap())?;

    Ok(auth_url)
}

// Fetch the access token from the auth provider using the given code
#[allow(clippy::type_complexity)]
async fn fetch_access_token<AC, AD, GC, JE, JS, JT, JU, K, P, TE, TR, TT, TIR, RT, TRE>(
    client: openidconnect::Client<AC, AD, GC, JE, JS, JT, JU, K, P, TE, TR, TT, TIR, RT, TRE>,
    auth_code: AuthorizationCode,
) -> Result<(AccessToken, AuthIdToken)>
where
    AC: AdditionalClaims,
    AD: AuthDisplay,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    P: AuthPrompt,
    TE: ErrorResponse + 'static + std::marker::Send + std::marker::Sync,
    TR: TokenResponse<AC, GC, JE, JS, JT, TT>,
    TT: TokenType + 'static,
    TIR: TokenIntrospectionResponse<TT>,
    RT: RevocableToken,
    TRE: ErrorResponse + 'static,
{
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
    let token_response = client
        .exchange_code(auth_code)
        .set_pkce_verifier(pkce_code_verifier)
        .request_async(async_http_client)
        .await?;

    // Extract the ID token claims after verifying its authenticity
    let id_token = token_response
        .id_token()
        .context("Server did not return an ID token")?;
    let claims = id_token.claims(
        &client
            .id_token_verifier()
            .set_other_audience_verifier_fn(|_| true),
        &nonce,
    )?;

    // Verify the access token hash to ensure that the access token hasn't been substituted for
    // another user's.
    if let Some(expected_access_token_hash) = claims.access_token_hash() {
        let actual_access_token_hash =
            AccessTokenHash::from_token(token_response.access_token(), &id_token.signing_alg()?)?;
        if actual_access_token_hash != *expected_access_token_hash {
            return Err(anyhow!("Invalid access token: Access token hash mismatch"));
        }
    }
    debug!("auth: Got a valid access token from auth provider");

    local_storage
        .remove_item("auth-oidc-pkce-code-verifier")
        .map_err(|err| JsError::try_from(err).unwrap())?;
    local_storage
        .remove_item("auth-oidc-nonce")
        .map_err(|err| JsError::try_from(err).unwrap())?;

    Ok((
        token_response.access_token().clone(),
        token_response
            .id_token()
            .context("No token ID found")?
            .to_string()
            .into(),
    ))
}

async fn is_session_authenticated(
    session_url: &Url,
    access_token: &AccessToken,
    auth_id_token: &AuthIdToken,
) -> Result<bool> {
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
    session_url: &Url,
    access_token: &AccessToken,
    auth_id_token: &AuthIdToken,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) -> Result<()> {
    debug!("auth: Creating authenticated session");
    ui_model_ref.write_silent().authentication_state = AuthenticationState::VerifyingAccessToken;
    let is_authenticated =
        is_session_authenticated(session_url, access_token, auth_id_token).await?;
    ui_model_ref.write().authentication_state = if is_authenticated {
        AuthenticationState::Authenticated
    } else {
        AuthenticationState::NotAuthenticated
    };
    Ok(())
}
