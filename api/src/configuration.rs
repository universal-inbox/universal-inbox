use std::{collections::HashMap, env};

use anyhow::Context;
use config::{Config, ConfigError, Environment, File};
use openidconnect::{ClientId, ClientSecret, IntrospectionUrl, IssuerUrl};
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Deserializer};
use serde_with::{serde_as, DisplayFromStr};
use universal_inbox::integration_connection::{
    provider::IntegrationProviderKind, NangoProviderKey, NangoPublicKey,
};
use url::Url;

use crate::universal_inbox::UniversalInboxError;

#[derive(Deserialize, Clone, Debug)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub integrations: IntegrationsSettings,
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
}

#[derive(Deserialize, Clone, Debug)]
pub struct SecuritySettings {
    pub csp_extra_connect_src: Vec<String>,
    pub authentication: AuthenticationSettings,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ObservabilitySettings {
    pub tracing: Option<TracingSettings>,
    pub logging: LoggingSettings,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TracingSettings {
    pub otlp_exporter_endpoint: Url,
    pub otlp_exporter_headers: HashMap<String, String>,
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

// tag: New notification integration
#[derive(Deserialize, Clone, Debug)]
pub struct IntegrationsSettings {
    pub oauth2: Oauth2Settings,
    pub github: GithubIntegrationSettings,
    pub linear: DefaultIntegrationSettings,
    pub google_mail: GoogleMailIntegrationSettings,
    pub slack: SlackIntegrationSettings,
    pub todoist: DefaultIntegrationSettings,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Oauth2Settings {
    pub nango_base_url: Url,
    pub nango_secret_key: String,
    pub nango_public_key: NangoPublicKey,
    pub nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GithubIntegrationSettings {
    pub name: String,
    pub page_size: usize,
    pub doc: String,
    pub warning_message: Option<String>,
    pub doc_for_actions: HashMap<String, String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GoogleMailIntegrationSettings {
    pub name: String,
    pub page_size: usize,
    pub doc: String,
    pub warning_message: Option<String>,
    pub doc_for_actions: HashMap<String, String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SlackIntegrationSettings {
    pub name: String,
    pub doc: String,
    pub doc_for_actions: HashMap<String, String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DefaultIntegrationSettings {
    pub name: String,
    pub doc: String,
    pub warning_message: Option<String>,
    pub doc_for_actions: HashMap<String, String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct EmailSettings {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: Secret<String>,
    pub from_header: String,
    pub reply_to_header: String,
}

impl EmailSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "smtp://{}:{}@{}:{}",
            self.smtp_username,
            self.smtp_password.expose_secret(),
            self.smtp_server,
            self.smtp_port,
        )
    }

    pub fn safe_connection_string(&self) -> String {
        self.connection_string()
            .replace(self.smtp_password.expose_secret(), "********")
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
        let config_file_required = file.is_some();
        let config_path = env::var("CONFIG_PATH").unwrap_or_else(|_| "config".into());
        let config_file = file.unwrap_or_else(|| {
            env::var("CONFIG_FILE").unwrap_or_else(|_| format!("{config_path}/dev"))
        });

        let default_config_file = format!("{config_path}/default");
        let local_config_file = format!("{config_path}/local");
        println!(
            "Trying to load {:?} config files",
            vec![&default_config_file, &local_config_file, &config_file]
        );

        let config = Config::builder()
            .add_source(File::with_name(&default_config_file))
            .add_source(File::with_name(&local_config_file).required(false))
            .add_source(File::with_name(&config_file).required(config_file_required))
            .add_source(
                Environment::with_prefix("universal_inbox")
                    .try_parsing(true)
                    .separator("__")
                    .list_separator(",")
                    .with_list_parse_key("application.security.csp_extra_connect_src"),
            )
            .build()?;

        config.try_deserialize()
    }

    pub fn new() -> Result<Self, ConfigError> {
        Settings::new_from_file(None)
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
