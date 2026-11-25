use std::{fmt, time::SystemTime};

use cucumber::World;
use futures::future::LocalBoxFuture;
use reqwest::{Client, StatusCode, header::HeaderMap};

use email_address::EmailAddress;

use universal_inbox::{pii::Pii, user::User};

use crate::{
    common::{
        db_connection, redis_storage,
        scenarios::{AppConfig, AppUnderTest, ScenarioApp, ScenarioWorld},
        settings,
    },
    helpers::{TestedApp, build_tested_app, local_auth_settings},
    state::{
        Account, Audit, Authorizations, Billing, Browser, Connections, EmailVerification,
        Encryption, GithubSync, GoogleDriveSync, GoogleMailSync, Integrations, Jobs, LinearTasks,
        LinkedMethod, McpOauth, McpProtocol, McpSessions, Notifications, NotionSync, NotionWebhook,
        OauthCallback, Passkeys, PasswordReset, Probe, RealtimeStream, Sessions, SetAside,
        SlackEmojis, SlackTasks, SlackThreads, SlackWebhook, Tasks, ThirdPartyItems, TickTickTasks,
        TodoistTaskChanges, TodoistTasks,
    },
    steps::set_aside_support::Scene,
};

#[derive(Default, World)]
pub struct ApiWorld {
    pub app: AppUnderTest<TestedApp>,
    pub other_app: AppUnderTest<TestedApp>,
    pub session: Option<Session>,
    pub other_session: Option<Session>,
    pub browser: Browser,
    pub account: Account,
    pub sessions: Sessions,
    pub authorizations: Authorizations,
    pub passkeys: Passkeys,
    pub probe: Probe,
    pub verification: EmailVerification,
    pub reset: PasswordReset,
    pub linked: LinkedMethod,
    pub integrations: Integrations,
    pub encryption: Encryption,
    pub jobs: Jobs,
    pub connections: Connections,
    pub tasks: Tasks,
    pub audit: Audit,
    pub billing: Billing,
    pub notifications: Notifications,
    pub set_aside: SetAside,
    pub github_sync: GithubSync,
    pub google_mail_sync: GoogleMailSync,
    pub google_drive_sync: GoogleDriveSync,
    pub linear_tasks: LinearTasks,
    pub todoist_tasks: TodoistTasks,
    pub realtime_stream: RealtimeStream,
    pub slack_tasks: SlackTasks,
    pub todoist_task_changes: TodoistTaskChanges,
    pub ticktick_tasks: TickTickTasks,
    pub slack_threads: SlackThreads,
    pub slack_webhook: SlackWebhook,
    pub oauth_callback: OauthCallback,
    pub notion_sync: NotionSync,
    pub notion_webhook: NotionWebhook,
    pub mcp_sessions: McpSessions,
    pub mcp: McpProtocol,
    pub mcp_oauth: McpOauth,
    pub slack_emojis: SlackEmojis,
    pub third_party: ThirdPartyItems,
    pub http: HttpExchanges,
}

impl fmt::Debug for ApiWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiWorld")
            .field("app", &self.app)
            .field("signed_in", &self.session.is_some())
            .field("account", &self.account)
            .field("sessions", &self.sessions)
            .field("passkeys", &self.passkeys)
            .field("http", &self.http)
            .finish()
    }
}

impl ScenarioApp for TestedApp {
    fn default_config() -> AppConfig {
        AppConfig {
            settings: settings(),
            billing: false,
            identity_provider: true,
            local_auth: None,
            local_auth_policy: None,
            blocked_domains: vec![],
        }
    }

    fn start(config: AppConfig) -> LocalBoxFuture<'static, Self> {
        let AppConfig {
            settings,
            billing,
            identity_provider,
            local_auth,
            local_auth_policy,
            blocked_domains,
        } = config;
        assert!(!billing, "Billing is not available to API scenarios yet");
        assert!(
            local_auth_policy.is_none() || local_auth.is_some(),
            "The password policy and the account limits need an app where users sign in with a password"
        );
        Box::pin(async move {
            let db = db_connection(settings.clone()).await;
            let redis = redis_storage(settings.clone()).await;
            let local_auth = local_auth.map(|local_auth| {
                (
                    local_auth_policy.unwrap_or_else(local_auth_settings),
                    local_auth.passkey,
                )
            });
            build_tested_app(
                settings,
                identity_provider,
                local_auth,
                &blocked_domains,
                db,
                redis,
            )
            .await
        })
    }

    fn stop(self) -> LocalBoxFuture<'static, ()> {
        Box::pin(async move {
            let _ = self.cache.clear(&None).await;
        })
    }
}

impl ScenarioWorld for ApiWorld {
    fn tear_down(&mut self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            self.app.stop().await;
            self.other_app.stop().await;
        })
    }
}

#[derive(Debug)]
pub struct Session {
    pub client: Client,
    pub user: User,
}

impl ApiWorld {
    pub async fn scene(&mut self) -> Scene<'_> {
        let client = self.client();
        let user = self.user().clone();
        let app = self.app.get().await;
        Scene { app, client, user }
    }

    fn session(&self) -> &Session {
        self.session
            .as_ref()
            .expect("The scenario is not signed in")
    }

    pub fn client(&self) -> Client {
        self.session().client.clone()
    }

    pub fn user(&self) -> &User {
        &self.session().user
    }

    pub fn my_email(&self) -> Pii<EmailAddress> {
        self.user()
            .email
            .clone()
            .expect("The signed-in user has no email address")
    }
}

#[derive(Debug, Default)]
pub struct HttpExchanges {
    responses: Vec<Response>,
}

#[derive(Debug)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub expires: Option<SystemTime>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site_lax: bool,
    pub path: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub cookies: Vec<Cookie>,
    pub body: Vec<u8>,
}

impl Response {
    pub async fn read(response: reqwest::Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let cookies = response
            .cookies()
            .map(|cookie| Cookie {
                name: cookie.name().to_string(),
                value: cookie.value().to_string(),
                expires: cookie.expires(),
                secure: cookie.secure(),
                http_only: cookie.http_only(),
                same_site_lax: cookie.same_site_lax(),
                path: cookie.path().map(str::to_string),
                domain: cookie.domain().map(str::to_string),
            })
            .collect();
        let body = response
            .bytes()
            .await
            .unwrap_or_else(|err| panic!("Failed to read the response body: {err}"))
            .to_vec();
        Self {
            status,
            headers,
            cookies,
            body,
        }
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).unwrap_or_else(|err| {
            panic!(
                "The response body is not the expected JSON: {err}\n{}",
                self.text()
            )
        })
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| {
            value
                .to_str()
                .unwrap_or_else(|err| panic!("The header {name} is not text: {err}"))
        })
    }
}

impl HttpExchanges {
    pub fn record(&mut self, response: Response) {
        self.responses.push(response);
    }

    pub fn last(&self) -> &Response {
        self.responses
            .last()
            .expect("No request was sent to the app in this scenario")
    }

    pub fn all(&self) -> &[Response] {
        &self.responses
    }
}
