use cucumber::World;
use futures::future::LocalBoxFuture;
use reqwest::StatusCode;

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

/// The HTTP requests a scenario sent to the app, and their responses.
#[derive(Debug, Default)]
pub struct HttpExchanges {
    responses: Vec<Response>,
}

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub body: String,
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
