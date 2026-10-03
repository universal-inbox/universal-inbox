use rstest::*;

use crate::helpers::{
    BrowserTestedApp, browser_tested_app, launch_browser, login, navigate_and_assert, register,
    verify_user_email,
};

/// Test that a new user can register, then login and navigate all pages.
#[rstest]
#[tokio::test]
async fn test_user_can_register(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let (_context, page) = launch_browser().await;

    // Register a new user. Registration no longer auto-logs-in (email-enumeration
    // hardening): the signup page shows a generic confirmation message instead of
    // redirecting. The `register` helper asserts that confirmation is visible.
    let email = format!("browser-test+{}@test.com", uuid::Uuid::new_v4());
    register(&page, &app.app_url, &email).await;

    // The frontend gates the app behind email validation: logging in before the
    // email is verified redirects to `/verify-email`. Simulate the user clicking
    // the verification link so the subsequent login can reach the app.
    verify_user_email(&app, &email).await;

    // The user is NOT authenticated yet — they must explicitly log in with the
    // credentials they just registered. The `login` helper navigates to /login,
    // submits the form, and asserts the notifications page becomes visible.
    login(&page, &app.app_url, &email).await;

    // Verify tasks page loads via SPA navigation
    navigate_and_assert(&page, "/synced-tasks", "#tasks-page").await;

    // Verify settings page loads via SPA navigation (integration cards container)
    navigate_and_assert(&page, "/settings", "div.integration-card").await;
}
