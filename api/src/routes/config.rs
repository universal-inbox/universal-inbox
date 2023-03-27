use actix_web::{
    http::header::{CacheControl, CacheDirective},
    web, HttpResponse,
};
use anyhow::Context;

use universal_inbox::FrontConfig;

use crate::{configuration::Settings, universal_inbox::UniversalInboxError};

pub async fn front_config(
    settings: web::Data<Settings>,
) -> Result<HttpResponse, UniversalInboxError> {
    let config = FrontConfig {
        oidc_issuer_url: settings
            .application
            .authentication
            .oidc_issuer_url
            .url()
            .clone(),
        oidc_client_id: settings
            .application
            .authentication
            .oidc_front_client_id
            .to_string(),
        oidc_redirect_url: settings
            .application
            .front_base_url
            .join("auth-oidc-callback")
            .context("Failed to parse OIDC redirect URL")?,
    };

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .insert_header(CacheControl(vec![
            CacheDirective::Public,
            // Cache only for a few second so that the preload of this config is effective
            CacheDirective::MaxAge(5u32),
        ]))
        .body(serde_json::to_string(&config).context("Cannot serialize front configuration")?))
}
