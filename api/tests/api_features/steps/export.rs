use cucumber::{then, when};
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, header};
use secrecy::ExposeSecret;
use serde_json::Value;

use crate::{
    helpers::{integration_connection::github_oauth_credential, user::export_user_data_response},
    world::{ApiWorld, Response},
};

fn ids(export: &Value, field: &str) -> Vec<String> {
    export[field]
        .as_array()
        .unwrap_or_else(|| panic!("`{field}` should be an array"))
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

#[when("I export my data")]
async fn i_export_my_data(world: &mut ApiWorld) {
    let client = world.client();
    let api_address = world.app.get().await.api_address.clone();
    let response = export_user_data_response(&client, &api_address).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "I export my data {int} times, each answered 200")]
async fn i_export_my_data_several_times(world: &mut ApiWorld, count: usize) {
    let client = world.client();
    let api_address = world.app.get().await.api_address.clone();
    for i in 0..count {
        let response = export_user_data_response(&client, &api_address).await;
        assert_eq!(response.status(), StatusCode::OK, "request {i}");
    }
}

#[when("a visitor who is not signed in exports their data")]
async fn a_visitor_exports_their_data(world: &mut ApiWorld) {
    let api_address = world.app.get().await.api_address.clone();
    let response = export_user_data_response(&Client::new(), &api_address).await;
    world.http.record(Response::read(response).await);
}

#[then(expr = "the response is a file download whose name starts with {string}")]
fn the_response_is_a_file_download(world: &mut ApiWorld, prefix: String) {
    let disposition = world
        .http
        .last()
        .header(header::CONTENT_DISPOSITION.as_str())
        .expect("No Content-Disposition header")
        .to_string();
    assert!(
        disposition.starts_with(&format!("attachment; filename=\"{prefix}")),
        "unexpected Content-Disposition: {disposition}"
    );
}

#[then("the export contains neither my API token nor the GitHub access token")]
fn the_export_contains_no_secret(world: &mut ApiWorld) {
    let body = world.http.last().text();
    let jwt = world
        .sessions
        .api_token
        .as_ref()
        .expect("The scenario has no API token")
        .jwt_token
        .expose_secret()
        .0
        .clone();
    assert!(!body.contains(&jwt), "the full API token leaked");
    assert!(
        !body.contains(&github_oauth_credential().access_token.0),
        "the OAuth access token leaked"
    );
}

#[then("the export is about me and has no chat support email signature")]
fn the_export_is_about_me(world: &mut ApiWorld) {
    let export: Value = world.http.last().json();
    assert_eq!(export["user"]["id"], world.user().id.to_string());
    assert!(export["user"]["chat_support_email_signature"].is_null());
}

#[then("the export lists exactly my API token")]
fn the_export_lists_my_api_token(world: &mut ApiWorld) {
    let export: Value = world.http.last().json();
    let token_id = world.sessions.api_token.as_ref().unwrap().id.to_string();
    assert_eq!(ids(&export, "authentication_tokens"), vec![token_id]);
}

#[then(
    "the export lists exactly my GitHub, Todoist and Linear connections and none of the other user's"
)]
fn the_export_lists_my_connections(world: &mut ApiWorld) {
    let export: Value = world.http.last().json();
    let integrations = &world.integrations;
    let mut expected = vec![
        integrations
            .github_connection
            .as_ref()
            .unwrap()
            .id
            .to_string(),
        integrations
            .todoist_connection
            .as_ref()
            .unwrap()
            .id
            .to_string(),
        integrations
            .linear_connection
            .as_ref()
            .unwrap()
            .id
            .to_string(),
    ];
    expected.sort();
    let mut listed = ids(&export, "integration_connections");
    listed.sort();
    assert_eq!(listed, expected);
    let other = integrations
        .other_github_connection
        .as_ref()
        .unwrap()
        .id
        .to_string();
    assert!(!listed.contains(&other));
}

#[then("the export lists exactly my notification")]
fn the_export_lists_my_notification(world: &mut ApiWorld) {
    let export: Value = world.http.last().json();
    let id = world
        .integrations
        .github_notification
        .as_ref()
        .unwrap()
        .id
        .to_string();
    assert_eq!(ids(&export, "notifications"), vec![id]);
}

#[then("the export lists exactly my task")]
fn the_export_lists_my_task(world: &mut ApiWorld) {
    let export: Value = world.http.last().json();
    let id = world
        .integrations
        .linear_task
        .as_ref()
        .unwrap()
        .id
        .to_string();
    assert_eq!(ids(&export, "tasks"), vec![id]);
}

#[when("I export my data once more")]
async fn i_export_my_data_once_more(world: &mut ApiWorld) {
    i_export_my_data(world).await;
}
