use playwright_rs::expect;
use rstest::*;

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, fill_and_submit_credentials,
    launch_browser, login, navigate_and_assert, register, verify_user_email,
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

/// Test that registration fails with an invalid email.
#[rstest]
#[tokio::test]
async fn test_registration_fails_with_invalid_email(
    #[future] browser_tested_app: BrowserTestedApp,
) {
    let app = browser_tested_app.await;
    let (_context, page) = launch_browser().await;

    page.goto(&format!("{}/signup", app.app_url), None)
        .await
        .expect("Failed to navigate to signup page");

    fill_and_submit_credentials(&page, "not-an-email", "test123456", "signup").await;

    // An inline validation error should appear for the email field
    let error_message = page.locator("#email-error").await;
    expect(error_message.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Validation error message not visible after submitting invalid email");

    let error_text = error_message
        .text_content()
        .await
        .expect("Failed to get validation error text");
    assert!(
        error_text.is_some() && !error_text.as_ref().unwrap().is_empty(),
        "Expected validation error message to have text content, but got: {error_text:?}"
    );

    // Should still be on the signup page since the email is invalid
    let url = page.url();
    assert!(
        url.contains("/signup"),
        "Expected to remain on /signup with invalid email, but URL is: {url}"
    );
}

/// Test that registration fails with a short password.
#[rstest]
#[tokio::test]
async fn test_registration_fails_with_short_password(
    #[future] browser_tested_app: BrowserTestedApp,
) {
    let app = browser_tested_app.await;
    let (_context, page) = launch_browser().await;

    page.goto(&format!("{}/signup", app.app_url), None)
        .await
        .expect("Failed to navigate to signup page");

    let email = format!("browser-test+{}@test.com", uuid::Uuid::new_v4());
    fill_and_submit_credentials(&page, &email, "short", "signup").await;

    // An inline validation error should appear for the password field
    let error_message = page.locator("#password-error").await;
    expect(error_message.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Validation error message not visible after submitting short password");

    let error_text = error_message
        .text_content()
        .await
        .expect("Failed to get validation error text");
    assert!(
        error_text.is_some() && !error_text.as_ref().unwrap().is_empty(),
        "Expected validation error message to have text content, but got: {error_text:?}"
    );

    // Should still be on the signup page since the password is too short
    let url = page.url();
    assert!(
        url.contains("/signup"),
        "Expected to remain on /signup with short password, but URL is: {url}"
    );
}
