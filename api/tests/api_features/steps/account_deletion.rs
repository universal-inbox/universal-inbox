use std::collections::HashMap;

use cucumber::{then, when};
use pretty_assertions::assert_eq;
use reqwest::Client;
use uuid::Uuid;

use universal_inbox::{SuccessResponse, user::UserId};

use crate::{
    helpers::{
        TestedApp,
        user::{delete_current_user_response, get_current_user, get_current_user_response},
    },
    world::{ApiWorld, Response},
};

async fn user_foreign_keys(app: &TestedApp) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        r#"
            SELECT c.conrelid::regclass::text, a.attname::text
            FROM pg_constraint c
            JOIN pg_attribute a
              ON a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
            WHERE c.contype = 'f' AND c.confrelid = '"user"'::regclass
        "#,
    )
    .fetch_all(&*app.repository.pool)
    .await
    .unwrap()
}

async fn count_rows(app: &TestedApp, table: &str, column: &str, id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM {table} WHERE {column} = $1"
    )))
    .bind(id)
    .fetch_one(&*app.repository.pool)
    .await
    .unwrap()
}

async fn rows_owned_by(app: &TestedApp, user_id: UserId) -> HashMap<String, i64> {
    let mut counts = HashMap::new();
    for (table, column) in user_foreign_keys(app).await {
        let count = count_rows(app, &table, &column, user_id.0).await;
        counts.insert(format!("{table}.{column}"), count);
    }
    counts.insert(
        "user.id".to_string(),
        count_rows(app, "\"user\"", "id", user_id.0).await,
    );
    counts
}

#[then(
    "my account owns data in the tables of audit events, API tokens, integration connections, notifications, Slack bridge actions and third-party items"
)]
async fn my_account_owns_data_in_every_table(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let owned = rows_owned_by(app, user_id).await;
    for table in [
        "audit_event.user_id",
        "authentication_token.user_id",
        "integration_connection.user_id",
        "notification.user_id",
        "slack_bridge_pending_action.user_id",
        "third_party_item.user_id",
    ] {
        assert!(owned[table] > 0, "{table} should have been seeded");
    }
}

#[when("I delete my account, confirming with my email address")]
async fn i_delete_my_account_with_my_email(world: &mut ApiWorld) {
    let email = world.my_email().expose().to_string();
    delete_my_account(world, &email).await;
}

#[when(expr = "I delete my account, confirming with {string}")]
async fn i_delete_my_account_confirming_with(world: &mut ApiWorld, confirmation: String) {
    delete_my_account(world, &confirmation).await;
}

async fn delete_my_account(world: &mut ApiWorld, confirmation: &str) {
    let client = world.client();
    let app = world.app.get().await;
    let response = delete_current_user_response(&client, app, confirmation).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "a visitor who is not signed in deletes an account, confirming with {string}")]
async fn a_visitor_deletes_an_account(world: &mut ApiWorld, confirmation: String) {
    let app = world.app.get().await;
    let response = delete_current_user_response(&Client::new(), app, &confirmation).await;
    world.http.record(Response::read(response).await);
}

#[then("the app says the deletion succeeded")]
fn the_app_says_the_deletion_succeeded(world: &mut ApiWorld) {
    let body: SuccessResponse = world.http.last().json();
    assert!(body.success);
}

#[then("no data of my account is left, nor any stored credential")]
async fn no_data_of_my_account_is_left(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let owned = rows_owned_by(app, user_id).await;
    assert!(
        owned.values().all(|count| *count == 0),
        "some user data survived the account deletion: {owned:?}"
    );
    let oauth_credentials: i64 = sqlx::query_scalar("SELECT count(*) FROM oauth_credential")
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap();
    assert_eq!(oauth_credentials, 0);
}

#[then("my session no longer works")]
async fn my_session_no_longer_works(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = get_current_user_response(&client, app).await;
    assert_eq!(response.status(), 401);
}

#[then("my account is still there")]
async fn my_account_is_still_there(world: &mut ApiWorld) {
    let client = world.client();
    let expected = world.user().id;
    let app = world.app.get().await;
    assert_eq!(get_current_user(&client, app).await.id, expected);
}
