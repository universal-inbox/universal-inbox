use std::{collections::HashMap, env};

use config::{Config, ConfigError, Environment, File};
use openidconnect::{ClientId, ClientSecret, IntrospectionUrl, IssuerUrl};
use serde::Deserialize;
use universal_inbox::integration_connection::{IntegrationProviderKind, NangoProviderKey};
use url::Url;

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub integrations: IntegrationsSettings,
}

#[derive(Deserialize, Clone)]
pub struct ApplicationSettings {
    pub port: u16,
    pub log_directive: String,
    pub dependencies_log_directive: String,
    pub front_base_url: Url,
    pub api_path: String,
    pub static_path: Option<String>,
    pub static_dir: Option<String>,
    pub authentication: AuthenticationSettings,
    pub http_session: HttpSessionSettings,
    pub min_sync_notifications_interval_in_minutes: i64,
    pub min_sync_tasks_interval_in_minutes: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AuthenticationSettings {
    pub oidc_issuer_url: IssuerUrl,
    pub oidc_introspection_url: IntrospectionUrl,
    pub oidc_front_client_id: ClientId,
    pub oidc_api_client_id: ClientId,
    pub oidc_api_client_secret: ClientSecret,
    pub user_profile_url: Url,
}

#[derive(Deserialize, Clone)]
pub struct HttpSessionSettings {
    pub secret_key: String,
    pub max_age_days: i64,
    pub max_age_inactive_days: i64,
}

#[derive(Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database_name: String,
}

#[derive(Deserialize, Clone)]
pub struct RedisSettings {
    pub port: u16,
    pub host: String,
    pub user: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct IntegrationsSettings {
    pub oauth2: Oauth2Settings,
    pub github: GithubIntegrationSettings,
    pub todoist: TodoistIntegrationSettings,
}

#[derive(Deserialize, Clone)]
pub struct Oauth2Settings {
    pub nango_base_url: Url,
    pub nango_secret_key: String,
    pub nango_provider_keys: HashMap<IntegrationProviderKind, NangoProviderKey>,
}

#[derive(Deserialize, Clone)]
pub struct GithubIntegrationSettings {
    pub name: String,
    pub comment: Option<String>,
    pub page_size: usize,
}

#[derive(Deserialize, Clone)]
pub struct TodoistIntegrationSettings {
    pub name: String,
    pub comment: Option<String>,
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
            .add_source(Environment::with_prefix("universal_inbox"))
            .build()?;

        config.try_deserialize()
    }

    pub fn new() -> Result<Self, ConfigError> {
        Settings::new_from_file(None)
    }
}
