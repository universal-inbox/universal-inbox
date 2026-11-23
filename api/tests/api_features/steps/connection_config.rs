use std::str::FromStr;

use chrono::Utc;
use cucumber::{given, then, when};
use email_address::EmailAddress;
use http::StatusCode;
use pretty_assertions::assert_eq;
use slack_morphism::prelude::SlackReactionName;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{
            google_mail::{GoogleMailConfig, GoogleMailContext},
            slack::{SlackConfig, SlackReactionConfig},
        },
        provider::{IntegrationConnectionContext, IntegrationProvider},
    },
    notification::Notification,
    pii::Pii,
    third_party::integrations::google_mail::GoogleMailLabel,
};
use universal_inbox_api::repository::{
    integration_connection::IntegrationConnectionRepository,
    oauth_credential::OAuthCredentialRepository,
};

use crate::{
    helpers::{
        integration_connection::create_integration_connection,
        notification::{
            google_mail::{
                create_notification_from_google_mail_thread, google_mail_thread_get_123,
                google_mail_user_profile, raw_google_mail_thread_get_123,
            },
            list_notifications,
        },
    },
    steps::integration_connections::stored_connection,
    world::{ApiWorld, Response},
};

fn label(name: &str) -> GoogleMailLabel {
    GoogleMailLabel {
        id: name.replace(' ', "_"),
        name: name.to_string(),
    }
}

fn mail_config(label_name: &str, sync_notifications_enabled: bool) -> IntegrationConnectionConfig {
    IntegrationConnectionConfig::GoogleMail(GoogleMailConfig {
        sync_notifications_enabled,
        synced_label: label(label_name),
    })
}

async fn have_google_mail_connection(
    world: &mut ApiWorld,
    label_name: &str,
    context: Option<IntegrationConnectionContext>,
) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
        app,
        user_id,
        mail_config(label_name, true),
        IntegrationConnectionStatus::Validated,
        context,
        None,
        None,
        None,
        None,
    )
    .await;
    world.connections.mail = Some(connection);
}

#[given(expr = "I have a validated Google Mail connection that synchronises the label {string}")]
async fn i_have_a_google_mail_connection(world: &mut ApiWorld, label_name: String) {
    have_google_mail_connection(world, &label_name, None).await;
}

#[given(
    expr = "I have a validated Google Mail connection for the address {string} that synchronises the label {string}"
)]
async fn i_have_a_google_mail_connection_with_an_address(
    world: &mut ApiWorld,
    address: String,
    label_name: String,
) {
    let context = IntegrationConnectionContext::GoogleMail(GoogleMailContext {
        user_email_address: Pii::<EmailAddress>::from_str(&address).unwrap(),
        labels: vec![],
    });
    have_google_mail_connection(world, &label_name, Some(context)).await;
}

#[given(expr = "I have a notification called {string} from a thread of my Google Mail connection")]
async fn i_have_a_notification_from_google_mail(world: &mut ApiWorld, name: String) {
    let user_id = world.user().id;
    let connection_id = world.connections.mail.as_ref().unwrap().id;
    let thread =
        google_mail_thread_get_123(raw_google_mail_thread_get_123(), google_mail_user_profile());
    let app = world.app.get().await;
    let notification =
        create_notification_from_google_mail_thread(app, &thread, user_id, connection_id).await;
    world.notifications.by_name.insert(name, *notification);
}

#[given("I have a validated Slack connection whose every sync is off, paused by my plan")]
async fn i_have_a_slack_connection_paused_by_my_plan(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::default()),
        IntegrationConnectionStatus::Validated,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    let paused_config = IntegrationConnectionConfig::Slack(SlackConfig::default());
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .set_integration_connection_plan_pause(
            &mut transaction,
            connection.id,
            Some(Utc::now()),
            Some(&paused_config),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.connections.slack = Some(connection);
}

async fn put_config(
    world: &mut ApiWorld,
    client: reqwest::Client,
    connection_id: impl std::fmt::Display,
    config: &IntegrationConnectionConfig,
) -> reqwest::Response {
    let app = world.app.get().await;
    client
        .put(format!(
            "{}integration-connections/{connection_id}/config",
            app.api_address
        ))
        .json(config)
        .send()
        .await
        .expect("Failed to execute request")
}

#[when(expr = "I change the synchronised label of my Google Mail connection to {string}")]
async fn i_change_the_label(world: &mut ApiWorld, label_name: String) {
    let id = world.connections.mail.as_ref().unwrap().id;
    let client = world.client();
    let response = put_config(world, client, id, &mail_config(&label_name, true)).await;
    world.connections.saved_config =
        Some(response.json().await.expect("Failed to parse JSON result"));
}

#[when(
    "that other user switches off the notifications of my Google Mail connection and of a connection that does not exist"
)]
async fn that_other_user_saves_a_configuration(world: &mut ApiWorld) {
    let id = world.connections.mail.as_ref().unwrap().id;
    let unknown_id = uuid::Uuid::new_v4();
    world.connections.unknown_id = Some(unknown_id);
    let client = world.other_session.as_ref().unwrap().client.clone();
    let config = mail_config("Label 2", false);
    let foreign = put_config(world, client.clone(), id, &config).await;
    world.http.record(Response::read(foreign).await);
    let missing = put_config(world, client, unknown_id, &config).await;
    world.http.record(Response::read(missing).await);
}

#[when("I turn on the reaction sync of my Slack connection, leaving the message sync off")]
async fn i_turn_on_the_reaction_sync(world: &mut ApiWorld) {
    let id = world.connections.slack.as_ref().unwrap().id;
    let client = world.client();
    let config = IntegrationConnectionConfig::Slack(SlackConfig {
        reaction_config: SlackReactionConfig {
            sync_enabled: true,
            ..SlackConfig::default().reaction_config
        },
        ..SlackConfig::default()
    });
    let response = put_config(world, client, id, &config).await;
    world.http.record(Response::read(response).await);
}

#[when("I change the reaction of my Slack connection to \"bookmark\", leaving every sync off")]
async fn i_change_the_reaction(world: &mut ApiWorld) {
    let id = world.connections.slack.as_ref().unwrap().id;
    let client = world.client();
    let config = IntegrationConnectionConfig::Slack(SlackConfig {
        reaction_config: SlackReactionConfig {
            reaction_name: SlackReactionName("bookmark".to_string()),
            ..SlackConfig::default().reaction_config
        },
        ..SlackConfig::default()
    });
    let response = put_config(world, client, id, &config).await;
    world.http.record(Response::read(response).await);
}

#[then(expr = "the saved configuration synchronises the label {string}")]
fn the_saved_configuration_synchronises(world: &mut ApiWorld, label_name: String) {
    assert_eq!(
        world.connections.saved_config,
        Some(Box::new(mail_config(&label_name, true)))
    );
}

#[then(
    expr = "my Google Mail connection synchronises the label {string} and still knows my address {string}"
)]
async fn the_connection_keeps_its_context(
    world: &mut ApiWorld,
    label_name: String,
    address: String,
) {
    let before = world.connections.mail.clone().unwrap();
    let stored = stored_connection(world, before.id).await;
    assert_eq!(
        stored,
        IntegrationConnection {
            provider: IntegrationProvider::GoogleMail {
                config: GoogleMailConfig {
                    sync_notifications_enabled: true,
                    synced_label: label(&label_name),
                },
                context: Some(GoogleMailContext {
                    user_email_address: Pii::<EmailAddress>::from_str(&address).unwrap(),
                    labels: vec![],
                }),
            },
            ..*before
        }
    );
}

#[then(expr = "my Google Mail connection still synchronises the label {string}, as before")]
async fn the_connection_is_unchanged(world: &mut ApiWorld, label_name: String) {
    let before = world.connections.mail.clone().unwrap();
    let stored = stored_connection(world, before.id).await;
    assert_eq!(
        stored,
        IntegrationConnection {
            provider: IntegrationProvider::GoogleMail {
                config: GoogleMailConfig {
                    sync_notifications_enabled: true,
                    synced_label: label(&label_name),
                },
                context: None,
            },
            ..stored.clone()
        }
    );
}

#[then("my other GitHub connection is unchanged")]
async fn my_github_connection_is_unchanged(world: &mut ApiWorld) {
    let before = world.integrations.github_connection.clone().unwrap();
    let stored = stored_connection(world, before.id).await;
    assert_eq!(stored, *before);
}

#[then(expr = "my inbox holds exactly the notification {string}, as it was")]
async fn my_inbox_holds_only(world: &mut ApiWorld, name: String) {
    let expected = world.notifications.named(&name).clone();
    let client = world.client();
    let app = world.app.get().await;
    let notifications: Vec<Notification> =
        list_notifications(&client, &app.api_address, vec![], true, None, None, false).await;
    assert_eq!(notifications, vec![expected.clone()]);
    assert_eq!(notifications[0].id, expected.id);
    assert_eq!(notifications[0].snoozed_until, expected.snoozed_until);
}

#[then(
    "both answers are 404 Not Found with the same message, which only repeats the id that was asked for"
)]
fn both_answers_are_the_same_not_found(world: &mut ApiWorld) {
    let answers = world.http.all();
    assert_eq!(answers.len(), 2);
    let foreign_id = world.connections.mail.as_ref().unwrap().id;
    let unknown_id = world.connections.unknown_id.unwrap();
    assert_eq!(answers[0].status, StatusCode::NOT_FOUND);
    assert_eq!(answers[1].status, StatusCode::NOT_FOUND);
    assert_eq!(
        answers[0].text(),
        format!("{{\"message\":\"Cannot update unknown integration connection {foreign_id}\"}}")
    );
    assert_eq!(
        answers[1].text(),
        format!("{{\"message\":\"Cannot update unknown integration connection {unknown_id}\"}}")
    );
}

#[then("my Slack connection still holds its paused configuration, and stays paused by my plan")]
async fn my_slack_connection_stays_paused(world: &mut ApiWorld) {
    let id = world.connections.slack.as_ref().unwrap().id;
    let stored = stored_connection(world, id).await;
    assert_eq!(
        stored.provider.config(),
        IntegrationConnectionConfig::Slack(SlackConfig::default())
    );
    assert!(stored.auto_paused_by_plan_at.is_some());
}

#[given("I have a validated Slack connection that synchronises my notifications")]
async fn i_have_a_slack_connection(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        IntegrationConnectionStatus::Validated,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    world.connections.slack = Some(connection);
}

#[when("a Slack token response holding cleartext tokens is stored for that connection")]
async fn a_token_response_is_stored(world: &mut ApiWorld) {
    let id = world.connections.slack.as_ref().unwrap().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let stored = app
        .repository
        .store_oauth_credential(
            &mut transaction,
            id,
            vec![1, 2, 3],
            None,
            None,
            serde_json::json!({
                "ok": true,
                "access_token": "xoxe.xoxp-cleartext",
                "refresh_token": "xoxe-1-cleartext",
                "authed_user": { "id": "U1", "access_token": "xoxp-cleartext" }
            }),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.connections.stored_token_response = Some(stored.raw_token_response);
}

#[then("the stored token response keeps the Slack user id and none of the tokens")]
fn the_stored_response_has_no_token(world: &mut ApiWorld) {
    assert_eq!(
        world.connections.stored_token_response,
        Some(serde_json::json!({ "ok": true, "authed_user": { "id": "U1" } }))
    );
}
