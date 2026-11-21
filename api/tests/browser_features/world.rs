use std::{fmt, sync::Once};

use cucumber::World;
use futures::future::LocalBoxFuture;
use playwright_rs::{BrowserContext, Page};

use crate::{
    common::{db_connection, init_tracing, redis_storage, scenarios::ScenarioWorld, settings},
    helpers::{
        BrowserTestedApp, browser_tested_app, browser_tested_app_with_billing, launch_browser,
    },
};

#[derive(Default, World)]
pub struct BrowserWorld {
    app: Option<BrowserTestedApp>,
    browser: Option<(BrowserContext, Page)>,
    pub second_tab: Option<Page>,
    email: Option<String>,
    /// Position in the list of the notification row last clicked.
    pub clicked_row: Option<usize>,
    /// Address of the notification the app last showed in the address bar.
    pub notification_url: Option<String>,
    /// Number of notification rows last counted.
    pub row_count: Option<usize>,
}

impl fmt::Debug for BrowserWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserWorld")
            .field("app_url", &self.app.as_ref().map(|app| &app.app_url))
            .field(
                "page_url",
                &self.browser.as_ref().map(|(_, page)| page.url()),
            )
            .field("email", &self.email)
            .finish()
    }
}

impl BrowserWorld {
    /// The app under test, started with the default settings unless a step started a variant.
    pub async fn app(&mut self) -> &BrowserTestedApp {
        if self.app.is_none() {
            self.app = Some(
                browser_tested_app(
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

    pub async fn start_app_with_billing(&mut self) {
        assert!(
            self.app.is_none(),
            "The app must be configured before the first step that uses it"
        );
        self.app = Some(
            browser_tested_app_with_billing(
                settings(),
                (),
                db_connection(settings()),
                redis_storage(settings()),
            )
            .await,
        );
    }

    pub async fn app_url(&mut self) -> String {
        self.app().await.app_url.clone()
    }

    /// The browser page, launched on first use.
    pub async fn page(&mut self) -> &Page {
        if self.browser.is_none() {
            self.browser = Some(launch_browser().await);
        }
        &self.browser.as_ref().unwrap().1
    }

    /// The app and the browser page together, both started on first use.
    pub async fn app_and_page(&mut self) -> (&BrowserTestedApp, &Page) {
        self.app().await;
        self.page().await;
        (
            self.app.as_ref().unwrap(),
            &self.browser.as_ref().unwrap().1,
        )
    }

    pub fn browser_context(&self) -> &BrowserContext {
        &self
            .browser
            .as_ref()
            .expect("The browser is not started yet")
            .0
    }

    pub fn notification_url(&self) -> String {
        self.notification_url
            .clone()
            .expect("No notification was shown in the address bar yet")
    }

    pub fn set_email(&mut self, email: String) {
        self.email = Some(email);
    }

    pub fn email(&self) -> String {
        self.email
            .clone()
            .expect("No user was created in this scenario")
    }
}

impl ScenarioWorld for BrowserWorld {
    fn set_up(&mut self) -> LocalBoxFuture<'_, ()> {
        static TRACING: Once = Once::new();
        TRACING.call_once(|| init_tracing(&settings()));
        Box::pin(async {})
    }

    fn tear_down(&mut self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            self.second_tab = None;
            self.browser = None;
            if let Some(app) = self.app.take() {
                let _ = app._cache.clear(&None).await;
            }
        })
    }
}
