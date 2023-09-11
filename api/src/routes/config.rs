use std::collections::HashMap;

use actix_web::{
    http::header::{CacheControl, CacheDirective},
    web, HttpResponse,
};
use anyhow::Context;

use universal_inbox::{
    integration_connection::IntegrationProviderKind, FrontConfig, IntegrationProviderConfig,
};

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
        user_profile_url: settings.application.authentication.user_profile_url.clone(),
        nango_base_url: settings.integrations.oauth2.nango_base_url.clone(),
        integration_providers: HashMap::from([
            (
                IntegrationProviderKind::Github,
                IntegrationProviderConfig {
                    name: settings.integrations.github.name.clone(),
                    nango_config_key: settings
                        .integrations
                        .oauth2
                        .nango_provider_keys
                        .get(&IntegrationProviderKind::Github)
                        .context("Missing Nango config key for Github")?
                        .clone(),
                    comment: settings.integrations.github.comment.clone(),
                    is_implemented: true,
                },
            ),
            (
                IntegrationProviderKind::Linear,
                IntegrationProviderConfig {
                    name: settings.integrations.linear.name.clone(),
                    nango_config_key: settings
                        .integrations
                        .oauth2
                        .nango_provider_keys
                        .get(&IntegrationProviderKind::Linear)
                        .context("Missing Nango config key for Linear")?
                        .clone(),
                    comment: settings.integrations.linear.comment.clone(),
                    is_implemented: true,
                },
            ),
            (
                IntegrationProviderKind::Notion,
                IntegrationProviderConfig {
                    name: "Notion".to_string(),
                    nango_config_key: "notion".to_string().into(),
                    comment: None,
                    is_implemented: false,
                },
            ),
            (
                IntegrationProviderKind::Todoist,
                IntegrationProviderConfig {
                    name: settings.integrations.todoist.name.clone(),
                    nango_config_key: settings
                        .integrations
                        .oauth2
                        .nango_provider_keys
                        .get(&IntegrationProviderKind::Todoist)
                        .context("Missing Nango config key for Todoist")?
                        .clone(),
                    comment: settings.integrations.todoist.comment.clone(),
                    is_implemented: true,
                },
            ),
        ]),
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
