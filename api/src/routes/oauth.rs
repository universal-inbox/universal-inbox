use std::sync::Arc;

use actix_jwt_authc::Authenticated;
use actix_web::{HttpResponse, Scope, web};
use anyhow::Context;
use secrecy::SecretBox;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::error;

use universal_inbox::{integration_connection::IntegrationConnectionId, user::UserId};

use crate::{
    configuration::Settings,
    integrations::oauth2::AuthorizationCode,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
    },
    utils::{cache::Cache, jwt::Claims},
};

pub fn authorize_scope() -> Scope {
    web::scope("/oauth").service(
        web::resource("/authorize/{integration_connection_id}")
            .route(web::get().to(oauth_authorize)),
    )
}

#[derive(Deserialize)]
pub struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn oauth_authorize(
    path: web::Path<IntegrationConnectionId>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    cache: web::Data<Cache>,
    authenticated: Authenticated<Claims>,
) -> Result<HttpResponse, UniversalInboxError> {
    let user_id = authenticated
        .claims
        .sub
        .parse::<UserId>()
        .context("Wrong user ID format")?;
    let integration_connection_id = path.into_inner();

    let service = integration_connection_service.read().await;
    let mut transaction = service
        .begin()
        .await
        .context("Failed to create new transaction while starting OAuth authorization")?;

    let authorization_url = service
        .start_oauth_authorization(&mut transaction, integration_connection_id, user_id, &cache)
        .await?;

    transaction
        .commit()
        .await
        .context("Failed to commit OAuth authorization transaction")?;

    Ok(HttpResponse::Found()
        .insert_header(("Location", authorization_url.as_str()))
        .finish())
}

pub async fn oauth_callback(
    query: web::Query<OAuthCallbackQuery>,
    integration_connection_service: web::Data<Arc<RwLock<IntegrationConnectionService>>>,
    cache: web::Data<Cache>,
    settings: web::Data<Settings>,
) -> HttpResponse {
    let front_base_url = settings
        .application
        .front_base_url
        .as_str()
        .trim_end_matches('/');

    if let Some(ref error) = query.error {
        return build_error_redirect(front_base_url, error);
    }

    let Some(ref code) = query.code else {
        return build_error_redirect(front_base_url, "missing_code");
    };

    let Some(ref state) = query.state else {
        return build_error_redirect(front_base_url, "missing_state");
    };

    let service = integration_connection_service.read().await;
    let transaction = service.begin().await;
    let mut transaction = match transaction {
        Ok(tx) => tx,
        Err(err) => {
            error!("Failed to create transaction for OAuth callback: {err:?}");
            return build_error_redirect(front_base_url, &format!("{err}"));
        }
    };

    match service
        .complete_oauth_callback(
            &mut transaction,
            &SecretBox::new(Box::new(AuthorizationCode(code.to_string()))),
            state,
            &cache,
        )
        .await
    {
        Ok(()) => {
            if let Err(err) = transaction
                .commit()
                .await
                .context("Failed to commit OAuth callback transaction")
            {
                error!("OAuth callback commit error: {err:?}");
                return build_error_redirect(front_base_url, &format!("{err}"));
            }
            build_success_redirect(front_base_url)
        }
        Err(err) => {
            error!("OAuth callback error: {err:?}");
            build_error_redirect(front_base_url, &format!("{err}"))
        }
    }
}

fn build_success_redirect(front_base_url: &str) -> HttpResponse {
    let redirect_url = format!("{front_base_url}/settings?oauth_success=true");
    HttpResponse::Found()
        .insert_header(("Location", redirect_url.as_str()))
        .finish()
}

fn build_error_redirect(front_base_url: &str, error: &str) -> HttpResponse {
    let encoded_error = urlencoding::encode(error);
    let redirect_url = format!("{front_base_url}/settings?oauth_error={encoded_error}");
    HttpResponse::Found()
        .insert_header(("Location", redirect_url.as_str()))
        .finish()
}
