use playwright_rs::expect;
use rstest::*;

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, assert_page_loaded, browser_tested_app, launch_browser,
    register,
};

/// Test that a new user can register, then login and navigate all pages.
#[rstest]
#[tokio::test]
async fn test_user_can_register(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let (_playwright, page) = launch_browser().await;

    // Register a new user
    let email = format!("browser-test+{}@test.com", uuid::Uuid::new_v4());
    register(&page, &app.app_url, &email).await;

    // Verify redirect away from /signup (user is auto-logged-in after registration)
    let url = page.url();
    assert!(
        !url.contains("/signup"),
        "Expected to be redirected away from /signup after registration, but URL is: {url}"
    );

    // Verify the user is authenticated by navigating to the main page
    assert_page_loaded(&page, &app.app_url, "/", "#notifications-page").await;

    // Verify tasks page loads
    assert_page_loaded(&page, &app.app_url, "/synced-tasks", "#tasks-page").await;

    // Verify settings page loads (integration cards container)
    page.goto(&format!("{}/settings", app.app_url), None)
        .await
        .expect("Failed to navigate to /settings");
    let settings_cards = page.locator("div.card").await;
    expect(settings_cards.first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Settings page card not visible");

    // Verify profile page loads
    page.goto(&format!("{}/profile", app.app_url), None)
        .await
        .expect("Failed to navigate to /profile");
    let profile_content = page.locator(".p-8").await;
    expect(profile_content)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Profile page content not visible");
}

/// Test that registration fails with an invalid email.
#[rstest]
#[tokio::test]
async fn test_registration_fails_with_invalid_email(
    #[future] browser_tested_app: BrowserTestedApp,
) {
    let app = browser_tested_app.await;
    let (_playwright, page) = launch_browser().await;

    page.goto(&format!("{}/signup", app.app_url), None)
        .await
        .expect("Failed to navigate to signup page");

    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Email input not visible");

    email_input
        .fill("not-an-email", None)
        .await
        .expect("Failed to fill email");

    let password_input = page.locator("input[name='password']").await;
    password_input
        .fill("test123456", None)
        .await
        .expect("Failed to fill password");

    let submit_button = page.locator("button[type='submit']").await;
    submit_button
        .click(None)
        .await
        .expect("Failed to click submit");

    // An inline validation error should appear for the email field
    let error_message = page.locator("span.helper-text").await;
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
    let (_playwright, page) = launch_browser().await;

    page.goto(&format!("{}/signup", app.app_url), None)
        .await
        .expect("Failed to navigate to signup page");

    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Email input not visible");

    let email = format!("browser-test+{}@test.com", uuid::Uuid::new_v4());
    email_input
        .fill(&email, None)
        .await
        .expect("Failed to fill email");

    let password_input = page.locator("input[name='password']").await;
    password_input
        .fill("short", None)
        .await
        .expect("Failed to fill password");

    let submit_button = page.locator("button[type='submit']").await;
    submit_button
        .click(None)
        .await
        .expect("Failed to click submit");

    // An inline validation error should appear for the password field
    let error_message = page.locator("span.helper-text").await;
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
