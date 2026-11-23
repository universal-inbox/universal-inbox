use std::{fmt, time::SystemTime};

use cucumber::World;
use futures::future::LocalBoxFuture;
use reqwest::{Client, StatusCode, header::HeaderMap};

use email_address::EmailAddress;

use universal_inbox::{pii::Pii, user::User};

use crate::{
    common::{
        db_connection, redis_storage,
        scenarios::{AppConfig, AppUnderTest, ScenarioApp, ScenarioWorld, SignInMode},
    },
    helpers::{
        TestedApp, local_auth_settings, setup_tested_app_with_local_auth, tested_app,
        tested_app_with_domain_blacklist,
    },
    state::{Account, Authorizations, Browser, LinkedMethod, Passkeys, Probe, Sessions},
};

#[derive(Default, World)]
pub struct ApiWorld {
    pub app: AppUnderTest<TestedApp>,
    /// A second instance of the app, for the scenarios that compare two instances.
    pub other_app: AppUnderTest<TestedApp>,
    pub session: Option<Session>,
    pub browser: Browser,
    pub account: Account,
    pub sessions: Sessions,
    pub authorizations: Authorizations,
    pub passkeys: Passkeys,
    pub probe: Probe,
    pub linked: LinkedMethod,
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
    fn start(config: AppConfig) -> LocalBoxFuture<'static, Self> {
        assert!(
            !config.billing,
            "Billing is not available to API scenarios yet"
        );
        let AppConfig {
            settings,
            sign_in,
            local_auth,
            ..
        } = config;
        Box::pin(async move {
            let db = db_connection(settings.clone());
            let redis = redis_storage(settings.clone());
            match sign_in {
                SignInMode::SingleSignOn => tested_app(settings, (), db, redis).await,
                SignInMode::Local { passkey } => {
                    setup_tested_app_with_local_auth(
                        settings,
                        local_auth.unwrap_or_else(local_auth_settings),
                        passkey,
                        db.await,
                        redis.await,
                    )
                    .await
                }
                SignInMode::SingleSignOnPasswordAndPasskey => {
                    tested_app_with_domain_blacklist(settings, (), db, redis).await
                }
            }
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

/// The HTTP client that holds the session of the user the scenario is signed in as.
#[derive(Debug)]
pub struct Session {
    pub client: Client,
    pub user: User,
}

impl ApiWorld {
    fn session(&self) -> &Session {
        self.session
            .as_ref()
            .expect("The scenario is not signed in")
    }

    /// The HTTP client of the user the scenario is signed in as.
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

/// The HTTP requests a scenario sent to the app, and their responses.
#[derive(Debug, Default)]
pub struct HttpExchanges {
    responses: Vec<Response>,
}

#[derive(Debug)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub expires: Option<SystemTime>,
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
