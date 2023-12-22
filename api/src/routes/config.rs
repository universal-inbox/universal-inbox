use std::collections::HashMap;

use actix_web::{
    http::header::{CacheControl, CacheDirective},
    web, HttpResponse,
};
use anyhow::Context;

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind, FrontAuthenticationConfig,
    FrontConfig, IntegrationProviderStaticConfig,
};

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

    let config = FrontConfig {
        authentication_config,
        nango_base_url: settings.integrations.oauth2.nango_base_url.clone(),
        nango_public_key: settings.integrations.oauth2.nango_public_key.clone(),
        // tag: New notification integration
        integration_providers: HashMap::from([
            (
                IntegrationProviderKind::Github,
                IntegrationProviderStaticConfig {
                    name: settings.integrations.github.name.clone(),
                    nango_config_key: settings
                        .integrations
                        .oauth2
                        .nango_provider_keys
                        .get(&IntegrationProviderKind::Github)
                        .context("Missing Nango config key for Github")?
                        .clone(),
                    doc: settings.integrations.github.doc.clone(),
                    warning_message: settings.integrations.github.warning_message.clone(),
                    doc_for_actions: settings.integrations.github.doc_for_actions.clone(),
                    is_implemented: true,
                },
            ),
            (
                IntegrationProviderKind::Linear,
                IntegrationProviderStaticConfig {
                    name: settings.integrations.linear.name.clone(),
                    nango_config_key: settings
                        .integrations
                        .oauth2
                        .nango_provider_keys
                        .get(&IntegrationProviderKind::Linear)
                        .context("Missing Nango config key for Linear")?
                        .clone(),
                    doc: settings.integrations.linear.doc.clone(),
                    warning_message: settings.integrations.linear.warning_message.clone(),
                    doc_for_actions: settings.integrations.linear.doc_for_actions.clone(),
                    is_implemented: true,
                },
            ),
            (
                IntegrationProviderKind::GoogleMail,
                IntegrationProviderStaticConfig {
                    name: settings.integrations.google_mail.name.clone(),
                    nango_config_key: settings
                        .integrations
                        .oauth2
                        .nango_provider_keys
                        .get(&IntegrationProviderKind::GoogleMail)
                        .context("Missing Nango config key for Google Mail")?
                        .clone(),
                    doc: settings.integrations.google_mail.doc.clone(),
                    warning_message: settings.integrations.google_mail.warning_message.clone(),
                    doc_for_actions: settings.integrations.google_mail.doc_for_actions.clone(),
                    is_implemented: true,
                },
            ),
            (
                IntegrationProviderKind::Slack,
                IntegrationProviderStaticConfig {
                    name: "Slack".to_string(),
                    nango_config_key: "slack".to_string().into(),
                    doc: "".to_string(),
                    warning_message: None,
                    doc_for_actions: HashMap::new(),
                    is_implemented: false,
                },
            ),
            (
                IntegrationProviderKind::Notion,
                IntegrationProviderStaticConfig {
                    name: "Notion".to_string(),
                    nango_config_key: "notion".to_string().into(),
                    doc: "".to_string(),
                    warning_message: None,
                    doc_for_actions: HashMap::new(),
                    is_implemented: false,
                },
            ),
            (
                IntegrationProviderKind::GoogleDocs,
                IntegrationProviderStaticConfig {
                    name: "Google Docs".to_string(),
                    nango_config_key: "googledocs".to_string().into(),
                    doc: "".to_string(),
                    warning_message: None,
                    doc_for_actions: HashMap::new(),
                    is_implemented: false,
                },
            ),
            (
                IntegrationProviderKind::Todoist,
                IntegrationProviderStaticConfig {
                    name: settings.integrations.todoist.name.clone(),
                    nango_config_key: settings
                        .integrations
                        .oauth2
                        .nango_provider_keys
                        .get(&IntegrationProviderKind::Todoist)
                        .context("Missing Nango config key for Todoist")?
                        .clone(),
                    doc: settings.integrations.todoist.doc.clone(),
                    warning_message: settings.integrations.todoist.warning_message.clone(),
                    doc_for_actions: settings.integrations.todoist.doc_for_actions.clone(),
                    is_implemented: true,
                },
            ),
            (
                IntegrationProviderKind::TickTick,
                IntegrationProviderStaticConfig {
                    name: "Tick Tick".to_string(),
                    nango_config_key: "ticktick".to_string().into(),
                    doc: "".to_string(),
                    warning_message: None,
                    doc_for_actions: HashMap::new(),
                    is_implemented: false,
                },
            ),
        ]),
        support_href: settings.application.support_href.clone(),
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
