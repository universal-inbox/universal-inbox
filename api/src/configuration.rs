use std::env;

use config::{Config, ConfigError, Environment, File};
use openidconnect::{ClientId, ClientSecret, IntrospectionUrl, IssuerUrl};
use serde::Deserialize;
use url::Url;

#[derive(Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub integrations: IntegrationsSettings,
}

#[derive(Deserialize)]
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
}

#[derive(Deserialize)]
pub struct AuthenticationSettings {
    pub oidc_issuer_url: IssuerUrl,
    pub oidc_introspection_url: IntrospectionUrl,
    pub oidc_front_client_id: ClientId,
    pub oidc_api_client_id: ClientId,
    pub oidc_api_client_secret: ClientSecret,
}

#[derive(Deserialize)]
pub struct HttpSessionSettings {
    pub secret_key: String,
    pub max_age_days: i64,
    pub max_age_inactive_days: i64,
}

#[derive(Deserialize)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database_name: String,
}

#[derive(Deserialize)]
pub struct RedisSettings {
    pub port: u16,
    pub host: String,
}

#[derive(Deserialize)]
pub struct IntegrationsSettings {
    pub github: GithubIntegrationSettings,
    pub todoist: TodoistIntegrationSettings,
}

#[derive(Deserialize)]
pub struct GithubIntegrationSettings {
    pub page_size: usize,
    pub api_token: String, // Temporary until oauth is implemented
}

#[derive(Deserialize)]
pub struct TodoistIntegrationSettings {
    pub api_token: String, // Temporary until oauth is implemented
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
        format!("{}:{}", self.host, self.port)
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
