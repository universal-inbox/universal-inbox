use cucumber::{then, when};
use playwright_rs::expect;

use crate::{
    helpers::{EXPECT_TIMEOUT, navigate_and_assert, wait_for_notification_rows},
    world::BrowserWorld,
};

/// The link to a page of the app, and an element that shows it is displayed.
fn page_link(page_name: &str) -> (&'static str, &'static str) {
    match page_name {
        "tasks" => ("/synced-tasks", "#tasks-page"),
        "settings" => ("/settings", "div.integration-card"),
        _ => panic!("Unknown page {page_name:?}"),
    }
}

#[then("I land on my notifications, away from the login page")]
async fn i_am_taken_to_my_notifications(world: &mut BrowserWorld) {
    let page = world.page().await;
    let url = page.url();
    assert!(
        !url.contains("/login"),
        "Expected to be redirected away from /login after successful login, but URL is: {url}"
    );
    expect(page.locator("#notifications-page"))
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Notifications page not visible after login");
}

#[then(expr = "I see at least {int} notifications")]
async fn i_see_at_least_notifications(world: &mut BrowserWorld, expected: usize) {
    let page = world.page().await;
    wait_for_notification_rows(page).await;
    let count = page
        .locator("#notifications-list .ui-nrow")
        .count()
        .await
        .expect("Failed to count notification rows");
    assert!(
        count >= expected,
        "Expected at least {expected} notification rows, but found {count}"
    );
    world.list.set_row_count(count);
}

#[then("the first notification is not blank")]
async fn the_first_notification_shows_some_text(world: &mut BrowserWorld) {
    let text = world
        .page()
        .await
        .locator("#notifications-list .ui-nrow >> nth=0")
        .text_content()
        .await
        .expect("Failed to get text content of first notification row");
    assert!(
        text.as_ref().is_some_and(|text| !text.is_empty()),
        "Expected first notification row to have text content, but got: {text:?}"
    );
}

#[when("I open the settings from the sidebar")]
async fn i_open_the_settings_from_the_sidebar(world: &mut BrowserWorld) {
    // Many sync-success notification rows also link to `/settings`: use the sidebar's.
    world
        .page()
        .await
        .locator("aside.sidebar a[href='/settings']")
        .click(None)
        .await
        .expect("Failed to click settings link");
}

#[then(expr = "I see at least {int} integration cards, among them {string}")]
async fn i_see_integration_cards(world: &mut BrowserWorld, expected: usize, names: String) {
    let page = world.page().await;
    let integration_cards = page.locator("div.integration-card");
    expect(integration_cards.first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Integration cards not visible on settings page");

    let card_count = integration_cards
        .count()
        .await
        .expect("Failed to count integration cards");
    assert!(
        card_count >= expected,
        "Expected at least {expected} integration cards, but found {card_count}"
    );

    let mut all_titles_text = String::new();
    for i in 0..card_count {
        let card = page.locator(format!("div.integration-card >> nth={i}"));
        if let Ok(Some(text)) = card.text_content().await {
            all_titles_text.push_str(&text);
            all_titles_text.push(' ');
        }
    }
    for name in names.split(", ") {
        assert!(
            all_titles_text.contains(name),
            "Expected integration '{name}' in card titles, but got: {all_titles_text}"
        );
    }
}

#[then(expr = "I can open the {word} page from the navigation")]
async fn i_can_open_a_page_from_the_navigation(world: &mut BrowserWorld, page_name: String) {
    let (path, expected_selector) = page_link(&page_name);
    navigate_and_assert(world.page().await, path, expected_selector).await;
}
