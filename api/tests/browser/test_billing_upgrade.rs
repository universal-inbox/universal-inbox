//! Browser test for the Stripe billing upgrade flow (Free → Paid).
//!
//! `Plan` only has two variants (`Free`, `Paid`), so "upgrading to the paid
//! plan" is the Free → Paid transition. The app is booted with billing enabled
//! and an in-process fake Stripe client (see
//! `crate::common::build_fake_billing_service`), so the whole journey runs with
//! no real Stripe account:
//!
//! 1. A fresh user lands on `/billing` showing the **Free** plan + an "Upgrade
//!    to Paid" button.
//! 2. Clicking "Upgrade to Paid" calls `POST /api/billing/checkout-session` and
//!    redirects to the faked Stripe Checkout URL (which points back at the test
//!    server's own `/ping`, so the browser can really land on it) — this is
//!    the exact path that regressed when the checkout session lacked
//!    `customer_update[address]=auto` for `automatic_tax`, so arriving there
//!    proves that call now succeeds.
//! 3. Stripe "completes" the checkout by firing `checkout.session.completed` at
//!    the webhook, flipping the user to Paid.
//! 4. Back on `/billing`, the page now shows the **Paid** plan + "Manage
//!    billing".

use email_address::EmailAddress;
use playwright_rs::{GotoOptions, expect};
use rstest::*;
use serde_json::json;

use universal_inbox::user::UserId;

use crate::common::fake_checkout_url;
use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app_with_billing, launch_browser, login,
    register, verify_user_email,
};

/// Look up the freshly-registered user's id so the simulated webhook can carry
/// it as `metadata.user_id` (how the real handler links a checkout to a user).
async fn user_id_for(app: &BrowserTestedApp, email: &str) -> UserId {
    let email: EmailAddress = email.parse().expect("valid test email");
    let mut transaction = app.repository.begin().await.expect("begin transaction");
    let user = app
        .user_service
        .get_user_by_email(&mut transaction, &email)
        .await
        .expect("look up user")
        .expect("user exists");
    transaction.commit().await.expect("commit");
    user.id
}

/// Simulate Stripe delivering `checkout.session.completed` to the webhook. The
/// fake Stripe client parses this plain-JSON envelope (no real signature) and
/// the handler upserts the subscription as active → the user becomes Paid.
async fn deliver_checkout_completed_webhook(app: &BrowserTestedApp, user_id: UserId) {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/billing/stripe/webhook", app.app_url))
        // Any non-empty signature; the fake client doesn't verify it.
        .header("Stripe-Signature", "t=1,v1=fake")
        .json(&json!({
            "id": "evt_test_upgrade",
            "type": "checkout.session.completed",
            "created": 1_700_000_000,
            "customer": "cus_fake_test",
            "subscription": "sub_fake_test",
            "user_id": user_id.0.to_string(),
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

#[rstest]
#[tokio::test]
async fn test_user_can_upgrade_to_paid_plan(
    #[future] browser_tested_app_with_billing: BrowserTestedApp,
) {
    let app = browser_tested_app_with_billing.await;
    let (_context, page) = launch_browser().await;

    let email = format!("upgrade-{}@test.com", uuid::Uuid::new_v4());
    register(&page, &app.app_url, &email).await;
    verify_user_email(&app, &email).await;
    login(&page, &app.app_url, &email).await;

    let user_id = user_id_for(&app, &email).await;

    // 1. Billing page renders the Free plan with an Upgrade action.
    page.goto(&format!("{}/billing", app.app_url), None)
        .await
        .expect("Failed to navigate to /billing");
    expect(page.locator("text=Free plan").first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Free plan should be shown for a new user");
    let upgrade_button = page.locator("button:has-text('Upgrade to Paid')");
    expect(upgrade_button.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Upgrade to Paid button should be visible on the Free plan");

    // 2. Clicking Upgrade hits POST /checkout-session and redirects to the
    //    (faked) Stripe URL — landing there proves the checkout call worked.
    upgrade_button
        .click(None)
        .await
        .expect("Failed to click Upgrade to Paid");
    let checkout_url = fake_checkout_url(&app.settings.application.front_base_url);
    page.wait_for_url(
        checkout_url.as_str(),
        Some(GotoOptions::new().timeout(EXPECT_TIMEOUT)),
    )
    .await
    .expect("Clicking Upgrade should redirect to the Stripe Checkout URL");

    // 3. Stripe completes the checkout → webhook flips the user to Paid.
    deliver_checkout_completed_webhook(&app, user_id).await;

    // 4. Back on the billing page, the user is now on the Paid plan and the
    //    Manage-billing action has replaced the Upgrade action.
    page.goto(&format!("{}/billing", app.app_url), None)
        .await
        .expect("Failed to navigate back to /billing");
    expect(page.locator("text=Paid plan").first())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Paid plan should be shown after the upgrade webhook");
    expect(page.locator("button:has-text('Manage billing')"))
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Manage billing button should be visible once Paid");
    expect(page.locator("button:has-text('Upgrade to Paid')"))
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_hidden()
        .await
        .expect("Upgrade button should be gone once Paid");
}
