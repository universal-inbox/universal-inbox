use std::time::Duration;

use playwright_rs::expect;
use rstest::*;

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, generate_test_user, launch_browser, login,
};

/// Wait until at least one notification row is visible in the DOM.
/// After login the notification list may still be loading from the API.
async fn wait_for_notification_rows(page: &playwright_rs::Page) {
    let first_row = page.locator("#notifications-list table tr.row-hover").await;
    expect(first_row.first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected at least one notification row to be visible");
}

/// Test that a logged-in user with generated data sees notifications on the main page.
#[rstest]
#[tokio::test]
async fn test_notifications_are_displayed(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_playwright, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;

    // Wait for notification rows to render (API data may still be loading after login)
    wait_for_notification_rows(&page).await;

    // After login, we should be on the notifications page with items visible.
    // The generated test user has 9 notifications.
    let notification_rows = page.locator("#notifications-list table tr.row-hover").await;
    let count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        count >= 9,
        "Expected at least 9 notification rows, but found {count}"
    );
}

/// Test that pressing 'd' on a notification deletes it.
#[rstest]
#[tokio::test]
async fn test_delete_notification_with_keyboard(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_playwright, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Count initial notifications
    let notification_rows = page.locator("#notifications-list table tr.row-hover").await;
    let initial_count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        initial_count > 0,
        "Expected at least one notification to delete, but found {initial_count}"
    );

    // Click on the first notification row to select it
    let first_row = page
        .locator("#notifications-list table tr.row-hover:first-child")
        .await;
    first_row
        .click(None)
        .await
        .expect("Failed to click first notification row");

    // Verify the row gets the row-active class (selected state)
    let active_row = page
        .locator("#notifications-list table tr.row-active")
        .await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected row-active class after clicking notification");

    // Press 'd' to delete the selected notification
    page.keyboard()
        .press("d", None)
        .await
        .expect("Failed to press 'd' key");

    // Wait for the deletion to be processed
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Count notifications after deletion
    let notification_rows_after = page.locator("#notifications-list table tr.row-hover").await;
    let count_after = notification_rows_after
        .count()
        .await
        .expect("Failed to count notification rows after deletion");
    assert_eq!(
        count_after,
        initial_count - 1,
        "Expected notification count to decrease by 1 after deletion. Before: {initial_count}, After: {count_after}"
    );
}

/// Test that pressing 'u' on a notification unsubscribes from it.
#[rstest]
#[tokio::test]
async fn test_unsubscribe_notification_with_keyboard(
    #[future] browser_tested_app: BrowserTestedApp,
) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_playwright, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Count initial notifications
    let notification_rows = page.locator("#notifications-list table tr.row-hover").await;
    let initial_count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        initial_count > 0,
        "Expected at least one notification to unsubscribe from, but found {initial_count}"
    );

    // Click on the first notification row to select it
    let first_row = page
        .locator("#notifications-list table tr.row-hover:first-child")
        .await;
    first_row
        .click(None)
        .await
        .expect("Failed to click first notification row");

    // Verify the row gets the row-active class (selected state)
    let active_row = page
        .locator("#notifications-list table tr.row-active")
        .await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected row-active class after clicking notification");

    // Press 'u' to unsubscribe from the selected notification
    page.keyboard()
        .press("u", None)
        .await
        .expect("Failed to press 'u' key");

    // Wait for the unsubscribe to be processed
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Count notifications after unsubscribe
    let notification_rows_after = page.locator("#notifications-list table tr.row-hover").await;
    let count_after = notification_rows_after
        .count()
        .await
        .expect("Failed to count notification rows after unsubscribe");
    assert_eq!(
        count_after,
        initial_count - 1,
        "Expected notification count to decrease by 1 after unsubscribe. Before: {initial_count}, After: {count_after}"
    );
}

/// Test that pressing 's' on a notification snoozes it.
#[rstest]
#[tokio::test]
async fn test_snooze_notification_with_keyboard(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_playwright, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Count initial notifications
    let notification_rows = page.locator("#notifications-list table tr.row-hover").await;
    let initial_count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        initial_count > 0,
        "Expected at least one notification to snooze, but found {initial_count}"
    );

    // Click on the first notification row to select it
    let first_row = page
        .locator("#notifications-list table tr.row-hover:first-child")
        .await;
    first_row
        .click(None)
        .await
        .expect("Failed to click first notification row");

    // Verify the row gets the row-active class (selected state)
    let active_row = page
        .locator("#notifications-list table tr.row-active")
        .await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected row-active class after clicking notification");

    // Press 's' to snooze the selected notification
    page.keyboard()
        .press("s", None)
        .await
        .expect("Failed to press 's' key");

    // Wait for the snooze to be processed
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Count notifications after snooze
    let notification_rows_after = page.locator("#notifications-list table tr.row-hover").await;
    let count_after = notification_rows_after
        .count()
        .await
        .expect("Failed to count notification rows after snooze");
    assert_eq!(
        count_after,
        initial_count - 1,
        "Expected notification count to decrease by 1 after snooze. Before: {initial_count}, After: {count_after}"
    );
}
