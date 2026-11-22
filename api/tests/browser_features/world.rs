use std::fmt;

use cucumber::World;
use futures::future::LocalBoxFuture;
use playwright_rs::{BrowserContext, Page};

use crate::{
    common::{
        db_connection, redis_storage,
        scenarios::{AppConfig, AppUnderTest, ScenarioApp, ScenarioWorld},
    },
    helpers::{
        BrowserTestedApp, browser_tested_app, browser_tested_app_with_billing, launch_browser,
    },
};

#[derive(Default, World)]
pub struct BrowserWorld {
    pub app: AppUnderTest<BrowserTestedApp>,
    browser: Option<(BrowserContext, Page)>,
    pub account: Account,
    pub list: NotificationList,
}

impl fmt::Debug for BrowserWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserWorld")
            .field("app", &self.app)
            .field(
                "page_url",
                &self.browser.as_ref().map(|(_, page)| page.url()),
            )
            .field("account", &self.account)
            .field("list", &self.list)
            .finish()
    }
}

impl ScenarioApp for BrowserTestedApp {
    fn start(config: AppConfig) -> LocalBoxFuture<'static, Self> {
        let AppConfig {
            settings, billing, ..
        } = config;
        Box::pin(async move {
            let db = db_connection(settings.clone());
            let redis = redis_storage(settings.clone());
            if billing {
                browser_tested_app_with_billing(settings, (), db, redis).await
            } else {
                browser_tested_app(settings, (), db, redis).await
            }
        })
    }

    fn stop(self) -> LocalBoxFuture<'static, ()> {
        Box::pin(async move {
            let _ = self._cache.clear(&None).await;
        })
    }
}

impl ScenarioWorld for BrowserWorld {
    fn tear_down(&mut self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            self.list.second_tab = None;
            self.browser = None;
            self.app.stop().await;
        })
    }
}

impl BrowserWorld {
    pub async fn app_url(&mut self) -> String {
        self.app.get().await.app_url.clone()
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
        self.page().await;
        let app = self.app.get().await;
        (app, &self.browser.as_ref().unwrap().1)
    }

    pub fn browser_context(&self) -> &BrowserContext {
        &self
            .browser
            .as_ref()
            .expect("The browser is not started yet")
            .0
    }
}

/// The account the scenario signs in with.
#[derive(Debug, Default)]
pub struct Account {
    email: Option<String>,
}

impl Account {
    pub fn set_email(&mut self, email: String) {
        self.email = Some(email);
    }

    pub fn email(&self) -> String {
        self.email
            .clone()
            .expect("No account was created in this scenario")
    }
}

/// What the scenario observed of the notification list.
#[derive(Debug, Default)]
pub struct NotificationList {
    clicked_row: Option<usize>,
    notification_url: Option<String>,
    row_count: Option<usize>,
    pub second_tab: Option<Page>,
}

impl NotificationList {
    pub fn set_clicked_row(&mut self, row: usize) {
        self.clicked_row = Some(row);
    }

    pub fn clicked_row(&self) -> usize {
        self.clicked_row.expect("No notification was clicked")
    }

    pub fn set_notification_url(&mut self, url: String) {
        self.notification_url = Some(url);
    }

    pub fn notification_url(&self) -> String {
        self.notification_url
            .clone()
            .expect("No notification was shown in the address bar yet")
    }

    pub fn set_row_count(&mut self, count: usize) {
        self.row_count = Some(count);
    }

    pub fn row_count(&self) -> usize {
        self.row_count.expect("The notifications were not counted")
    }

    pub fn second_tab(&self) -> &Page {
        self.second_tab.as_ref().expect("No second tab is open")
    }
}
