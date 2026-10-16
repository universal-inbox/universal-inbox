use std::time::Duration;

use playwright_rs::expect;
use rstest::*;

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, generate_test_user, launch_browser,
    login, wait_for_notification_rows,
};

/// Far below the 60s polling of the web application: only a pushed event refreshes the list
/// that fast.
const PUSHED_UPDATE_TIMEOUT: Duration = Duration::from_secs(3);

/// A notification deleted in a tab disappears from another tab of the same user, pushed by the
/// realtime event stream.
#[rstest]
#[tokio::test]
async fn test_notification_deleted_in_a_tab_is_removed_from_another_tab(
    #[future] browser_tested_app: BrowserTestedApp,
) {
    let app = browser_tested_app.await;
    let email = generate_test_user(&app).await;
    let (context, page) = launch_browser().await;
    login(&page, &app.app_url, &email).await;
    wait_for_notification_rows(&page).await;

    // The second tab shares the session cookie of the first one
    let other_page = context.new_page().await.expect("Failed to create page");
    other_page
        .goto(&format!("{}/", app.app_url), None)
        .await
        .expect("Failed to open the second tab");
    wait_for_notification_rows(&other_page).await;
    let other_rows = other_page.locator("#notifications-list .ui-nrow");
    let count = other_rows
        .count()
        .await
        .expect("Failed to count notification rows");
    // The second tab listens to the event stream before the first one acts
    expect(other_page.locator("body[data-realtime-connected='true']"))
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_count(1)
        .await
        .expect("The second tab should open its realtime event stream");

    let row = page.locator("#notifications-list .ui-nrow >> nth=0");
    row.click(None).await.expect("click row");
    expect(row)
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_class_regex(r"\bselected\b")
        .await
        .expect("clicked row should be selected");
    page.keyboard()
        .press("d", None)
        .await
        .expect("Failed to press 'd'");

    expect(other_rows)
        .with_timeout(PUSHED_UPDATE_TIMEOUT)
        .to_have_count(count - 1)
        .await
        .expect("The deletion should be pushed to the other tab");
}
