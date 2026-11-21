use std::time::Duration;

use cucumber::{given, then, when};
use playwright_rs::{Page, expect, expect_page};

use universal_inbox::notification::{Notification, NotificationStatus};

use crate::{
    helpers::{
        EXPECT_TIMEOUT, login, notification_id_from_url, wait_for_notification,
        wait_for_notification_rows,
    },
    world::BrowserWorld,
};

/// Matches a URL targeting a single notification.
const NOTIFICATION_URL_PATTERN: &str = r"/notifications/[0-9a-fA-F-]{36}$";

/// Far below the 60s polling of the web application: only a pushed event refreshes the list
/// that fast.
const PUSHED_UPDATE_TIMEOUT: Duration = Duration::from_secs(3);

const ROWS: &str = "#notifications-list .ui-nrow";

fn position(ordinal: &str) -> usize {
    match ordinal {
        "first" => 0,
        "second" => 1,
        _ => panic!("Unknown position {ordinal:?}"),
    }
}

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

#[given("I am signed in with my notifications listed")]
async fn i_am_signed_in_with_my_notifications_listed(world: &mut BrowserWorld) {
    let email = world.email();
    let (app, page) = world.app_and_page().await;
    login(page, &app.app_url, &email).await;
    wait_for_notification_rows(page).await;
}

#[given(expr = "I have at least {int} notifications listed")]
async fn i_have_at_least_notifications_listed(world: &mut BrowserWorld, expected: usize) {
    let page = world.page().await;
    wait_for_notification_rows(page).await;
    let count = page
        .locator(ROWS)
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        count >= expected,
        "Expected at least {expected} notifications to act on, but found {count}"
    );
    world.row_count = Some(count);
}

#[when(regex = r"^I click the (first|second) notification$")]
async fn i_click_a_notification(world: &mut BrowserWorld, ordinal: String) {
    let nth = position(&ordinal);
    world
        .page()
        .await
        .locator(format!("{ROWS} >> nth={nth}"))
        .click(None)
        .await
        .unwrap_or_else(|_| panic!("Failed to click notification row {nth}"));
    world.clicked_row = Some(nth);
}

#[then("a notification is selected")]
async fn a_notification_is_selected(world: &mut BrowserWorld) {
    expect(world.page().await.locator(format!("{ROWS}.selected")))
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Expected a selected notification row");
}

#[then("it is selected")]
async fn it_is_selected(world: &mut BrowserWorld) {
    let nth = world.clicked_row.expect("No notification was clicked");
    expect(world.page().await.locator(format!("{ROWS} >> nth={nth}")))
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_class_regex(r"\bselected\b")
        .await
        .expect("clicked row should be selected");
}

#[then("it is selected and the address bar shows it")]
async fn it_is_selected_and_the_address_bar_shows_it(world: &mut BrowserWorld) {
    it_is_selected(world).await;
    let page = world.page().await;
    expect_page(page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_url_regex(NOTIFICATION_URL_PATTERN)
        .await
        .expect("URL should target the selected notification");
    world.notification_url = Some(page.url());
}

#[then("the address bar moves to another notification")]
async fn the_address_bar_moves_to_another_notification(world: &mut BrowserWorld) {
    let previous_url = world.notification_url();
    let url = wait_for_notification_url_change(world.page().await, &previous_url).await;
    world.notification_url = Some(url);
}

#[then("the address bar shows a notification")]
async fn the_address_bar_shows_a_notification(world: &mut BrowserWorld) {
    expect_page(world.page().await)
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_url_regex(NOTIFICATION_URL_PATTERN)
        .await
        .expect("Switching to a non-empty section should select its first notification");
}

#[then("the address bar still shows its address")]
async fn the_address_bar_still_shows_its_address(world: &mut BrowserWorld) {
    let deep_url = world.notification_url();
    assert_eq!(
        world.page().await.url(),
        deep_url,
        "URL must remain the deep link (no bounce to the list route)"
    );
}

#[when(
    regex = r#"^I press "([^"]+)" to (delete it|unsubscribe from it|snooze it|select the next one)$"#
)]
async fn i_press(world: &mut BrowserWorld, key: String, intent: String) {
    let shortcut = match intent.as_str() {
        "delete it" => "d",
        "unsubscribe from it" => "u",
        "snooze it" => "s",
        "select the next one" => "ArrowDown",
        _ => unreachable!(),
    };
    assert_eq!(key, shortcut, "The shortcut to {intent} is {shortcut:?}");
    world
        .page()
        .await
        .keyboard()
        .press(&key, None)
        .await
        .unwrap_or_else(|_| panic!("Failed to press '{key}'"));
}

#[then("it disappears from the list")]
async fn it_disappears_from_the_list(world: &mut BrowserWorld) {
    let count = world.row_count.expect("The notifications were not counted") - 1;
    expect(world.page().await.locator(ROWS))
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_count(count)
        .await
        .expect("The action should remove the notification from the list");
    world.row_count = Some(count);
}

#[then(regex = r"^the server has (deleted|unsubscribed from|snoozed) it$")]
async fn the_server_has_persisted_the_action(world: &mut BrowserWorld, action: String) {
    let (what, is_persisted): (&str, fn(&Notification) -> bool) = match action.as_str() {
        "deleted" => ("deleted", |n| n.status == NotificationStatus::Deleted),
        "unsubscribed from" => ("unsubscribed", |n| {
            n.status == NotificationStatus::Unsubscribed
        }),
        "snoozed" => ("snoozed", |n| n.snoozed_until.is_some()),
        _ => unreachable!(),
    };
    let notification_id = notification_id_from_url(&world.notification_url());
    wait_for_notification(world.app().await, notification_id, is_persisted, what).await;
}

#[when("I open its address directly")]
async fn i_open_its_address_directly(world: &mut BrowserWorld) {
    let deep_url = world.notification_url();
    world
        .page()
        .await
        .goto(&deep_url, None)
        .await
        .expect("goto deep url");
}

#[when("I open the Snoozed section")]
async fn i_open_the_snoozed_section(world: &mut BrowserWorld) {
    let page = world.page().await;
    page.locator("a[href$='/snoozed']")
        .click(None)
        .await
        .expect("click Snoozed nav");
    wait_for_notification_rows(page).await;
}

#[when("I open my inbox in a second tab")]
async fn i_open_my_inbox_in_a_second_tab(world: &mut BrowserWorld) {
    let app_url = world.app_url().await;
    let other_page = world
        .browser_context()
        .new_page()
        .await
        .expect("Failed to create page");
    other_page
        .goto(&format!("{app_url}/"), None)
        .await
        .expect("Failed to open the second tab");
    world.second_tab = Some(other_page);
}

#[then("the second tab lists my notifications and is connected for live updates")]
async fn the_second_tab_listens_to_realtime_updates(world: &mut BrowserWorld) {
    let other_page = world.second_tab.as_ref().expect("No second tab is open");
    wait_for_notification_rows(other_page).await;
    let count = other_page
        .locator(ROWS)
        .count()
        .await
        .expect("Failed to count notification rows");
    expect(other_page.locator("body[data-realtime-connected='true']"))
        .with_timeout(EXPECT_TIMEOUT)
        .to_have_count(1)
        .await
        .expect("The second tab should open its realtime event stream");
    world.row_count = Some(count);
}

#[then("the second tab shows one fewer notification within 3 seconds")]
async fn the_second_tab_shows_one_notification_less(world: &mut BrowserWorld) {
    let count = world.row_count.expect("The notifications were not counted");
    let other_page = world.second_tab.as_ref().expect("No second tab is open");
    expect(other_page.locator(ROWS))
        .with_timeout(PUSHED_UPDATE_TIMEOUT)
        .to_have_count(count - 1)
        .await
        .expect("The deletion should be pushed to the other tab");
}
