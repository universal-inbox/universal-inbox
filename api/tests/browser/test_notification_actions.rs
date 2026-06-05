use playwright_rs::expect;
use rstest::*;

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, generate_test_user, launch_browser,
    login, wait_for_notification_rows,
};

/// Test that a logged-in user with generated data sees notifications on the main page.
#[rstest]
#[tokio::test]
async fn test_notifications_are_displayed(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;

    // Wait for notification rows to render (API data may still be loading after login)
    wait_for_notification_rows(&page).await;

    // After login, we should be on the notifications page with items visible.
    // The generated test user has 9 notifications.
    let notification_rows = page.locator("#notifications-list .ui-nrow").await;
    let count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        count >= 8,
        "Expected at least 8 notification rows, but found {count}"
    );
}

/// Selecting a notification (by click or keyboard) must update the URL to
/// /notifications/{id} and change it when a different notification is selected.
#[rstest]
#[tokio::test]
async fn test_selecting_notification_updates_url(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    async fn url_after_click(page: &playwright_rs::Page, nth: usize) -> String {
        let row = page
            .locator(&format!("#notifications-list .ui-nrow >> nth={nth}"))
            .await;
        row.click(None).await.expect("click row");
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        page.url()
    }

    let url0 = url_after_click(&page, 0).await;
    let url1 = url_after_click(&page, 1).await;
    assert!(
        url0.contains("/notifications/") && url1.contains("/notifications/"),
        "Both selections should produce a /notifications/<id> URL. url0={url0}, url1={url1}"
    );
    assert_ne!(
        url0, url1,
        "Selecting a different notification must change the URL. url0={url0}, url1={url1}"
    );

    // Keyboard navigation must also update the URL.
    let before_arrow = page.url();
    page.keyboard()
        .press("ArrowDown", None)
        .await
        .expect("press ArrowDown");
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let after_arrow = page.url();
    assert_ne!(
        before_arrow, after_arrow,
        "ArrowDown must change the URL. before={before_arrow}, after={after_arrow}"
    );

    // Deleting the selected notification must move the URL to the notification that
    // takes its place (the next one slides into the same index).
    let before_delete = page.url();
    page.keyboard()
        .press("d", None)
        .await
        .expect("press d to delete");
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let after_delete = page.url();
    assert!(
        after_delete.contains("/notifications/"),
        "After delete the URL should still target a notification, but is: {after_delete}"
    );
    assert_ne!(
        before_delete, after_delete,
        "Deleting the selected notification must update the URL to the new selection. \
         before={before_delete}, after={after_delete}"
    );
}

/// Deep-linking (entering a URL) to a notification that is NOT in the current section's
/// list must fetch it, switch to its section and select it — without bouncing the URL
/// to the list route.
#[rstest]
#[tokio::test]
async fn test_deeplink_other_section_notification_is_selected(
    #[future] browser_tested_app: BrowserTestedApp,
) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Select the first inbox notification and capture its URL/id.
    let first_row = page.locator("#notifications-list .ui-nrow >> nth=0").await;
    first_row.click(None).await.expect("click first row");
    let active_row = page.locator("#notifications-list .ui-nrow.selected").await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("row selected");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let deep_url = page.url();
    assert!(
        deep_url.contains("/notifications/"),
        "expected a notification URL, got {deep_url}"
    );

    // Snooze it so it leaves the inbox (now reachable only via the Snoozed section).
    page.keyboard().press("s", None).await.expect("press s");
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // Deep-link straight to that notification while the inbox is the active section.
    page.goto(&deep_url, None).await.expect("goto deep url");

    // It must end up selected (LoadAndSelect resolves its section + selects it), and the
    // URL must remain the deep link (the guard prevents bouncing to the list route).
    let selected = page.locator("#notifications-list .ui-nrow.selected").await;
    expect(selected)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("deep-linked notification should be selected");
    assert_eq!(
        page.url(),
        deep_url,
        "URL must remain the deep link (no bounce to the list route)"
    );
}

/// Switching to a section with notifications must update the URL to its selected
/// (first) notification, not leave it stale on the previous section's notification.
#[rstest]
#[tokio::test]
async fn test_switching_section_updates_url(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Snooze the first inbox notification so the Snoozed section is non-empty.
    let first_row = page.locator("#notifications-list .ui-nrow >> nth=0").await;
    first_row.click(None).await.expect("click first row");
    let active_row = page.locator("#notifications-list .ui-nrow.selected").await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("row selected");
    page.keyboard().press("s", None).await.expect("press s");
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // Switch to the Snoozed section via the sidebar link.
    let snoozed_link = page.locator("a[href$='/snoozed']").await;
    snoozed_link.click(None).await.expect("click Snoozed nav");
    wait_for_notification_rows(&page).await;

    // The URL should target the (auto-selected) snoozed notification.
    let mut url = String::new();
    for _ in 0..25 {
        url = page.url();
        if url.contains("/notifications/") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(
        url.contains("/notifications/"),
        "Switching to a non-empty Snoozed section should select its first notification, but URL is: {url}"
    );
}

/// Test that pressing 'd' on a notification deletes it.
#[rstest]
#[tokio::test]
async fn test_delete_notification_with_keyboard(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Count initial notifications
    let notification_rows = page.locator("#notifications-list .ui-nrow").await;
    let initial_count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        initial_count > 0,
        "Expected at least one notification to delete, but found {initial_count}"
    );

    // Click on the first notification row to select it
    let first_row = page.locator("#notifications-list .ui-nrow >> nth=0").await;
    first_row
        .click(None)
        .await
        .expect("Failed to click first notification row");

    // Verify the row gets the `selected` class (selected state)
    let active_row = page.locator("#notifications-list .ui-nrow.selected").await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected selected class after clicking notification");

    // Press 'd' to delete the selected notification
    page.keyboard()
        .press("d", None)
        .await
        .expect("Failed to press 'd' key");

    // Wait for the row to be removed from the DOM
    let expected_count = initial_count - 1;
    let expected_count_index = expected_count - 1;
    let remaining_rows = page
        .locator(&format!(
            "#notifications-list .ui-nrow >> nth={expected_count_index}"
        ))
        .await;
    expect(remaining_rows)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected remaining rows to be visible after deletion");

    let notification_rows_after = page.locator("#notifications-list .ui-nrow").await;
    let count_after = notification_rows_after
        .count()
        .await
        .expect("Failed to count notification rows after deletion");
    assert_eq!(
        count_after, expected_count,
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
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Count initial notifications
    let notification_rows = page.locator("#notifications-list .ui-nrow").await;
    let initial_count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        initial_count > 0,
        "Expected at least one notification to unsubscribe from, but found {initial_count}"
    );

    // Click on the first notification row to select it
    let first_row = page.locator("#notifications-list .ui-nrow >> nth=0").await;
    first_row
        .click(None)
        .await
        .expect("Failed to click first notification row");

    // Verify the row gets the `selected` class (selected state)
    let active_row = page.locator("#notifications-list .ui-nrow.selected").await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected selected class after clicking notification");

    // Press 'u' to unsubscribe from the selected notification
    page.keyboard()
        .press("u", None)
        .await
        .expect("Failed to press 'u' key");

    // Wait for the row to be removed from the DOM
    let expected_count = initial_count - 1;
    let expected_count_index = expected_count - 1;
    let remaining_rows = page
        .locator(&format!(
            "#notifications-list .ui-nrow >> nth={expected_count_index}"
        ))
        .await;
    expect(remaining_rows)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected remaining rows to be visible after unsubscribe");

    let notification_rows_after = page.locator("#notifications-list .ui-nrow").await;
    let count_after = notification_rows_after
        .count()
        .await
        .expect("Failed to count notification rows after unsubscribe");
    assert_eq!(
        count_after, expected_count,
        "Expected notification count to decrease by 1 after unsubscribe. Before: {initial_count}, After: {count_after}"
    );
}

/// Test that pressing 's' on a notification snoozes it.
#[rstest]
#[tokio::test]
async fn test_snooze_notification_with_keyboard(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // Count initial notifications
    let notification_rows = page.locator("#notifications-list .ui-nrow").await;
    let initial_count = notification_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        initial_count > 0,
        "Expected at least one notification to snooze, but found {initial_count}"
    );

    // Click on the first notification row to select it
    let first_row = page.locator("#notifications-list .ui-nrow >> nth=0").await;
    first_row
        .click(None)
        .await
        .expect("Failed to click first notification row");

    // Verify the row gets the `selected` class (selected state)
    let active_row = page.locator("#notifications-list .ui-nrow.selected").await;
    expect(active_row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected selected class after clicking notification");

    // Press 's' to snooze the selected notification
    page.keyboard()
        .press("s", None)
        .await
        .expect("Failed to press 's' key");

    // Wait for the row to be removed from the DOM
    let expected_count = initial_count - 1;
    let expected_count_index = expected_count - 1;
    let remaining_rows = page
        .locator(&format!(
            "#notifications-list .ui-nrow >> nth={expected_count_index}"
        ))
        .await;
    expect(remaining_rows)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected remaining rows to be visible after snooze");

    let notification_rows_after = page.locator("#notifications-list .ui-nrow").await;
    let count_after = notification_rows_after
        .count()
        .await
        .expect("Failed to count notification rows after snooze");
    assert_eq!(
        count_after, expected_count,
        "Expected notification count to decrease by 1 after snooze. Before: {initial_count}, After: {count_after}"
    );
}
