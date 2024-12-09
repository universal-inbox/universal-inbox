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
    pub authentication: AuthenticationSettings,
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
        config_builder
            .add_source(
                Environment::with_prefix("universal_inbox")
                    .try_parsing(true)
                    .separator("__")
                    .list_separator(",")
                    .with_list_parse_key("application.security.csp_extra_connect_src"),
            )
            .build()?
            .try_deserialize()
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
