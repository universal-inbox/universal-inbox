use std::collections::HashMap;

use actix_web::{
    http::header::{CacheControl, CacheDirective},
    web, HttpResponse,
};
use anyhow::Context;

use universal_inbox::{FrontAuthenticationConfig, FrontConfig, IntegrationProviderStaticConfig};

use crate::{
    configuration::{
        AuthenticationSettings, OIDCAuthorizationCodePKCEFlowSettings, OIDCFlowSettings,
        OpenIDConnectSettings, Settings,
    },
    universal_inbox::UniversalInboxError,
};

pub async fn front_config(
    settings: web::Data<Settings>,
) -> Result<HttpResponse, UniversalInboxError> {
    let authentication_config = match &settings.application.security.authentication {
        AuthenticationSettings::OpenIDConnect(oidc_settings) => match **oidc_settings {
            OpenIDConnectSettings {
                ref oidc_issuer_url,
                ref user_profile_url,
                oidc_flow_settings:
                    OIDCFlowSettings::AuthorizationCodePKCEFlow(OIDCAuthorizationCodePKCEFlowSettings {
                        ref front_client_id,
                        ..
                    }),
                ..
            } => FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow {
                oidc_issuer_url: oidc_issuer_url.url().clone(),
                oidc_client_id: front_client_id.to_string(),
                oidc_redirect_url: settings
                    .application
                    .get_oidc_auth_code_pkce_flow_redirect_url()?,
                user_profile_url: user_profile_url.clone(),
            },
            OpenIDConnectSettings {
                ref user_profile_url,
                oidc_flow_settings: OIDCFlowSettings::GoogleAuthorizationCodeFlow,
                ..
            } => FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow {
                user_profile_url: user_profile_url.clone(),
            },
        },
        AuthenticationSettings::Local(_) => FrontAuthenticationConfig::Local,
    };

    let integration_providers = HashMap::from_iter(settings.integrations.values().map(|config| {
        (
            config.kind,
            IntegrationProviderStaticConfig {
                name: config.name.clone(),
                nango_config_key: config.nango_key.clone(),
                oauth_user_scopes: if config.use_as_oauth_user_scopes.unwrap_or_default() {
                    config.required_oauth_scopes.clone()
                } else {
                    vec![]
                },
                required_oauth_scopes: config.required_oauth_scopes.clone(),
                doc: config.doc.clone(),
                warning_message: config.warning_message.clone(),
                doc_for_actions: config.doc_for_actions.clone(),
                is_implemented: true,
            },
        )
    }));
    let config = FrontConfig {
        authentication_config,
        nango_base_url: settings.oauth2.nango_base_url.clone(),
        nango_public_key: settings.oauth2.nango_public_key.clone(),
        integration_providers,
        support_href: settings.application.support_href.clone(),
        show_changelog: settings.application.show_changelog,
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
