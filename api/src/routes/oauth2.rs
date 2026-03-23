use std::sync::Arc;

use actix_jwt_authc::Authenticated;
use actix_web::{HttpResponse, web};
use anyhow::Context;
use serde::Deserialize;

use universal_inbox::user::UserId;

use crate::{
    universal_inbox::{UniversalInboxError, oauth2::service::OAuth2Service},
    utils::jwt::Claims,
};

#[derive(Debug, Deserialize)]
pub struct RegisterClientRequest {
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeParams {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub resource: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenParams {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub client_id: String,
    pub refresh_token: Option<String>,
}

pub async fn register(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    body: web::Json<RegisterClientRequest>,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while registering OAuth2 client")?;

    let client = service
        .register_client(
            &mut transaction,
            body.client_name.clone(),
            body.redirect_uris.clone(),
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while registering OAuth2 client")?;

    Ok(HttpResponse::Created()
        .content_type("application/json")
        .body(serde_json::to_string(&client).context("Cannot serialize OAuth2 client")?))
}

pub async fn authorize(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    authenticated: Authenticated<Claims>,
    params: web::Query<AuthorizeParams>,
) -> Result<HttpResponse, UniversalInboxError> {
    if params.response_type != "code" {
        return Err(UniversalInboxError::InvalidInputData {
            source: None,
            user_error: "Only response_type=code is supported".to_string(),
        });
    }

    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;

    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while creating authorization code")?;

    let code = service
        .create_authorization_code(
            &mut transaction,
            &params.client_id,
            user_id,
            &params.redirect_uri,
            params.scope.as_deref(),
            &params.code_challenge,
            &params.code_challenge_method,
            params.resource.as_deref(),
        )
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit while creating authorization code")?;

    let mut redirect_url = url::Url::parse(&params.redirect_uri).map_err(|_| {
        UniversalInboxError::InvalidInputData {
            source: None,
            user_error: format!("Invalid redirect_uri: {}", params.redirect_uri),
        }
    })?;
    redirect_url.query_pairs_mut().append_pair("code", &code);
    if let Some(ref state) = params.state {
        redirect_url.query_pairs_mut().append_pair("state", state);
    }

    Ok(HttpResponse::Found()
        .insert_header(("Location", redirect_url.as_str()))
        .finish())
}

pub async fn token(
    oauth2_service: web::Data<Arc<OAuth2Service>>,
    form: web::Form<TokenParams>,
) -> Result<HttpResponse, UniversalInboxError> {
    let service = oauth2_service.clone();
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while exchanging token")?;

    let token_response = match form.grant_type.as_str() {
        "authorization_code" => {
            let code =
                form.code
                    .as_deref()
                    .ok_or_else(|| UniversalInboxError::InvalidInputData {
                        source: None,
                        user_error: "Missing 'code' parameter for authorization_code grant"
                            .to_string(),
                    })?;
            let redirect_uri = form.redirect_uri.as_deref().ok_or_else(|| {
                UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Missing 'redirect_uri' parameter for authorization_code grant"
                        .to_string(),
                }
            })?;
            let code_verifier = form.code_verifier.as_deref().ok_or_else(|| {
                UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Missing 'code_verifier' parameter for authorization_code grant"
                        .to_string(),
                }
            })?;

            service
                .exchange_code(
                    &mut transaction,
                    code,
                    &form.client_id,
                    redirect_uri,
                    code_verifier,
                )
                .await?
        }
        "refresh_token" => {
            let refresh_token = form.refresh_token.as_deref().ok_or_else(|| {
                UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "Missing 'refresh_token' parameter for refresh_token grant"
                        .to_string(),
                }
            })?;

            service
                .refresh_token(&mut transaction, refresh_token, &form.client_id)
                .await?
        }
        _ => {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Unsupported grant_type: {}", form.grant_type),
            });
        }
    };

    transaction
        .commit()
        .await
        .context("Failed to commit while exchanging token")?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&token_response).context("Cannot serialize token response")?))
}
