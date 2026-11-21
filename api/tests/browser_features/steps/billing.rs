use cucumber::{given, then, when};
use email_address::EmailAddress;
use playwright_rs::{GotoOptions, expect};
use serde_json::json;

use universal_inbox::pii::Pii;

use crate::{common::fake_checkout_url, helpers::EXPECT_TIMEOUT, world::BrowserWorld};

#[given("billing with Stripe is enabled")]
async fn billing_with_stripe_is_enabled(world: &mut BrowserWorld) {
    world.start_app_with_billing().await;
}

#[when("I open the billing page")]
async fn i_open_the_billing_page(world: &mut BrowserWorld) {
    let (app, page) = world.app_and_page().await;
    page.goto(&format!("{}/billing", app.app_url), None)
        .await
        .expect("Failed to navigate to /billing");
}

#[then(regex = r#"^I see the "([^"]+)" with an? "([^"]+)" button$"#)]
async fn i_am_on_a_plan_with_a_button(world: &mut BrowserWorld, plan: String, button: String) {
    let page = world.page().await;
    expect(page.locator(format!("text={plan}")).first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .unwrap_or_else(|_| panic!("{plan} should be shown"));
    expect(page.locator(format!("button:has-text('{button}')")))
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .unwrap_or_else(|_| panic!("{button} button should be visible on the {plan}"));
}

#[then(expr = "there is no {string} button")]
async fn there_is_no_button(world: &mut BrowserWorld, button: String) {
    expect(
        world
            .page()
            .await
            .locator(format!("button:has-text('{button}')")),
    )
    .with_timeout(EXPECT_TIMEOUT)
    .to_be_hidden()
    .await
    .unwrap_or_else(|_| panic!("{button} button should be gone"));
}

#[when(expr = "I click the {string} button")]
async fn i_click_the_button(world: &mut BrowserWorld, button: String) {
    world
        .page()
        .await
        .locator(format!("button:has-text('{button}')"))
        .click(None)
        .await
        .unwrap_or_else(|_| panic!("Failed to click {button}"));
}

#[then("I am sent to Stripe Checkout")]
async fn i_am_sent_to_stripe_checkout(world: &mut BrowserWorld) {
    let (app, page) = world.app_and_page().await;
    let checkout_url = fake_checkout_url(&app.settings.application.front_base_url);
    page.wait_for_url(
        checkout_url.as_str(),
        Some(GotoOptions::new().timeout(EXPECT_TIMEOUT)),
    )
    .await
    .expect("Clicking Upgrade should redirect to the Stripe Checkout URL");
}

/// Stripe delivers `checkout.session.completed` to the webhook, carrying the user id as
/// metadata. The fake Stripe client parses this plain JSON envelope without a signature.
#[when("Stripe reports that my checkout is completed")]
async fn stripe_reports_that_my_checkout_is_completed(world: &mut BrowserWorld) {
    let email: Pii<EmailAddress> = world.email().parse().expect("valid test email");
    let app = world.app().await;
    let mut transaction = app.repository.begin().await.expect("begin transaction");
    let user = app
        .user_service
        .get_user_by_email(&mut transaction, &email)
        .await
        .expect("look up user")
        .expect("user exists");
    transaction.commit().await.expect("commit");

    let response = reqwest::Client::new()
        .post(format!("{}/api/billing/stripe/webhook", app.app_url))
        .header("Stripe-Signature", "t=1,v1=fake")
        .json(&json!({
            "id": "evt_test_upgrade",
            "type": "checkout.session.completed",
            "created": 1_700_000_000,
            "customer": "cus_fake_test",
            "subscription": "sub_fake_test",
            "user_id": user.id.0.to_string(),
        }))
        .send()
        .await
        .expect("Failed to POST Stripe webhook");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::NO_CONTENT,
        "webhook should be accepted and processed"
    );
}
