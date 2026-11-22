use cucumber::World;
use futures::future::LocalBoxFuture;
use reqwest::{Client, StatusCode, header::HeaderMap};

use crate::{
    common::{
        db_connection, redis_storage,
        scenarios::{AppConfig, AppUnderTest, ScenarioApp, ScenarioWorld},
    },
    helpers::{TestedApp, tested_app},
};

#[derive(Debug, Default, World)]
pub struct ApiWorld {
    pub app: AppUnderTest<TestedApp>,
    pub session: Option<Session>,
    pub http: HttpExchanges,
}

impl ScenarioApp for TestedApp {
    fn start(config: AppConfig) -> LocalBoxFuture<'static, Self> {
        assert!(
            !config.billing,
            "Billing is not available to API scenarios yet"
        );
        let AppConfig { settings, .. } = config;
        Box::pin(async move {
            let db_settings = settings.clone();
            tested_app(
                settings,
                (),
                db_connection(db_settings.clone()),
                redis_storage(db_settings),
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
        Box::pin(self.app.stop())
    }
}

/// The HTTP client that holds the session of the user the scenario is signed in as.
#[derive(Debug)]
pub struct Session {
    pub client: Client,
}

/// The HTTP requests a scenario sent to the app, and their responses.
#[derive(Debug, Default)]
pub struct HttpExchanges {
    responses: Vec<Response>,
}

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Response {
    pub async fn read(response: reqwest::Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .unwrap_or_else(|err| panic!("Failed to read the response body: {err}"))
            .to_vec();
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
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
