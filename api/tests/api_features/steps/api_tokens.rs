use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode};
use secrecy::ExposeSecret;
use uuid::Uuid;

use universal_inbox::auth::auth_token::AuthenticationToken;

use crate::{
    helpers::auth::fetch_auth_tokens_for_user,
    steps::reauthentication::create_api_token,
    world::{ApiWorld, Response},
};

fn api_token(world: &ApiWorld) -> &AuthenticationToken {
    world
        .sessions
        .api_token
        .as_ref()
        .expect("The scenario has no API token")
}

async fn create(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    world.sessions.api_token = Some(create_api_token(&client, app).await.json().await.unwrap());
}

async fn get_me_with_the_api_token(world: &mut ApiWorld) -> StatusCode {
    let jwt = api_token(world).jwt_token.expose_secret().0.clone();
    let app = world.app.get().await;
    Client::new()
        .get(format!("{}users/me", app.api_address))
        .bearer_auth(jwt)
        .send()
        .await
        .unwrap()
        .status()
}

#[given("I have an API token")]
async fn i_have_an_api_token(world: &mut ApiWorld) {
    create(world).await;
}

#[when("I create an API token")]
async fn i_create_an_api_token(world: &mut ApiWorld) {
    create(world).await;
}

#[then("the API token is mine, is not a session token and is neither revoked nor expired")]
fn the_api_token_is_an_ordinary_valid_token(world: &mut ApiWorld) {
    let token = api_token(world);
    assert_eq!(token.user_id, world.user().id);
    assert!(!token.is_session_token);
    assert!(!token.is_revoked);
    assert!(!token.is_expired());
}

#[then("my account has that one API token, which remembers only the last 5 characters of it")]
async fn my_account_has_that_one_api_token(world: &mut ApiWorld) {
    let (id, raw_token) = {
        let token = api_token(world);
        (token.id.clone(), token.jwt_token.expose_secret().0.clone())
    };
    let user_id = world.user().id;
    let app = world.app.get().await;
    let stored = fetch_auth_tokens_for_user(app, user_id).await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, id);
    assert_eq!(
        stored[0].truncated_jwt_token,
        raw_token[raw_token.len() - 5..]
    );
}

#[then("the app stores only a digest of the API token, never the token itself")]
async fn the_app_stores_only_a_digest(world: &mut ApiWorld) {
    let (id, raw_token) = {
        let token = api_token(world);
        (token.id.clone(), token.jwt_token.expose_secret().0.clone())
    };
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let row_as_text: String = sqlx::query_scalar(
        "SELECT row_to_json(authentication_token)::text FROM authentication_token WHERE id = $1",
    )
    .bind(id.0)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    assert!(
        !row_as_text.contains(&raw_token),
        "the raw token must not be stored: {row_as_text}"
    );
}

#[then("the API token is accepted as a bearer token")]
async fn the_api_token_is_accepted(world: &mut ApiWorld) {
    assert_eq!(get_me_with_the_api_token(world).await, StatusCode::OK);
}

#[then("the API token is rejected as a bearer token")]
async fn the_api_token_is_rejected(world: &mut ApiWorld) {
    assert_eq!(
        get_me_with_the_api_token(world).await,
        StatusCode::UNAUTHORIZED
    );
}

#[when("I revoke my API token")]
async fn i_revoke_my_api_token(world: &mut ApiWorld) {
    let id = api_token(world).id.clone();
    revoke(world, id.to_string()).await;
}

#[when("I revoke an API token that I never created")]
async fn i_revoke_an_api_token_i_never_created(world: &mut ApiWorld) {
    revoke(world, Uuid::new_v4().to_string()).await;
}

async fn revoke(world: &mut ApiWorld, id: String) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .delete(format!(
            "{}users/me/authentication-tokens/{id}",
            app.api_address
        ))
        .send()
        .await
        .unwrap();
    world.http.record(Response::read(response).await);
}

#[then("my account marks the API token as revoked")]
async fn my_account_marks_the_api_token_as_revoked(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let stored = fetch_auth_tokens_for_user(app, user_id).await;
    assert!(stored[0].is_revoked);
}

#[when("the stored expiry of my API token passes while the token itself is still valid")]
async fn the_stored_expiry_passes(world: &mut ApiWorld) {
    let id = api_token(world).id.clone();
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    sqlx::query(
        "UPDATE authentication_token SET expire_at = now() - interval '1 day' WHERE id = $1",
    )
    .bind(id.0)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}
