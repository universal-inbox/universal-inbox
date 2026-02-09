use std::{collections::HashMap, env, fs, sync::Arc};

use apalis_redis::RedisStorage;
use openidconnect::{ClientId, IntrospectionUrl, IssuerUrl};
use rstest::*;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::info;
use wiremock::MockServer;

use universal_inbox_api::{
    configuration::{
        AuthenticationSettings, LocalAuthenticationSettings, OIDCFlowSettings, Settings,
    },
    integrations::slack::SlackService,
    jobs::UniversalInboxJob,
    repository::Repository,
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
    },
    utils::cache::Cache,
};

use crate::common::mailer::MailerStub;
use crate::common::{build_and_spawn, setup_test_env};

// Re-export shared fixtures so rstest can resolve them by name in this module's fixtures
pub use crate::common::{db_connection, redis_storage, settings, tracing_setup};

pub mod auth;
pub mod integration_connection;
pub mod mailer;
pub mod notification;
pub mod rest;
pub mod task;
pub mod user;

pub struct TestedApp {
    pub app_address: String,
    pub api_address: String,
    pub repository: Arc<Repository>,
    pub user_service: Arc<UserService>,
    pub task_service: Arc<RwLock<TaskService>>,
    pub notification_service: Arc<RwLock<NotificationService>>,
    pub integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    pub third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    pub slack_service: Arc<SlackService>,
    pub github_mock_server: MockServer,
    pub linear_mock_server: MockServer,
    pub google_calendar_mock_server: MockServer,
    pub google_mail_mock_server: MockServer,
    pub google_drive_mock_server: MockServer,
    pub slack_mock_server: MockServer,
    pub todoist_mock_server: MockServer,
    pub oidc_issuer_mock_server: Option<MockServer>,
    pub nango_mock_server: MockServer,
    pub mailer_stub: Arc<RwLock<MailerStub>>,
    pub redis_storage: RedisStorage<UniversalInboxJob>,
    pub cache: Cache,
}

impl Drop for TestedApp {
    fn drop(&mut self) {
        let cache = self.cache.clone();
        tokio::spawn(async move {
            let _ = cache.clear(&None).await;
        });
    }
}

#[fixture]
pub async fn tested_app(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) -> TestedApp {
    info!("Setting up server");

    let (listener, port, cache, mock_servers) = setup_test_env(&settings).await;

    let oidc_issuer_mock_server = MockServer::start().await;
    let oidc_issuer_mock_server_url = &oidc_issuer_mock_server.uri();

    if let AuthenticationSettings::OpenIDConnect(oidc_settings) =
        &mut settings.application.security.authentication[0]
    {
        oidc_settings.oidc_issuer_url =
            IssuerUrl::new(oidc_issuer_mock_server_url.to_string()).unwrap();
        if let OIDCFlowSettings::AuthorizationCodePKCEFlow(ref mut flow_settings) =
            oidc_settings.oidc_flow_settings
        {
            flow_settings.introspection_url =
                IntrospectionUrl::new(format!("{oidc_issuer_mock_server_url}/introspect")).unwrap();
            flow_settings.front_client_id = ClientId::new("12345".to_string());
        }
    }

    let pool: Arc<PgPool> = db_connection.await;
    let redis_storage = redis_storage.await;

    let (services, mailer_stub, redis_storage) = build_and_spawn(
        listener,
        pool.clone(),
        settings.clone(),
        &mock_servers,
        redis_storage,
    )
    .await;

    let app_address = format!("http://127.0.0.1:{port}");
    let api_address = format!("{app_address}{}", settings.application.api_path);
    let repository = Arc::new(Repository::new(pool.clone()));

    TestedApp {
        app_address,
        api_address,
        repository,
        user_service: services.user_service,
        task_service: services.task_service,
        notification_service: services.notification_service,
        integration_connection_service: services.integration_connection_service,
        third_party_item_service: services.third_party_item_service,
        slack_service: services.slack_service,
        github_mock_server: mock_servers.github,
        linear_mock_server: mock_servers.linear,
        google_calendar_mock_server: mock_servers.google_calendar,
        google_mail_mock_server: mock_servers.google_mail,
        google_drive_mock_server: mock_servers.google_drive,
        slack_mock_server: mock_servers.slack,
        todoist_mock_server: mock_servers.todoist,
        oidc_issuer_mock_server: Some(oidc_issuer_mock_server),
        nango_mock_server: mock_servers.nango,
        mailer_stub,
        redis_storage,
        cache,
    }
}

#[fixture]
pub async fn tested_app_with_local_auth(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) -> TestedApp {
    info!("Setting up server");

    let (listener, port, cache, mock_servers) = setup_test_env(&settings).await;

    settings.application.security.authentication =
        vec![AuthenticationSettings::Local(LocalAuthenticationSettings {
            argon2_algorithm: argon2::Algorithm::Argon2id,
            argon2_version: argon2::Version::V0x13,
            argon2_memory_size: 20000,
            argon2_iterations: 2,
            argon2_parallelism: 1,
        })];
    settings.application.security.email_domain_blacklist = HashMap::new();

    let pool: Arc<PgPool> = db_connection.await;
    let redis_storage = redis_storage.await;

    let (services, mailer_stub, redis_storage) = build_and_spawn(
        listener,
        pool.clone(),
        settings.clone(),
        &mock_servers,
        redis_storage,
    )
    .await;

    let app_address = format!("http://127.0.0.1:{port}");
    let api_address = format!("{app_address}{}", settings.application.api_path);
    let repository = Arc::new(Repository::new(pool.clone()));

    TestedApp {
        app_address,
        api_address,
        repository,
        user_service: services.user_service,
        task_service: services.task_service,
        notification_service: services.notification_service,
        integration_connection_service: services.integration_connection_service,
        third_party_item_service: services.third_party_item_service,
        slack_service: services.slack_service,
        github_mock_server: mock_servers.github,
        linear_mock_server: mock_servers.linear,
        google_calendar_mock_server: mock_servers.google_calendar,
        google_mail_mock_server: mock_servers.google_mail,
        google_drive_mock_server: mock_servers.google_drive,
        slack_mock_server: mock_servers.slack,
        todoist_mock_server: mock_servers.todoist,
        oidc_issuer_mock_server: None,
        nango_mock_server: mock_servers.nango,
        mailer_stub,
        redis_storage,
        cache,
    }
}

#[fixture]
pub async fn tested_app_with_domain_blacklist(
    mut settings: Settings,
    #[allow(unused, clippy::let_unit_value)] tracing_setup: (),
    #[future] db_connection: Arc<PgPool>,
    #[future] redis_storage: RedisStorage<UniversalInboxJob>,
) -> TestedApp {
    info!("Setting up server with domain blacklist");

    let (listener, port, cache, mock_servers) = setup_test_env(&settings).await;

    let oidc_issuer_mock_server = MockServer::start().await;
    let oidc_issuer_mock_server_url = &oidc_issuer_mock_server.uri();

    // Set up OIDC authentication settings pointing to mock server
    if let AuthenticationSettings::OpenIDConnect(oidc_settings) =
        &mut settings.application.security.authentication[0]
    {
        oidc_settings.oidc_issuer_url =
            IssuerUrl::new(oidc_issuer_mock_server_url.to_string()).unwrap();
        if let OIDCFlowSettings::AuthorizationCodePKCEFlow(ref mut flow_settings) =
            oidc_settings.oidc_flow_settings
        {
            flow_settings.introspection_url =
                IntrospectionUrl::new(format!("{oidc_issuer_mock_server_url}/introspect")).unwrap();
            flow_settings.front_client_id = ClientId::new("12345".to_string());
        }
    }
    // Also add local auth for local registration tests
    settings
        .application
        .security
        .authentication
        .push(AuthenticationSettings::Local(LocalAuthenticationSettings {
            argon2_algorithm: argon2::Algorithm::Argon2id,
            argon2_version: argon2::Version::V0x13,
            argon2_memory_size: 20000,
            argon2_iterations: 2,
            argon2_parallelism: 1,
        }));
    settings.application.security.email_domain_blacklist.insert(
        "blocked.com".to_string(),
        "Registration is not allowed from this domain".to_string(),
    );

    let pool: Arc<PgPool> = db_connection.await;
    let redis_storage = redis_storage.await;

    let (services, mailer_stub, redis_storage) = build_and_spawn(
        listener,
        pool.clone(),
        settings.clone(),
        &mock_servers,
        redis_storage,
    )
    .await;

    let app_address = format!("http://127.0.0.1:{port}");
    let api_address = format!("{app_address}{}", settings.application.api_path);
    let repository = Arc::new(Repository::new(pool.clone()));

    TestedApp {
        app_address,
        api_address,
        repository,
        user_service: services.user_service,
        task_service: services.task_service,
        notification_service: services.notification_service,
        integration_connection_service: services.integration_connection_service,
        third_party_item_service: services.third_party_item_service,
        slack_service: services.slack_service,
        github_mock_server: mock_servers.github,
        linear_mock_server: mock_servers.linear,
        google_calendar_mock_server: mock_servers.google_calendar,
        google_mail_mock_server: mock_servers.google_mail,
        google_drive_mock_server: mock_servers.google_drive,
        slack_mock_server: mock_servers.slack,
        todoist_mock_server: mock_servers.todoist,
        oidc_issuer_mock_server: Some(oidc_issuer_mock_server),
        nango_mock_server: mock_servers.nango,
        mailer_stub,
        redis_storage,
        cache,
    }
}

/// Custom wiremock matcher: query parameter is absent
pub struct QueryParamAbsent(pub String);

impl wiremock::Match for QueryParamAbsent {
    fn matches(&self, request: &wiremock::Request) -> bool {
        !request
            .url
            .query_pairs()
            .any(|(name, _)| name == self.0.as_str())
    }
}

/// Custom wiremock matcher: query parameter exists (any value)
pub struct QueryParamPresent(pub String);

impl wiremock::Match for QueryParamPresent {
    fn matches(&self, request: &wiremock::Request) -> bool {
        request
            .url
            .query_pairs()
            .any(|(name, _)| name == self.0.as_str())
    }
}

pub fn fixture_path(fixture_file_name: &str) -> String {
    format!(
        "{}/tests/api/fixtures/{fixture_file_name}",
        env::var("CARGO_MANIFEST_DIR").unwrap()
    )
}

pub fn load_json_fixture_file<T: for<'de> serde::de::Deserialize<'de>>(
    fixture_file_name: &str,
) -> T {
    let input_str = fs::read_to_string(fixture_path(fixture_file_name)).unwrap();
    serde_json::from_str::<T>(&input_str).unwrap()
}
