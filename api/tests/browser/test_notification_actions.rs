use playwright_rs::{Page, expect, expect_page};
use rstest::*;

use universal_inbox::notification::{Notification, NotificationStatus};

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, generate_test_user, launch_browser,
    login, notification_id_from_url, wait_for_notification, wait_for_notification_rows,
};

/// Tells whether the server has persisted an action on a notification.
type PersistedCheck = fn(&Notification) -> bool;

/// Matches a URL targeting a single notification.
const NOTIFICATION_URL_PATTERN: &str = r"/notifications/[0-9a-fA-F-]{36}$";

/// Wait until the URL targets a notification and differs from `previous_url`.
async fn wait_for_notification_url_change(page: &Page, previous_url: &str) -> String {
    expect_page(page)
        .with_timeout(EXPECT_TIMEOUT)
        .not()
        .to_have_url(previous_url)
        .await
        .unwrap_or_else(|_| panic!("URL did not change from {previous_url}"));
    expect_page(page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_url_regex(NOTIFICATION_URL_PATTERN)
        .await
        .expect("URL should target a notification");
    page.url()
}

/// Click the `nth` row and wait until it is selected and the URL targets a notification.
async fn select_row(page: &Page, nth: usize) -> String {
    let row = page.locator(format!("#notifications-list .ui-nrow >> nth={nth}"));
    row.click(None).await.expect("click row");
    expect(row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_class_regex(r"\bselected\b")
        .await
        .expect("clicked row should be selected");
    expect_page(page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_url_regex(NOTIFICATION_URL_PATTERN)
        .await
        .expect("URL should target the selected notification");
    page.url()
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

    let url0 = select_row(&page, 0).await;
    page.locator("#notifications-list .ui-nrow >> nth=1")
        .click(None)
        .await
        .expect("click row 1");
    // Selecting a different notification must change the URL.
    let url1 = wait_for_notification_url_change(&page, &url0).await;

    page.keyboard()
        .press("ArrowDown", None)
        .await
        .expect("press ArrowDown");
    let url2 = wait_for_notification_url_change(&page, &url1).await;

    page.keyboard()
        .press("d", None)
        .await
        .expect("press d to delete");
    // Deleting the selected notification must move the URL to the new selection.
    wait_for_notification_url_change(&page, &url2).await;
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

    let deep_url = select_row(&page, 0).await;
    let notification_id = notification_id_from_url(&deep_url);

    page.keyboard().press("s", None).await.expect("press s");
    // The page reload below reads server state: wait for the snooze to be persisted.
    wait_for_notification(
        &app,
        notification_id,
        |n| n.snoozed_until.is_some(),
        "snoozed",
    )
    .await;

    page.goto(&deep_url, None).await.expect("goto deep url");

    let selected = page.locator("#notifications-list .ui-nrow.selected");
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

    let snoozed_url = select_row(&page, 0).await;
    let notification_id = notification_id_from_url(&snoozed_url);
    page.keyboard().press("s", None).await.expect("press s");
    // The Snoozed section is loaded from the server: wait for the snooze to be persisted.
    wait_for_notification(
        &app,
        notification_id,
        |n| n.snoozed_until.is_some(),
        "snoozed",
    )
    .await;

    let snoozed_link = page.locator("a[href$='/snoozed']");
    snoozed_link.click(None).await.expect("click Snoozed nav");
    wait_for_notification_rows(&page).await;

    expect_page(&page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_url_regex(NOTIFICATION_URL_PATTERN)
        .await
        .expect("Switching to a non-empty Snoozed section should select its first notification");
}

/// The delete (`d`), unsubscribe (`u`) and snooze (`s`) shortcuts each remove the
/// selected notification from the list and persist the action.
#[rstest]
#[tokio::test]
async fn test_act_on_notifications_with_keyboard(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (_context, page) = launch_browser().await;

    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    let rows = page.locator("#notifications-list .ui-nrow");
    let mut count = rows
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        count >= 3,
        "Expected at least 3 notifications to act on, but found {count}"
    );

    let actions: [(&str, &str, PersistedCheck); 3] = [
        ("d", "deleted", |n| n.status == NotificationStatus::Deleted),
        ("u", "unsubscribed", |n| {
            n.status == NotificationStatus::Unsubscribed
        }),
        ("s", "snoozed", |n| n.snoozed_until.is_some()),
    ];
    let mut url = select_row(&page, 0).await;
    for (key, what, is_persisted) in actions {
        let notification_id = notification_id_from_url(&url);

        page.keyboard()
            .press(key, None)
            .await
            .unwrap_or_else(|_| panic!("Failed to press '{key}'"));

        count -= 1;
        expect(rows.clone())
            .with_timeout(EXPECT_TIMEOUT)
            .to_have_count(count)
            .await
            .unwrap_or_else(|_| panic!("'{key}' should remove the notification from the list"));
        wait_for_notification(&app, notification_id, is_persisted, what).await;
        // The selection (and URL) moves to the next notification.
        url = wait_for_notification_url_change(&page, &url).await;
    }
}
