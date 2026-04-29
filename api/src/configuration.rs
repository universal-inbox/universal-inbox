use std::{collections::HashMap, env, time::Duration};

use anyhow::Context;
use config::{Config, ConfigError, Environment, File};
use hex;
use openidconnect::{ClientId, ClientSecret as OidcClientSecret, IntrospectionUrl, IssuerUrl};
use ring::hmac;
use secrecy::{CloneableSecret, ExposeSecret, SecretBox, zeroize::Zeroize};
use serde::{
    Deserialize, Deserializer,
    de::{self, SeqAccess, Visitor},
};
use serde_with::{DisplayFromStr, serde_as};
use url::Url;

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind, user::UserAuthKind,
};

use crate::{
    ExecutionContext, integrations::oauth2::ClientSecret, universal_inbox::UniversalInboxError,
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
    pub sync_backoff_base_delay_in_seconds: u64,
    pub sync_backoff_max_delay_in_seconds: u64,
    pub sync_failure_window_in_hours: i64,
    pub observability: ObservabilitySettings,
    pub security: SecuritySettings,
    pub support_href: Option<String>,
    pub email: EmailSettings,
    pub show_changelog: bool,
    pub version: Option<String>,
    pub dry_run: bool,
    pub chat_support: Option<ChatSupportSettings>,
}

/// Deserialize a list of strings from either a TOML array (`["a", "b"]`)
/// or a comma-separated environment variable (`"a,b"`).
///
/// Required because env vars are loaded without `try_parsing`, so the config
/// crate's built-in `list_separator` no longer applies. `try_parsing` was
/// dropped because it eagerly parses every env value as f64/i64/bool, which
/// destroys precision for numeric-looking strings (e.g. OAuth client IDs).
fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringListVisitor;

    impl<'de> Visitor<'de> for StringListVisitor {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a sequence of strings or a comma-separated string")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect())
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                out.push(item);
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(StringListVisitor)
}

#[derive(Deserialize, Clone, Debug)]
pub struct SecuritySettings {
    #[serde(deserialize_with = "deserialize_string_list")]
    pub csp_extra_connect_src: Vec<String>,
    /// Extra origins allowed to access the MCP endpoint (e.g. MCP inspector URL).
    /// The server's own origin is always allowed.
    #[serde(default)]
    pub mcp_extra_allowed_origins: Vec<String>,
    pub authentication: Vec<AuthenticationSettings>,
    /// Map of email domains to rejection messages.
    /// If a user tries to register/authenticate with an email from a blacklisted domain,
    /// access will be rejected with the corresponding message.
    #[serde(default)]
    pub email_domain_blacklist: HashMap<String, String>,
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
    pub oidc_api_client_secret: OidcClientSecret,
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
    /// Hex-encoded 32-byte AES-256 key for encrypting OAuth tokens at rest.
    pub token_encryption_key: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct IntegrationSettings {
    pub name: String,
    pub kind: IntegrationProviderKind,
    pub required_oauth_scopes: Vec<String>,
    pub use_as_oauth_user_scopes: Option<bool>,
    pub page_size: Option<usize>,
    pub warning_message: Option<String>,
    #[serde(default = "yes")]
    pub is_enabled: bool,
    pub api_max_retry_duration_http_seconds: Option<u64>,
    pub api_max_retry_duration_worker_seconds: Option<u64>,
    pub oauth_client_id: String,
    pub oauth_client_secret: ClientSecret,
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

#[derive(Deserialize, Clone, Debug)]
pub struct ChatSupportSettings {
    pub website_id: String,
    pub identity_verification_secret_key: String,
}

impl ChatSupportSettings {
    pub fn sign_email(&self, email: &str) -> String {
        let key = hmac::Key::new(
            hmac::HMAC_SHA256,
            self.identity_verification_secret_key.as_bytes(),
        );
        let signature = hmac::sign(&key, email.as_bytes());
        hex::encode(signature.as_ref())
    }
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
                .add_source(Environment::with_prefix("universal_inbox").separator("__"))
                .build()?,
        )
        .map_err(|e| ConfigError::Message(e.to_string()))
    }

    pub fn new() -> Result<Self, ConfigError> {
        Settings::new_from_file(None)
    }

    pub fn required_oauth_scopes(&self) -> HashMap<IntegrationProviderKind, Vec<String>> {
        HashMap::from_iter(
            self.integrations
                .values()
                .map(|config| (config.kind, config.required_oauth_scopes.clone())),
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

    /// This function is used to get the OAuth2 callback redirect URL for internal OAuth flows.
    pub fn get_oauth_redirect_url(&self) -> Result<Url, UniversalInboxError> {
        Ok(self
            .front_base_url
            .join(&self.api_path)
            .context("Failed to parse OAuth redirect URL")?
            .join("oauth/callback")
            .context("Failed to parse OAuth redirect URL")?)
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
            mcp_extra_allowed_origins: vec![],
            authentication: vec![local_auth_settings],
            email_domain_blacklist: HashMap::new(),
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
                oidc_api_client_secret: OidcClientSecret::new("secret".to_string()),
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
            mcp_extra_allowed_origins: vec![],
            authentication: vec![oidc_auth_settings],
            email_domain_blacklist: HashMap::new(),
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
                oidc_api_client_secret: OidcClientSecret::new("secret".to_string()),
                user_profile_url: Url::parse("https://example.com/profile").unwrap(),
                oidc_flow_settings: OIDCFlowSettings::GoogleAuthorizationCodeFlow,
            }));
        let security_settings = SecuritySettings {
            csp_extra_connect_src: vec![],
            mcp_extra_allowed_origins: vec![],
            authentication: vec![oidc_auth_settings],
            email_domain_blacklist: HashMap::new(),
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
                oidc_api_client_secret: OidcClientSecret::new("secret".to_string()),
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
            mcp_extra_allowed_origins: vec![],
            authentication: vec![local_auth_settings, oidc_auth_settings],
            email_domain_blacklist: HashMap::new(),
        };

        let result = security_settings.get_authentication_settings(UserAuthKind::Local);
        assert!(matches!(result, Some(local_auth_settings)));

        let result =
            security_settings.get_authentication_settings(UserAuthKind::OIDCAuthorizationCodePKCE);
        assert!(matches!(result, Some(oidc_auth_settings)));
    }

    mod env_source {
        use super::*;
        use config::Map;

        /// Numeric-looking env values must reach String fields verbatim.
        /// Regression test for `try_parsing(true)` coercing OAuth client IDs to f64.
        #[test]
        fn float_like_env_var_keeps_full_precision() {
            #[derive(Deserialize)]
            struct Holder {
                client_id: String,
            }

            let mut env = Map::new();
            env.insert(
                "TEST__CLIENT_ID".to_string(),
                "5775363312438.6774811086659".to_string(),
            );

            let cfg = Config::builder()
                .add_source(
                    Environment::with_prefix("TEST")
                        .separator("__")
                        .source(Some(env)),
                )
                .build()
                .unwrap();

            let holder: Holder = cfg.try_deserialize().unwrap();
            assert_eq!(holder.client_id, "5775363312438.6774811086659");
        }

        #[test]
        fn csp_list_parses_comma_separated_env_var() {
            #[derive(Deserialize)]
            struct Holder {
                #[serde(deserialize_with = "deserialize_string_list")]
                items: Vec<String>,
            }

            let mut env = Map::new();
            env.insert(
                "TEST__ITEMS".to_string(),
                "https://a.example, https://b.example".to_string(),
            );

            let cfg = Config::builder()
                .add_source(
                    Environment::with_prefix("TEST")
                        .separator("__")
                        .source(Some(env)),
                )
                .build()
                .unwrap();

            let holder: Holder = cfg.try_deserialize().unwrap();
            assert_eq!(
                holder.items,
                vec![
                    "https://a.example".to_string(),
                    "https://b.example".to_string()
                ]
            );
        }

        #[test]
        fn csp_list_parses_toml_array() {
            #[derive(Deserialize)]
            struct Holder {
                #[serde(deserialize_with = "deserialize_string_list")]
                items: Vec<String>,
            }

            let cfg = Config::builder()
                .add_source(config::File::from_str(
                    r#"items = ["https://a.example", "https://b.example"]"#,
                    config::FileFormat::Toml,
                ))
                .build()
                .unwrap();

            let holder: Holder = cfg.try_deserialize().unwrap();
            assert_eq!(
                holder.items,
                vec![
                    "https://a.example".to_string(),
                    "https://b.example".to_string()
                ]
            );
        }
    }

    mod sign_email {
        use super::*;

        #[test]
        fn test_sign_email() {
            let chat_support_settings = ChatSupportSettings {
                website_id: "test_website".to_string(),
                identity_verification_secret_key: "0fd72e0ff53b274293029fd1f3f40c92".to_string(),
            };

            let signature = chat_support_settings.sign_email("user@gmail.com");
            assert_eq!(
                signature,
                "cd7cc422ea97c82d844b2373fdcd6259c9ee6e135af65ab6fe6ca85e3f07abb1"
            );
        }
    }
}
