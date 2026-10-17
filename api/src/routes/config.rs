use std::collections::HashMap;

use actix_web::{
    HttpResponse,
    http::header::{CacheControl, CacheDirective},
    web,
};
use anyhow::Context;

use universal_inbox::{
    FrontAuthenticationConfig, FrontConfig, FrontOIDCAuthorizationCodePKCEFlowConfig,
    FrontOIDCGoogleAuthorizationCodeFlowConfig, IntegrationProviderStaticConfig,
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
    let oidc_redirect_url = settings
        .application
        .get_oidc_auth_code_pkce_flow_redirect_url()?;
    let authentication_configs = settings
        .application
        .security
        .authentication
        .iter()
        .map(|auth| match auth {
            AuthenticationSettings::OpenIDConnect(oidc_settings) => match **oidc_settings {
                OpenIDConnectSettings {
                    ref oidc_issuer_url,
                    ref user_profile_url,
                    oidc_flow_settings:
                        OIDCFlowSettings::AuthorizationCodePKCEFlow(
                            OIDCAuthorizationCodePKCEFlowSettings {
                                ref front_client_id,
                                ..
                            },
                        ),
                    ..
                } => FrontAuthenticationConfig::OIDCAuthorizationCodePKCEFlow(
                    FrontOIDCAuthorizationCodePKCEFlowConfig {
                        oidc_issuer_url: oidc_issuer_url.url().clone(),
                        oidc_client_id: front_client_id.to_string(),
                        oidc_redirect_url: oidc_redirect_url.clone(),
                        user_profile_url: user_profile_url.clone(),
                    },
                ),
                OpenIDConnectSettings {
                    ref user_profile_url,
                    oidc_flow_settings: OIDCFlowSettings::GoogleAuthorizationCodeFlow,
                    ..
                } => FrontAuthenticationConfig::OIDCGoogleAuthorizationCodeFlow(
                    FrontOIDCGoogleAuthorizationCodeFlowConfig {
                        user_profile_url: user_profile_url.clone(),
                    },
                ),
            },
            AuthenticationSettings::Local(_) => FrontAuthenticationConfig::Local,
            AuthenticationSettings::Passkey => FrontAuthenticationConfig::Passkey,
        })
        .collect();

    let integration_providers = HashMap::from_iter(settings.integrations.values().map(|config| {
        (
            config.kind,
            IntegrationProviderStaticConfig {
                name: config.name.clone(),
                oauth_user_scopes: if config.use_as_oauth_user_scopes.unwrap_or_default() {
                    config.required_oauth_scopes.clone()
                } else {
                    vec![]
                },
                required_oauth_scopes: config.required_oauth_scopes.clone(),
                warning_message: config.warning_message.clone(),
                is_enabled: config.is_enabled,
            },
        )
    }));
    let config = FrontConfig {
        authentication_configs,
        integration_providers,
        support_href: settings.application.support_href.clone(),
        privacy_policy_href: settings.application.privacy_policy_href.clone(),
        terms_href: settings.application.terms_href.clone(),
        show_changelog: settings.application.show_changelog,
        chat_support_website_id: settings
            .application
            .chat_support
            .as_ref()
            .map(|chat_support| chat_support.website_id.clone()),
        version: settings.application.version.clone(),
        email_enabled: settings.application.email.is_some(),
        audit_log_retention_days: settings.application.audit_log.retention_days,
    };

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .insert_header(CacheControl(vec![
            // Keep this response out of shared caches (CDN/proxies): the
            // payload may contain per-deployment values (OIDC issuer URLs,
            // client_ids, chat_support metadata) that must not surface across
            // tenants if a shared cache is misconfigured.
            CacheDirective::Private,
            // Cache only for a few second so that the preload of this config is effective
            CacheDirective::MaxAge(5u32),
        ]))
        .body(serde_json::to_string(&config).context("Cannot serialize front configuration")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::body::to_bytes;
    use pretty_assertions::assert_eq;
    use rstest::*;
    use universal_inbox::FrontConfig;

    #[fixture]
    fn settings() -> Settings {
        Settings::new_from_file(Some("config/test".to_string()))
            .expect("Cannot load test configuration")
    }

    async fn get_front_config(settings: Settings) -> FrontConfig {
        let response = front_config(web::Data::new(settings))
            .await
            .expect("front_config failed");
        let body = to_bytes(response.into_body())
            .await
            .expect("Cannot read the front_config body");
        serde_json::from_slice(&body).expect("Cannot deserialize the front configuration")
    }

    #[rstest]
    #[tokio::test]
    async fn test_front_config_has_no_legal_links_by_default(settings: Settings) {
        let config = get_front_config(settings).await;

        assert_eq!(config.privacy_policy_href, None);
        assert_eq!(config.terms_href, None);
    }

    #[rstest]
    #[tokio::test]
    async fn test_front_config_exposes_the_configured_legal_links(mut settings: Settings) {
        settings.application.privacy_policy_href = Some("https://example.com/privacy".to_string());
        settings.application.terms_href = Some("https://example.com/terms".to_string());

        let config = get_front_config(settings).await;

        assert_eq!(
            config.privacy_policy_href,
            Some("https://example.com/privacy".to_string())
        );
        assert_eq!(
            config.terms_href,
            Some("https://example.com/terms".to_string())
        );
    }
}
