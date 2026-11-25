use cucumber::{given, then, when};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::str::FromStr;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus,
        integrations::{
            google_mail::{GoogleMailConfig, GoogleMailContext},
            notion::{NotionConfig, NotionContext},
        },
        provider::IntegrationProvider,
    },
    notification::NotificationSourceKind,
    pii::Pii,
};
use universal_inbox_api::integrations::google_mail::GoogleMailThreadList;

use crate::{
    common::settings,
    helpers::{
        integration_connection::google_account_id,
        notification::{
            google_mail::{
                google_mail_labels_list, mock_google_mail_labels_list_service,
                mock_google_mail_threads_list_service,
            },
            sync_notifications,
        },
        notion::{
            BLOCK_ID, DISCUSSION_ID, PAGE_ID, notion_comment_event, post_signed_notion_event,
        },
    },
    steps::integration_connections::stored_connection,
    world::ApiWorld,
};

#[then(expr = "my connection is validated, and pinned to the Google account of {string}")]
async fn the_connection_is_pinned(world: &mut ApiWorld, account: String) {
    let id = world.oauth_callback.connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert_eq!(
        connection.provider_user_id,
        Some(google_account_id(&account))
    );
}

#[when("I sync my Google Mail notifications, which has no thread")]
async fn i_sync_gmail_with_no_thread(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let server = &app.google_mail_mock_server;
    mock_google_mail_labels_list_service(server, &google_mail_labels_list()).await;
    let page_size = settings().integrations["google_mail"].page_size.unwrap();
    mock_google_mail_threads_list_service(
        server,
        None,
        page_size,
        Some(vec![GoogleMailConfig::enabled().synced_label.id]),
        &GoogleMailThreadList {
            threads: None,
            result_size_estimate: 0,
            next_page_token: None,
        },
    )
    .await;
    sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::GoogleMail),
        false,
    )
    .await;
}

#[then(expr = "my Google Mail connection caches the address {string}")]
async fn the_connection_caches_the_address(world: &mut ApiWorld, address: String) {
    let id = world.oauth_callback.connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    let IntegrationProvider::GoogleMail {
        context: Some(GoogleMailContext {
            user_email_address, ..
        }),
        ..
    } = connection.provider
    else {
        panic!("Google Mail integration connection must have a context");
    };
    assert_eq!(
        user_email_address,
        Pii::<EmailAddress>::from_str(&address).unwrap()
    );
}

#[then(
    "the Google Calendar authorization asks for the scope to read the primary calendar, and the one to write events"
)]
fn the_calendar_scopes(world: &mut ApiWorld) {
    let url = world.oauth_callback.authorization_url.clone().unwrap();
    let scope = url
        .query_pairs()
        .find(|(key, _)| key == "scope")
        .expect("Missing scope in authorization URL")
        .1
        .to_string();
    let scopes: Vec<&str> = scope.split(' ').collect();
    assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.calendars.readonly"));
    assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.events"));
}

#[then("the Notion authorization asks for a user owner, with the client id and no scope")]
fn the_notion_authorization(world: &mut ApiWorld) {
    let url = world.oauth_callback.authorization_url.clone().unwrap();
    assert_eq!(
        format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap(),
            url.path()
        ),
        "https://api.notion.com/v1/oauth/authorize"
    );
    let params: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    assert!(params.contains(&("owner".to_string(), "user".to_string())));
    assert!(params.contains(&(
        "client_id".to_string(),
        settings().integrations["notion"].oauth_client_id.clone()
    )));
    assert!(!params.iter().any(|(key, _)| key == "scope"));
}

#[then(
    "my Notion connection is validated, pinned to the Notion user, and keeps the workspace and bot"
)]
async fn the_notion_connection_is_pinned(world: &mut ApiWorld) {
    let id = world.oauth_callback.connection_id.unwrap();
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert_eq!(
        connection.provider_user_id,
        Some("notion-user-id".to_string())
    );
    assert_eq!(
        connection.provider,
        IntegrationProvider::Notion {
            context: Some(NotionContext {
                workspace_id: "notion-workspace-id".to_string(),
                workspace_name: Some("Acme".to_string()),
                bot_id: "notion-bot-id".to_string(),
                user_display_name: Some(Pii::new("Jane Doe".to_string())),
                user_email_address: Some(Pii::new(
                    EmailAddress::from_str("jane.doe@example.com").unwrap()
                )),
                comments_reconciled_at: None,
            }),
            config: NotionConfig {
                sync_comments_enabled: true,
                sync_mentions_enabled: true,
            },
        }
    );
}

fn notion_event() -> Value {
    notion_comment_event(
        "notion-workspace-id",
        "comment.created",
        "comment-1",
        "notion-user-someone",
        DISCUSSION_ID,
        PAGE_ID,
        BLOCK_ID,
        &["notion-user-id"],
    )
}

#[when("Notion sends an event of that workspace")]
#[given("Notion sends an event of that workspace")]
async fn notion_sends_an_event(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    post_signed_notion_event(&client, &app.api_address, &notion_event()).await;
}
