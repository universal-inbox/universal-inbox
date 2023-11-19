use std::{collections::HashMap, env};

use anyhow::Context;
use config::{Config, ConfigError, Environment, File};
use openidconnect::{ClientId, ClientSecret, IntrospectionUrl, IssuerUrl};
use serde::Deserialize;
use universal_inbox::integration_connection::{
    IntegrationProviderKind, NangoProviderKey, NangoPublicKey,
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
    pub dependencies_log_directive: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AuthenticationSettings {
    pub oidc_issuer_url: IssuerUrl,
    pub oidc_api_client_id: ClientId,
    pub oidc_api_client_secret: ClientSecret,
    pub oidc_flow_settings: OIDCFlowSettings,
    pub user_profile_url: Url,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum OIDCFlowSettings {
    AuthorizationCodePKCEFlow {
        // Introspection URL is only required for the Authorization code PKCE flow as
        // the API server must validate (ie. has not be revoked) the access token sent by the front.
        introspection_url: IntrospectionUrl,
        front_client_id: ClientId,
    },
    GoogleAuthorizationCodeFlow,
}

#[derive(Deserialize, Clone, Debug)]
pub struct HttpSessionSettings {
    pub secret_key: String,
    pub max_age_days: i64,
    pub max_age_inactive_days: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database_name: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RedisSettings {
    pub port: u16,
    pub host: String,
    pub user: Option<String>,
    pub password: Option<String>,
}

// tag: New notification integration
#[derive(Deserialize, Clone, Debug)]
pub struct IntegrationsSettings {
    pub oauth2: Oauth2Settings,
    pub github: GithubIntegrationSettings,
    pub linear: DefaultIntegrationSettings,
    pub google_mail: GoogleMailIntegrationSettings,
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
pub struct DefaultIntegrationSettings {
    pub name: String,
    pub doc: String,
    pub warning_message: Option<String>,
    pub doc_for_actions: HashMap<String, String>,
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database_name
        )
    }

    pub fn connection_string_without_db(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}",
            self.username, self.password, self.host, self.port
        )
    }
}

impl RedisSettings {
    pub fn connection_string(&self) -> String {
        if let Some(password) = &self.password {
            format!(
                "redis://{}:{password}@{}:{}",
                self.user.clone().unwrap_or_default(),
                self.host,
                self.port
            )
        } else {
            format!("redis://{}:{}", self.host, self.port)
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
