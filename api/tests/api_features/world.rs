use std::{fmt, sync::Once};

use cucumber::World;
use futures::future::LocalBoxFuture;
use reqwest::StatusCode;

use crate::{
    common::{db_connection, init_tracing, redis_storage, scenarios::ScenarioWorld, settings},
    helpers::{TestedApp, tested_app},
};

#[derive(Default, World)]
pub struct ApiWorld {
    app: Option<TestedApp>,
    pub responses: Vec<Response>,
}

pub struct Response {
    pub status: StatusCode,
    pub body: String,
}

impl fmt::Debug for ApiWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiWorld")
            .field(
                "app_address",
                &self.app.as_ref().map(|app| &app.app_address),
            )
            .field(
                "responses",
                &self
                    .responses
                    .iter()
                    .map(|response| response.status)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ApiWorld {
    /// The app under test, started with the default settings unless a step started a variant.
    pub async fn app(&mut self) -> &TestedApp {
        if self.app.is_none() {
            self.app = Some(
                tested_app(
                    settings(),
                    (),
                    db_connection(settings()),
                    redis_storage(settings()),
                )
                .await,
            );
        }
        self.app.as_ref().unwrap()
    }

    pub fn last_response(&self) -> &Response {
        self.responses
            .last()
            .expect("No request was sent to the app in this scenario")
    }
}

impl ScenarioWorld for ApiWorld {
    fn set_up(&mut self) -> LocalBoxFuture<'_, ()> {
        static TRACING: Once = Once::new();
        TRACING.call_once(|| init_tracing(&settings()));
        Box::pin(async {})
    }

    fn tear_down(&mut self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            if let Some(app) = self.app.take() {
                let _ = app.cache.clear(&None).await;
            }
        })
    }
}
