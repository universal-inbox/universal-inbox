use cucumber::{given, then, when};
use playwright_rs::expect;

use crate::{
    helpers::{
        EXPECT_TIMEOUT, fill_and_submit_credentials, generate_test_user, login, register,
        verify_user_email,
    },
    world::BrowserWorld,
};

#[given("I have an account with sample notifications and integrations")]
async fn i_am_a_user_with_sample_data(world: &mut BrowserWorld) {
    let email = generate_test_user(world.app().await).await;
    world.set_email(email);
}

#[when("I register with a new email address")]
async fn i_register_with_a_new_email_address(world: &mut BrowserWorld) {
    let email = format!("browser-test+{}@test.com", uuid::Uuid::new_v4());
    let (app, page) = world.app_and_page().await;
    register(page, &app.app_url, &email).await;
    world.set_email(email);
}

#[when("I verify my email address")]
async fn i_verify_my_email_address(world: &mut BrowserWorld) {
    let email = world.email();
    verify_user_email(world.app().await, &email).await;
}

#[when("I sign in from the login page")]
async fn i_sign_in_from_the_login_page(world: &mut BrowserWorld) {
    let email = world.email();
    let (app, page) = world.app_and_page().await;
    login(page, &app.app_url, &email).await;
}

#[when("I open the login page")]
async fn i_open_the_login_page(world: &mut BrowserWorld) {
    let (app, page) = world.app_and_page().await;
    page.goto(&format!("{}/login", app.app_url), None)
        .await
        .expect("Failed to navigate to login page");
}

#[when(expr = "I sign in with the password {string}")]
async fn i_sign_in_with_the_password(world: &mut BrowserWorld, password: String) {
    let email = world.email();
    fill_and_submit_credentials(world.page().await, &email, &password, "login").await;
}

#[when(expr = "I sign in with the password {string} {int} times, seeing an error each time")]
async fn i_sign_in_with_the_password_several_times(
    world: &mut BrowserWorld,
    password: String,
    attempts: usize,
) {
    let email = world.email();
    let page = world.page().await;
    for attempt in 1..=attempts {
        fill_and_submit_credentials(page, &email, &password, "login").await;
        expect(page.locator("#auth-error"))
            .with_timeout(EXPECT_TIMEOUT)
            .to_be_visible()
            .await
            .unwrap_or_else(|_| panic!("Error alert not visible after attempt {attempt}"));
    }
}

#[then("I see a sign-in error message")]
async fn i_see_a_sign_in_error_message(world: &mut BrowserWorld) {
    let error_alert = world.page().await.locator("#auth-error");
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
        error_text.as_ref().is_some_and(|text| !text.is_empty()),
        "Expected error alert to have text content, but got: {error_text:?}"
    );
}

#[then("I am still on the login page")]
async fn i_am_still_on_the_login_page(world: &mut BrowserWorld) {
    let url = world.page().await.url();
    assert!(
        url.contains("/login"),
        "Expected to remain on /login with wrong password, but URL is: {url}"
    );
}

#[then(expr = "I see the sign-in error {string}, not a raw technical error")]
async fn i_see_the_sign_in_error(world: &mut BrowserWorld, message: String) {
    let error_alert = world.page().await.locator("#auth-error");
    let message_shown = expect(error_alert.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_contain_text(&message)
        .await;
    let error_text = error_alert
        .text_content()
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    assert!(
        !error_text.to_lowercase().contains("decoding"),
        "Login lockout surfaced a raw decode error to the user: {error_text:?}"
    );
    assert!(
        message_shown.is_ok(),
        "Expected the friendly message {message:?}, but got: {error_text:?}"
    );
}

#[then("I am asked to confirm my email address")]
async fn i_am_asked_to_confirm_my_email_address(world: &mut BrowserWorld) {
    expect(world.page().await.locator("#auth-confirmation"))
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .expect("Confirmation message not visible after signup");
}
