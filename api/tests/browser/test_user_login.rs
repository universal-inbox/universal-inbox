use playwright_rs::expect;
use rstest::*;

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, generate_test_user, launch_browser,
    login, wait_for_notification_rows,
};

/// Test that a generated test user can log in, see notifications, interact with them,
/// and verify the settings page shows connected integrations.
#[rstest]
#[tokio::test]
async fn test_user_can_login(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;

    // Part A: Verify redirect to notifications page
    let url = page.url();
    assert!(
        !url.contains("/login"),
        "Expected to be redirected away from /login after successful login, but URL is: {url}"
    );

    // Verify notifications page is visible
    let notifications_page = page.locator("#notifications-page").await;
    expect(notifications_page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Notifications page not visible after login");

    // Wait for notification rows to render (API data may still be loading after login)
    wait_for_notification_rows(&page).await;

    // Count notification rows — test user has 9 notifications
    let notification_rows = page.locator("#notifications-list table tr.row-hover").await;
    let count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        count >= 9,
        "Expected at least 9 notification rows, but found {count}"
    );

    // Check that at least one notification row has text content
    let first_row = page
        .locator("#notifications-list table tr.row-hover:first-child")
        .await;
    let text = first_row
        .text_content()
        .await
        .expect("Failed to get text content of first notification row");
    assert!(
        text.is_some() && !text.as_ref().unwrap().is_empty(),
        "Expected first notification row to have text content, but got: {text:?}"
    );

    // Click a notification row and verify it gets the row-active class
    first_row
        .click(None)
        .await
        .expect("Failed to click first notification row");
    let active_row = page
        .locator("#notifications-list table tr.row-active")
        .await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected a row-active row after clicking a notification");

    // Part B: Navigate to settings page via SPA link (avoids full page reload
    // which would re-download the ~74 MB debug WASM binary)
    let settings_link = page.locator("a[href='/settings'].btn.btn-square").await;
    settings_link
        .click(None)
        .await
        .expect("Failed to click settings link");

    // Verify integration cards are visible (use .first() to satisfy strict mode)
    let integration_cards = page.locator("div.card.bg-base-200").await;
    expect(integration_cards.first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Integration cards not visible on settings page");

    // Count integration cards — should be >= 7
    let card_count = integration_cards
        .count()
        .await
        .expect("Failed to count integration cards");
    assert!(
        card_count >= 7,
        "Expected at least 7 integration cards, but found {card_count}"
    );

    // Collect all card texts to check for expected integration names.
    // Use Playwright's `>> nth=` syntax (filters by locator matches) instead of
    // CSS `:nth-child()` (counts all DOM siblings including non-card dividers).
    let mut all_titles_text = String::new();
    for i in 0..card_count {
        let card = page
            .locator(&format!("div.card.bg-base-200 >> nth={i}"))
            .await;
        if let Ok(Some(text)) = card.text_content().await {
            all_titles_text.push_str(&text);
            all_titles_text.push(' ');
        }
    }
    let expected_integrations = ["Github", "Linear", "Slack", "Google Mail"];
    for name in &expected_integrations {
        assert!(
            all_titles_text.contains(name),
            "Expected integration '{name}' in card titles, but got: {all_titles_text}"
        );
    }
}

/// Test that login fails with wrong password.
#[rstest]
#[tokio::test]
async fn test_login_fails_with_wrong_password(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    page.goto(&format!("{}/login", app.app_url), None)
        .await
        .expect("Failed to navigate to login page");

    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Email input not visible");

    email_input
        .fill(&email, None)
        .await
        .expect("Failed to fill email");

    let password_input = page.locator("input[name='password']").await;
    password_input
        .fill("wrong_password", None)
        .await
        .expect("Failed to fill password");

    let submit_button = page.locator("button[type='submit']").await;
    submit_button
        .click(None)
        .await
        .expect("Failed to click submit");

    // An error alert should appear on the login page
    let error_alert = page.locator("div.alert.alert-error").await;
    expect(error_alert.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Error alert not visible after wrong password");

    let error_text = error_alert
        .text_content()
        .await
        .expect("Failed to get error alert text");
    assert!(
        error_text.is_some() && !error_text.as_ref().unwrap().is_empty(),
        "Expected error alert to have text content, but got: {error_text:?}"
    );

    // Should still be on the login page
    let url = page.url();
    assert!(
        url.contains("/login"),
        "Expected to remain on /login with wrong password, but URL is: {url}"
    );
}

/// Test that login fails with non-existent user.
#[rstest]
#[tokio::test]
async fn test_login_fails_with_nonexistent_user(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let (_context, page) = launch_browser().await;

    page.goto(&format!("{}/login", app.app_url), None)
        .await
        .expect("Failed to navigate to login page");

    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Email input not visible");

    email_input
        .fill("nonexistent@test.com", None)
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

    // An error alert should appear on the login page
    let error_alert = page.locator("div.alert.alert-error").await;
    expect(error_alert.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Error alert not visible after nonexistent user login");

    let error_text = error_alert
        .text_content()
        .await
        .expect("Failed to get error alert text");
    assert!(
        error_text.is_some() && !error_text.as_ref().unwrap().is_empty(),
        "Expected error alert to have text content, but got: {error_text:?}"
    );

    // Should still be on the login page
    let url = page.url();
    assert!(
        url.contains("/login"),
        "Expected to remain on /login with nonexistent user, but URL is: {url}"
    );
}
