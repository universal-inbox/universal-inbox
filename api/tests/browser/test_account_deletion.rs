//! Browser test for self-service account deletion from the Profile page.

use email_address::EmailAddress;
use playwright_rs::expect;
use rstest::*;

use crate::helpers::{
    BrowserTestedApp, EXPECT_TIMEOUT, browser_tested_app, fill_and_verify, launch_browser, login,
    navigate_and_assert, register, verify_user_email,
};

#[rstest]
#[tokio::test]
async fn test_user_can_delete_their_account(#[future] browser_tested_app: BrowserTestedApp) {
    let app = browser_tested_app.await;
    let (_context, page) = launch_browser().await;

    let email = format!("delete-account-{}@test.com", uuid::Uuid::new_v4());
    register(&page, &app.app_url, &email).await;
    verify_user_email(&app, &email).await;
    login(&page, &app.app_url, &email).await;

    navigate_and_assert(&page, "/profile", "#delete-account-card").await;

    // The destructive button stays disabled until the email is re-typed.
    let delete_button = page.locator("#delete-account-button");
    expect(delete_button.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_disabled()
        .await
        .expect("Delete button should be disabled before confirmation");

    let confirmation_input = page.locator("#deleteAccountConfirmation");
    fill_and_verify(&confirmation_input, &email, "account deletion confirmation").await;
    expect(delete_button.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_enabled()
        .await
        .expect("Delete button should be enabled once the email is typed");

    delete_button
        .click(None)
        .await
        .expect("Failed to click the delete account button");

    let confirmation = page.locator("#auth-confirmation");
    expect(confirmation)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("A confirmation should be displayed once the account is deleted");

    let user_email: EmailAddress = email.parse().expect("valid test email");
    let mut transaction = app.repository.begin().await.expect("begin transaction");
    let user = app
        .user_service
        .get_user_by_email(&mut transaction, &user_email)
        .await
        .expect("look up user");
    transaction.commit().await.expect("commit");
    assert!(user.is_none(), "the user should have been deleted");
}
