use std::{collections::HashMap, env, time::Duration};

use anyhow::Context;
use config::{Config, ConfigError, Environment, File};
use openidconnect::{ClientId, ClientSecret, IntrospectionUrl, IssuerUrl};
use secrecy::{zeroize::Zeroize, CloneableSecret, ExposeSecret, SecretBox};
use serde::{Deserialize, Deserializer};
use serde_with::{serde_as, DisplayFromStr};
use url::Url;

use universal_inbox::integration_connection::{
    provider::IntegrationProviderKind, NangoProviderKey, NangoPublicKey,
};

use crate::{
    universal_inbox::{user::model::UserAuthKind, UniversalInboxError},
    ExecutionContext,
};

#[derive(Deserialize, Clone, Debug)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub integrations: HashMap<String, IntegrationSettings>,
    pub oauth2: Oauth2Settings,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ApplicationSettings {
    pub environment: String,
    pub listen_address: String,
    pub listen_port: u16,
    pub front_base_url: Url,
    pub api_path: String,
    pub static_path: Option<String>,
    pub static_dir: Option<String>,
    pub http_session: HttpSessionSettings,
    pub min_sync_notifications_interval_in_minutes: i64,
    pub min_sync_tasks_interval_in_minutes: i64,
    pub observability: ObservabilitySettings,
    pub security: SecuritySettings,
    pub support_href: Option<String>,
    pub email: EmailSettings,
    pub show_changelog: bool,
    pub version: Option<String>,
    pub dry_run: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SecuritySettings {
    pub csp_extra_connect_src: Vec<String>,
    pub authentication: Vec<AuthenticationSettings>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ObservabilitySettings {
    pub tracing: Option<TracingSettings>,
    pub logging: LoggingSettings,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug)]
pub struct TracingSettings {
    pub otlp_exporter_protocol: OtlpExporterProtocol,
    pub otlp_exporter_endpoint: Url,
    pub otlp_exporter_headers: HashMap<String, String>,
    #[serde(default = "yes")]
    pub is_stdout_logging_enabled: bool,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Copy)]
pub enum OtlpExporterProtocol {
    Http,
    Grpc,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LoggingSettings {
    pub log_directive: String,
    pub dependencies_log_level: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum AuthenticationSettings {
    OpenIDConnect(Box<OpenIDConnectSettings>),
    Local(LocalAuthenticationSettings),
    Passkey,
}

#[derive(Deserialize, Clone, Debug)]
pub struct OpenIDConnectSettings {
    pub oidc_issuer_url: IssuerUrl,
    pub oidc_api_client_id: ClientId,
    pub oidc_api_client_secret: ClientSecret,
    pub user_profile_url: Url,
    pub oidc_flow_settings: OIDCFlowSettings,
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
pub struct LocalAuthenticationSettings {
    #[serde_as(as = "DisplayFromStr")]
    pub argon2_algorithm: argon2::Algorithm,
    #[serde(deserialize_with = "from_u32")]
    pub argon2_version: argon2::Version,
    pub argon2_memory_size: u32,
    pub argon2_iterations: u32,
    pub argon2_parallelism: u32,
}

fn from_u32<'de, D>(deserializer: D) -> Result<argon2::Version, D::Error>
where
    D: Deserializer<'de>,
{
    let value: u32 = Deserialize::deserialize(deserializer)?;
    value.try_into().map_err(serde::de::Error::custom)
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum OIDCFlowSettings {
    AuthorizationCodePKCEFlow(OIDCAuthorizationCodePKCEFlowSettings),
    GoogleAuthorizationCodeFlow,
}

#[derive(Deserialize, Clone, Debug)]
pub struct OIDCAuthorizationCodePKCEFlowSettings {
    // Introspection URL is only required for the Authorization code PKCE flow as
    // the API server must validate (ie. has not be revoked) the access token sent by the front.
    pub introspection_url: IntrospectionUrl,
    pub front_client_id: ClientId,
}

#[derive(Deserialize, Clone, Debug)]
pub struct HttpSessionSettings {
    pub secret_key: String,
    pub jwt_secret_key: String,
    pub jwt_public_key: String,
    pub jwt_token_expiration_in_days: i64,
    pub max_age_days: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub use_tls: bool,
    pub max_connections: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RedisSettings {
    pub port: u16,
    pub host: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub use_tls: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Oauth2Settings {
    pub nango_base_url: Url,
    pub nango_secret_key: String,
    pub nango_public_key: NangoPublicKey,
}

#[derive(Deserialize, Clone, Debug)]
pub struct IntegrationSettings {
    pub name: String,
    pub kind: IntegrationProviderKind,
    pub nango_key: NangoProviderKey,
    pub required_oauth_scopes: Vec<String>,
    pub use_as_oauth_user_scopes: Option<bool>,
    pub page_size: Option<usize>,
    pub warning_message: Option<String>,
    #[serde(default = "yes")]
    pub is_enabled: bool,
    pub api_max_retry_duration_http_seconds: Option<u64>,
    pub api_max_retry_duration_worker_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SmtpPassword(pub String);

impl Zeroize for SmtpPassword {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl CloneableSecret for SmtpPassword {}

impl<'de> serde::Deserialize<'de> for SmtpPassword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(SmtpPassword(s))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct EmailSettings {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: SecretBox<SmtpPassword>,
    pub from_header: String,
    pub reply_to_header: String,
}

impl EmailSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "smtp://{}:{}@{}:{}",
            self.smtp_username,
            self.smtp_password.expose_secret().0,
            self.smtp_server,
            self.smtp_port,
        )
    }

    pub fn safe_connection_string(&self) -> String {
        self.connection_string()
            .replace(&self.smtp_password.expose_secret().0, "********")
    }
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}{}",
            self.username,
            self.password,
            self.host,
            self.port,
            self.database_name,
            if self.use_tls { "?sslmode=require" } else { "" }
        )
    }

    pub fn safe_connection_string(&self) -> String {
        self.connection_string().replace(&self.password, "********")
    }

    pub fn connection_string_without_db(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}{}",
            self.username,
            self.password,
            self.host,
            self.port,
            if self.use_tls { "?sslmode=require" } else { "" }
        )
    }
}

impl RedisSettings {
    pub fn connection_string(&self) -> String {
        let scheme = if self.use_tls { "rediss" } else { "redis" };
        if let Some(password) = &self.password {
            format!(
                "{scheme}://{}:{password}@{}:{}",
                self.user.clone().unwrap_or_default(),
                self.host,
                self.port
            )
        } else {
            format!("{scheme}://{}:{}", self.host, self.port)
        }
    }

    pub fn safe_connection_string(&self) -> String {
        if let Some(password) = &self.password {
            self.connection_string().replace(password, "********")
        } else {
            self.connection_string()
        }
    }
}

impl Settings {
    pub fn new_from_file(file: Option<String>) -> Result<Self, ConfigError> {
        let config_path = env::var("CONFIG_PATH").unwrap_or_else(|_| "config".into());
        let mut config_builder =
            Config::builder().add_source(File::with_name(&format!("{config_path}/default")));
        eprintln!("Loading {config_path}/default config file");
        if let Some(file) = file {
            config_builder = config_builder.add_source(File::with_name(&file).required(true));
            eprintln!("Loading {file} config file");
        } else if let Ok(config_file) = env::var("CONFIG_FILE") {
            config_builder =
                config_builder.add_source(File::with_name(&config_file).required(true));
            eprintln!("Loading {config_file} config file");
        } else {
            config_builder = config_builder
                .add_source(File::with_name(&format!("{config_path}/dev")).required(true));
            eprintln!("Loading {config_path}/dev config file");
            config_builder = config_builder
                .add_source(File::with_name(&format!("{config_path}/local")).required(false));
            eprintln!("Loading {config_path}/local config file");
        }

        serde_path_to_error::deserialize(
            config_builder
                .add_source(
                    Environment::with_prefix("universal_inbox")
                        .try_parsing(true)
                        .separator("__")
                        .list_separator(",")
                        .with_list_parse_key("application.security.csp_extra_connect_src"),
                )
                .build()?,
        )
        .map_err(|e| ConfigError::Message(e.to_string()))
    }

    pub fn new() -> Result<Self, ConfigError> {
        Settings::new_from_file(None)
    }

    pub fn nango_provider_keys(&self) -> HashMap<IntegrationProviderKind, NangoProviderKey> {
        HashMap::from_iter(
            self.integrations
                .values()
                .map(|config| (config.kind, config.nango_key.clone())),
        )
    }

    pub fn get_integration_max_retry_duration(
        &self,
        context: ExecutionContext,
        integration_name: &str,
    ) -> Duration {
        let config = self.integrations.get(integration_name);
        match context {
            ExecutionContext::Http => Duration::from_secs(
                config
                    .and_then(|config| config.api_max_retry_duration_http_seconds)
                    .unwrap_or(30), // Default to 30 seconds for HTTP
            ),
            ExecutionContext::Worker => Duration::from_secs(
                config
                    .and_then(|config| config.api_max_retry_duration_worker_seconds)
                    .unwrap_or(600), // Default to 600 seconds for workers
            ),
        }
    }
}

impl ApplicationSettings {
    /// This function is used to get the OIDC redirect URL for the front client.
    pub fn get_oidc_auth_code_pkce_flow_redirect_url(&self) -> Result<Url, UniversalInboxError> {
        Ok(self
            .front_base_url
            .join("auth-oidc-callback")
            .context("Failed to parse OIDC redirect URL")?)
    }

    /// This function is used to get the OIDC redirect URL for the API client.
    pub fn get_oidc_auth_code_flow_redirect_url(&self) -> Result<Url, UniversalInboxError> {
        Ok(self
            .front_base_url
            .join(&self.api_path)
            .context("Failed to parse OIDC redirect URL")?
            .join("auth/session/authenticated")
            .context("Failed to parse OIDC redirect URL")?)
    }
}

impl SecuritySettings {
    pub fn get_authentication_settings(
        &self,
        user_auth_kind: UserAuthKind,
    ) -> Option<AuthenticationSettings> {
        self.authentication
            .iter()
            .find(|auth_settings| match (auth_settings, user_auth_kind) {
                (AuthenticationSettings::Local(_), UserAuthKind::Local) => true,
                (AuthenticationSettings::Passkey, UserAuthKind::Passkey) => true,
                (
                    AuthenticationSettings::OpenIDConnect(oidc_settings),
                    UserAuthKind::OIDCAuthorizationCodePKCE,
                ) => {
                    matches!(
                        oidc_settings.oidc_flow_settings,
                        OIDCFlowSettings::AuthorizationCodePKCEFlow(_)
                    )
                }
                (
                    AuthenticationSettings::OpenIDConnect(oidc_settings),
                    UserAuthKind::OIDCGoogleAuthorizationCode,
                ) => {
                    matches!(
                        oidc_settings.oidc_flow_settings,
                        OIDCFlowSettings::GoogleAuthorizationCodeFlow
                    )
                }
                _ => false,
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_variables)]
    use super::*;
    use openidconnect::{IntrospectionUrl, IssuerUrl};
    use url::Url;

    #[test]
    fn test_get_authentication_settings_local() {
        let local_auth_settings = AuthenticationSettings::Local(LocalAuthenticationSettings {
            argon2_algorithm: argon2::Algorithm::Argon2id,
            argon2_version: argon2::Version::V0x13,
            argon2_memory_size: 19456,
            argon2_iterations: 2,
            argon2_parallelism: 1,
        });
        let security_settings = SecuritySettings {
            csp_extra_connect_src: vec![],
            authentication: vec![local_auth_settings],
        };

        let result = security_settings.get_authentication_settings(UserAuthKind::Local);
        assert!(matches!(result, Some(local_auth_settings)));

        // Should not match other auth kinds
        let result =
            security_settings.get_authentication_settings(UserAuthKind::OIDCAuthorizationCodePKCE);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_authentication_settings_oidc_pkce() {
        let oidc_auth_settings =
            AuthenticationSettings::OpenIDConnect(Box::new(OpenIDConnectSettings {
                oidc_issuer_url: IssuerUrl::new("https://example.com".to_string()).unwrap(),
                oidc_api_client_id: ClientId::new("client_id".to_string()),
                oidc_api_client_secret: ClientSecret::new("secret".to_string()),
                user_profile_url: Url::parse("https://example.com/profile").unwrap(),
                oidc_flow_settings: OIDCFlowSettings::AuthorizationCodePKCEFlow(
                    OIDCAuthorizationCodePKCEFlowSettings {
                        introspection_url: IntrospectionUrl::new(
                            "https://example.com/introspect".to_string(),
                        )
                        .unwrap(),
                        front_client_id: ClientId::new("front_client_id".to_string()),
                    },
                ),
            }));
        let security_settings = SecuritySettings {
            csp_extra_connect_src: vec![],
            authentication: vec![oidc_auth_settings],
        };

        let result =
            security_settings.get_authentication_settings(UserAuthKind::OIDCAuthorizationCodePKCE);
        assert!(matches!(result, Some(oidc_auth_settings)));

        // Should not match other auth kinds
        let result = security_settings.get_authentication_settings(UserAuthKind::Local);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_authentication_settings_oidc_google() {
        let oidc_auth_settings =
            AuthenticationSettings::OpenIDConnect(Box::new(OpenIDConnectSettings {
                oidc_issuer_url: IssuerUrl::new("https://example.com".to_string()).unwrap(),
                oidc_api_client_id: ClientId::new("client_id".to_string()),
                oidc_api_client_secret: ClientSecret::new("secret".to_string()),
                user_profile_url: Url::parse("https://example.com/profile").unwrap(),
                oidc_flow_settings: OIDCFlowSettings::GoogleAuthorizationCodeFlow,
            }));
        let security_settings = SecuritySettings {
            csp_extra_connect_src: vec![],
            authentication: vec![oidc_auth_settings],
        };

        let result = security_settings
            .get_authentication_settings(UserAuthKind::OIDCGoogleAuthorizationCode);
        assert!(matches!(result, Some(oidc_auth_settings)));

        // Should not match other auth kinds
        let result =
            security_settings.get_authentication_settings(UserAuthKind::OIDCAuthorizationCodePKCE);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_authentication_settings_multiple_configs() {
        let local_auth_settings = AuthenticationSettings::Local(LocalAuthenticationSettings {
            argon2_algorithm: argon2::Algorithm::Argon2id,
            argon2_version: argon2::Version::V0x13,
            argon2_memory_size: 19456,
            argon2_iterations: 2,
            argon2_parallelism: 1,
        });
        let oidc_auth_settings =
            AuthenticationSettings::OpenIDConnect(Box::new(OpenIDConnectSettings {
                oidc_issuer_url: IssuerUrl::new("https://example.com".to_string()).unwrap(),
                oidc_api_client_id: ClientId::new("client_id".to_string()),
                oidc_api_client_secret: ClientSecret::new("secret".to_string()),
                user_profile_url: Url::parse("https://example.com/profile").unwrap(),
                oidc_flow_settings: OIDCFlowSettings::AuthorizationCodePKCEFlow(
                    OIDCAuthorizationCodePKCEFlowSettings {
                        introspection_url: IntrospectionUrl::new(
                            "https://example.com/introspect".to_string(),
                        )
                        .unwrap(),
                        front_client_id: ClientId::new("front_client_id".to_string()),
                    },
                ),
            }));
        let security_settings = SecuritySettings {
            csp_extra_connect_src: vec![],
            authentication: vec![local_auth_settings, oidc_auth_settings],
        };

        let result = security_settings.get_authentication_settings(UserAuthKind::Local);
        assert!(matches!(result, Some(local_auth_settings)));

        let result =
            security_settings.get_authentication_settings(UserAuthKind::OIDCAuthorizationCodePKCE);
        assert!(matches!(result, Some(oidc_auth_settings)));
    }
}
