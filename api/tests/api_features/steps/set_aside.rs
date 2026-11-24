use chrono::{TimeDelta, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use slack_morphism::SlackReactionName;

use universal_inbox::{
    audit::AuditActor,
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::slack::SlackConfig, provider::IntegrationProviderKind,
    },
    third_party::{
        integrations::api::{APISource, WebPage},
        item::{ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};
use universal_inbox_api::{
    repository::integration_connection::IntegrationConnectionRepository,
    universal_inbox::audit::actor_context::with_actor,
};

use crate::{
    helpers::rest::{create_resource, delete_resource},
    steps::{
        provider::ProviderName,
        set_aside_support::{
            Scene, assert_config_updated, assert_notification_set_aside,
            assert_triage_state_preserved, complete_notifications_sync, complete_restoring_sync,
            fetch_set_aside_at, provider_fixtures, reconnect_connection, seed_provider,
            triage_notification,
        },
    },
    world::ApiWorld,
};

#[given(expr = "I have a {provider} notification that I read, snoozed and turned into a task")]
async fn i_have_a_triaged_notification(world: &mut ApiWorld, provider: ProviderName) {
    let client = world.client();
    let user = world.user().clone();
    let app = world.app.get().await;
    let scene = Scene { app, client, user };
    let fixtures = provider_fixtures();
    let case = seed_provider(&scene, provider.0, &fixtures).await;
    let triaged = triage_notification(&scene, &case.notification, &fixtures.todoist_item).await;
    world.set_aside.case = Some(case);
    world.set_aside.triaged = Some(triaged);
}

#[given("I have a notification from a web page sent through the API")]
async fn i_have_an_api_notification(world: &mut ApiWorld) {
    let client = world.client();
    let user = world.user().clone();
    let app = world.app.get().await;
    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &client,
        &app.api_address,
        "third_party/notification/items",
        Box::new(ThirdPartyItemData::WebPage(Box::new(WebPage {
            url: "https://www.universal-inbox.com".parse().unwrap(),
            title: "Universal Inbox".to_string(),
            timestamp: Utc::now(),
            source: APISource::UniversalInboxExtension,
            favicon: None,
        }))),
    )
    .await;
    let notification = creation
        .notification
        .expect("Expected an API notification to be created");
    let scene = Scene { app, client, user };
    let triaged =
        triage_notification(&scene, &notification, &provider_fixtures().todoist_item).await;
    world.set_aside.triaged = Some(triaged);
}

#[given("my GitHub connection turned failing, as the API does when a refresh token expires")]
async fn my_github_connection_turned_failing(world: &mut ApiWorld) {
    let connection_id = world.set_aside.case().connection.id;
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_integration_connection_status(
            &mut transaction,
            connection_id,
            IntegrationConnectionStatus::Failing,
            Some("Failed to refresh the access token".to_string()),
            None,
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[when(
    expr = "I save, one after the other, every change to my {provider} connection that leaves what belongs in my inbox alone"
)]
async fn i_save_harmless_configurations(world: &mut ApiWorld, _provider: ProviderName) {
    let client = world.client();
    let user = world.user().clone();
    let app = world.app.get().await;
    let scene = Scene { app, client, user };
    let case = world.set_aside.case();
    let triaged = world.set_aside.triaged();
    assert_config_updated(&scene, case.connection.id, &case.harmless_config).await;
    assert_triage_state_preserved(&scene, triaged).await;
    if let Some(narrowing) = &case.scope_narrowing_config {
        assert_config_updated(&scene, case.connection.id, narrowing).await;
        assert_triage_state_preserved(&scene, triaged).await;
    }
}

async fn save_config(world: &mut ApiWorld, config: IntegrationConnectionConfig) {
    let client = world.client();
    let user = world.user().clone();
    let connection_id = world.set_aside.case().connection.id;
    let app = world.app.get().await;
    assert_config_updated(&Scene { app, client, user }, connection_id, &config).await;
}

#[when(expr = "I switch off the notifications of my {provider} connection")]
async fn i_switch_off_the_notifications(world: &mut ApiWorld, provider: ProviderName) {
    let config = world
        .set_aside
        .case()
        .notifications_off_config
        .clone()
        .unwrap_or_else(|| panic!("{:?} has no notification mute toggle", provider.0));
    save_config(world, config).await;
}

#[when(expr = "I switch off the synchronisation of my {provider} connection")]
async fn i_switch_off_the_synchronisation(world: &mut ApiWorld, provider: ProviderName) {
    let config = world
        .set_aside
        .case()
        .notifications_off_config
        .clone()
        .unwrap_or_else(|| panic!("{:?} has no off switch", provider.0));
    save_config(world, config).await;
}

#[when(expr = "I switch the notifications of my {provider} connection back on")]
async fn i_switch_the_notifications_back_on(world: &mut ApiWorld, _provider: ProviderName) {
    let config = world.set_aside.case().enabled_config.clone();
    save_config(world, config).await;
}

#[when("I save another configuration that leaves my Slack notifications switched off")]
async fn i_save_another_muted_configuration(world: &mut ApiWorld) {
    let mut still_muted = SlackConfig::disabled();
    still_muted.reaction_config.completion_reaction_name =
        Some(SlackReactionName("white_check_mark".to_string()));
    save_config(world, IntegrationConnectionConfig::Slack(still_muted)).await;
}

#[when("my Slack notifications are muted, with nothing set aside before")]
async fn my_slack_notifications_are_muted(world: &mut ApiWorld) {
    save_config(
        world,
        IntegrationConnectionConfig::Slack(SlackConfig::disabled()),
    )
    .await;
    let client = world.client();
    let user = world.user().clone();
    let triaged = world.set_aside.triaged().clone();
    let app = world.app.get().await;
    let set_aside_at = fetch_set_aside_at(&Scene { app, client, user }, &triaged).await;
    assert!(
        set_aside_at.is_some(),
        "Expected the Slack notification to have been set aside"
    );
    world.set_aside.set_aside_at = set_aside_at;
}

#[when(expr = "I disconnect my {provider} connection")]
async fn i_disconnect_the_connection(world: &mut ApiWorld, _provider: ProviderName) {
    let connection_id = world.set_aside.case().connection.id;
    disconnect(world, connection_id).await;
}

async fn disconnect(
    world: &mut ApiWorld,
    connection_id: universal_inbox::integration_connection::IntegrationConnectionId,
) {
    let client = world.client();
    let app = world.app.get().await;
    let disconnected: Box<IntegrationConnection> = delete_resource(
        &client,
        &app.api_address,
        "integration-connections",
        connection_id.into(),
    )
    .await;
    assert_eq!(disconnected.status, IntegrationConnectionStatus::Created);
}

#[when("I disconnect my API connection")]
async fn i_disconnect_the_api_connection(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connection = app
        .repository
        .get_integration_connection_per_provider(
            &mut transaction,
            user_id,
            IntegrationProviderKind::API,
            None,
            None,
        )
        .await
        .unwrap()
        .expect("Expected an API integration connection to have been created");
    transaction.commit().await.unwrap();
    disconnect(world, connection.id).await;
}

#[when("I reconnect it")]
async fn i_reconnect_it(world: &mut ApiWorld) {
    let client = world.client();
    let user = world.user().clone();
    let connection_id = world.set_aside.case().connection.id;
    let app = world.app.get().await;
    reconnect_connection(&Scene { app, client, user }, connection_id).await;
}

#[when(expr = "the notifications sync of {provider} completes")]
async fn the_notifications_sync_completes(world: &mut ApiWorld, provider: ProviderName) {
    let client = world.client();
    let user = world.user().clone();
    let app = world.app.get().await;
    complete_notifications_sync(&Scene { app, client, user }, provider.0).await;
}

#[when(expr = "the sync that restores the {provider} notifications completes")]
async fn the_restoring_sync_completes(world: &mut ApiWorld, provider: ProviderName) {
    let client = world.client();
    let user = world.user().clone();
    let app = world.app.get().await;
    complete_restoring_sync(&Scene { app, client, user }, provider.0).await;
}

#[when("the pause job pauses the connections of users inactive for over 90 days")]
async fn the_pause_job_pauses_inactive_users(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let result = with_actor(
        AuditActor::System {
            job: "PauseIntegrationConnections".to_string(),
            source: None,
            triggered_by: None,
        },
        async {
            app.integration_connection_service
                .read()
                .await
                .pause_integration_connections_of_inactive_users(
                    Utc::now() - TimeDelta::days(90),
                    None,
                )
                .await
        },
    )
    .await
    .unwrap();
    assert_eq!(result, (1, 0));
}

#[then("my notification is unchanged in my inbox")]
async fn my_notification_is_unchanged(world: &mut ApiWorld) {
    let client = world.client();
    let user = world.user().clone();
    let triaged = world.set_aside.triaged().clone();
    let app = world.app.get().await;
    assert_triage_state_preserved(&Scene { app, client, user }, &triaged).await;
}

#[then("my notification is set aside: out of my inbox and its count, but still readable by its id")]
async fn my_notification_is_set_aside(world: &mut ApiWorld) {
    let client = world.client();
    let user = world.user().clone();
    let triaged = world.set_aside.triaged().clone();
    let app = world.app.get().await;
    assert_notification_set_aside(&Scene { app, client, user }, &triaged).await;
}

#[then("my notification keeps the moment it was set aside")]
async fn my_notification_keeps_its_set_aside_moment(world: &mut ApiWorld) {
    let client = world.client();
    let user = world.user().clone();
    let triaged = world.set_aside.triaged().clone();
    let before = world.set_aside.set_aside_at;
    let app = world.app.get().await;
    assert_eq!(
        fetch_set_aside_at(&Scene { app, client, user }, &triaged).await,
        before,
        "Expected the notification to keep the moment it was set aside"
    );
}
