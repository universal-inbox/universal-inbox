use actix_identity::Identity;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Scope};
use anyhow::{anyhow, Context};
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata},
    reqwest::async_http_client,
    AccessToken, TokenIntrospectionResponse,
};
use tracing::error;

use crate::{
    configuration::Settings, routes::option_wildcard, universal_inbox::UniversalInboxError,
};

pub fn scope() -> Scope {
    web::scope("/auth").service(
        web::resource("session")
            .route(web::get().to(authenticate_session))
            .route(web::method(http::Method::OPTIONS).to(option_wildcard)),
    )
}

pub async fn authenticate_session(
    request: HttpRequest,
    settings: web::Data<Settings>,
) -> Result<HttpResponse, UniversalInboxError> {
    let bearer_access_token = request
        .headers()
        .get("Authorization")
        .context("Missing `Authorization` request header")?
        .to_str()
        .context("Failed to convert `Authorization` request header to a string")?
        .split(' ')
        .nth(1)
        .context("Failed to extract the access token from the `Authorization` request header")?;

    let provider_metadata = CoreProviderMetadata::discover_async(
        settings.application.authentication.oidc_issuer_url.clone(),
        async_http_client,
    )
    .await
    .context("metadata provider error")?;

    // Create an OpenID Connect client by specifying the client ID
    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        settings
            .application
            .authentication
            .oidc_api_client_id
            .clone(),
        Some(
            settings
                .application
                .authentication
                .oidc_api_client_secret
                .clone(),
        ),
    )
    .set_introspection_uri(
        settings
            .application
            .authentication
            .oidc_introspection_url
            .clone(),
    );

    let access_token = AccessToken::new(bearer_access_token.to_string());
    let introspection_result = client
        .introspect(&access_token)
        .context("Introspection configuration error")?
        .set_token_type_hint("access_token")
        .request_async(async_http_client)
        .await
        .context("Introspection request error")?;

    if !introspection_result.active() {
        error!("Given access token is not active");
        return Ok(HttpResponse::Unauthorized().finish());
    }

    let auth_user_id = introspection_result
        .sub()
        .ok_or_else(|| anyhow!("No subject found in introspection result"))?;
    Identity::login(&request.extensions(), auth_user_id.to_string())?;

    Ok(HttpResponse::Ok().finish())
}
