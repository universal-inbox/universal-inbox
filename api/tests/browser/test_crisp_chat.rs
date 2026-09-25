//! Browser test for the lazy Crisp chat loading.
//!
//! The Privacy Policy presents the Crisp cookies as functional (CNIL
//! "strictly necessary for a service explicitly requested by the user"), which
//! only holds if Crisp is not loaded before the user asks for support. This
//! test boots the app with `[application.chat_support]` configured and checks
//! that:
//!
//! 1. neither the login page nor the logged-in app sends any request to
//!    `*.crisp.chat` or sets a Crisp cookie on page load;
//! 2. clicking the sidebar "Contact support" button loads Crisp with the
//!    connected user's identity (token id).
//!
//! Crisp requests are intercepted and aborted: the test never talks to the
//! real Crisp service.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use email_address::EmailAddress;
use playwright_rs::expect;
use rstest::*;
use tokio::time::{Instant, sleep};

use universal_inbox_api::configuration::{ChatSupportSettings, Settings};

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, launch_browser, login, register,
    settings, verify_user_email,
};

#[fixture]
fn settings_with_chat_support(settings: Settings) -> Settings {
    let mut settings = settings;
    settings.application.chat_support = Some(ChatSupportSettings {
        website_id: "00000000-0000-0000-0000-000000000000".to_string(),
        identity_verification_secret_key: "test-crisp-secret".to_string(),
    });
    settings
}

#[rstest]
#[tokio::test]
async fn test_crisp_chat_is_only_loaded_on_user_interaction(
    #[future]
    #[with(settings_with_chat_support(settings()))]
    browser_tested_app: BrowserTestedApp,
) {
    let app = browser_tested_app.await;
    let (context, page) = launch_browser().await;

    // Count every request the page issues to a Crisp host...
    let crisp_requests = Arc::new(AtomicUsize::new(0));
    let counter = crisp_requests.clone();
    page.on_request(move |request| {
        let counter = counter.clone();
        async move {
            if request.url().contains("crisp.chat") {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    })
    .await
    .expect("Failed to listen to page requests");
    // ...and never let them reach the real Crisp service.
    page.route("**/*.crisp.chat/**", |route| async move {
        route.abort(None).await
    })
    .await
    .expect("Failed to set up route interception for crisp.chat");

    let email = format!("crisp-{}@test.com", uuid::Uuid::new_v4());
    register(&page, &app.app_url, &email).await;
    verify_user_email(&app, &email).await;
    // `login` loads the (unauthenticated) login page, then reaches the app.
    login(&page, &app.app_url, &email).await;

    let support_button = page.locator("button[aria-label='Contact support']");
    expect(support_button.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Support button should be visible when chat support is configured");

    assert_eq!(
        crisp_requests.load(Ordering::SeqCst),
        0,
        "No request to *.crisp.chat must be sent before the user opens the chat"
    );
    let cookies = context.cookies(None).await.expect("Failed to read cookies");
    assert!(
        cookies
            .iter()
            .all(|cookie| !cookie.name.to_lowercase().contains("crisp")),
        "No Crisp cookie must be set before the user opens the chat: {cookies:?}"
    );
    let token_id = page
        .evaluate_value("window.CRISP_TOKEN_ID || ''")
        .await
        .expect("Failed to read CRISP_TOKEN_ID");
    assert_eq!(token_id, "", "Crisp must not be initialised on page load");

    support_button
        .click(None)
        .await
        .expect("Failed to click the support button");

    let deadline = Instant::now() + EXPECT_TIMEOUT;
    while crisp_requests.load(Ordering::SeqCst) == 0 {
        assert!(
            Instant::now() < deadline,
            "Clicking the support button should load the Crisp script"
        );
        sleep(std::time::Duration::from_millis(100)).await;
    }

    let user_email: EmailAddress = email.parse().expect("valid test email");
    let mut transaction = app.repository.begin().await.expect("begin transaction");
    let user = app
        .user_service
        .get_user_by_email(&mut transaction, &user_email)
        .await
        .expect("look up user")
        .expect("user exists");
    transaction.commit().await.expect("commit");

    let token_id = page
        .evaluate_value("window.CRISP_TOKEN_ID || ''")
        .await
        .expect("Failed to read CRISP_TOKEN_ID");
    assert_eq!(
        token_id,
        user.id.to_string(),
        "Crisp should be loaded with the connected user's identity"
    );
}
