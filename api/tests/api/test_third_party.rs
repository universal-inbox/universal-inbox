//! Regression tests for `POST /third_party/task/items`.
//!
//! Documents the security contract introduced by `universal-inbox-bkj.29`: the
//! endpoint accepts only `ThirdPartyItemData`, so user-controlled identity
//! fields (`user_id`, `integration_connection_id`, `id`) cannot be forged.

use chrono::Utc;
use http::StatusCode;
use rstest::rstest;
use serde_json::json;
use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    third_party::{
        integrations::{
            api::{APISource, WebPage},
            todoist::TodoistItem,
        },
        item::{ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};
use universal_inbox_api::{configuration::Settings, integrations::todoist::TodoistSyncResponse};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, todoist_oauth_credential,
    },
    rest::{create_resource, create_resource_response},
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
};

#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_uses_authenticated_user(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let data = ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
        project_id: "1111".to_string(),
        ..*todoist_item.clone()
    }));

    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &app.client,
        &app.app.api_address,
        "third_party/task/items",
        Box::new(data.clone()),
    )
    .await;

    assert_eq!(creation.third_party_item.user_id, app.user.id);
    assert_eq!(
        creation.third_party_item.integration_connection_id,
        integration_connection.id
    );
    assert_eq!(creation.third_party_item.data, data);
    assert!(creation.task.is_some());
}

/// Regression for the cross-tenant write vulnerability (universal-inbox-bkj.29).
///
/// Before the fix, the request body deserialized into a full `ThirdPartyItem`,
/// letting a caller smuggle in a victim's `user_id`. After the fix, the body
/// type is `ThirdPartyItemData` — extra top-level fields are simply ignored by
/// serde, so even a hand-crafted JSON with `"user_id"` cannot influence the
/// persisted row.
#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_ignores_forged_identity_fields(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    // Craft a JSON body that LOOKS like the legacy full `ThirdPartyItem` shape,
    // with a bogus victim user_id and integration_connection_id. The new
    // contract only deserializes `ThirdPartyItemData`, so the spoofed fields
    // are dropped silently — and the persisted row uses the authenticated user.
    let victim_user_id = uuid::Uuid::new_v4();
    let victim_ic_id = uuid::Uuid::new_v4();
    let forged_body = json!({
        "id": uuid::Uuid::new_v4(),
        "source_id": todoist_item.id,
        "created_at": Utc::now(),
        "updated_at": Utc::now(),
        "user_id": victim_user_id,
        "integration_connection_id": victim_ic_id,
        "source_item": serde_json::Value::Null,
        "type": "TodoistItem",
        "content": *todoist_item.clone(),
    });

    let response = app
        .client
        .post(format!("{}third_party/task/items", app.app.api_address))
        .json(&forged_body)
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), StatusCode::OK);
    let creation: Box<ThirdPartyItemCreationResult> =
        response.json().await.expect("Cannot parse response");

    assert_eq!(
        creation.third_party_item.user_id, app.user.id,
        "Persisted row must use the authenticated user_id, not the spoofed one"
    );
    assert_ne!(creation.third_party_item.user_id.0, victim_user_id);
    assert_eq!(
        creation.third_party_item.integration_connection_id, integration_connection.id,
        "Persisted row must use the server-looked-up integration_connection_id"
    );
    assert_ne!(
        creation.third_party_item.integration_connection_id.0,
        victim_ic_id
    );
}

#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_requires_validated_integration_connection(
    #[future] authenticated_app: AuthenticatedApp,
    todoist_item: Box<TodoistItem>,
) {
    let app = authenticated_app.await;

    let data = ThirdPartyItemData::TodoistItem(Box::new(*todoist_item.clone()));

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "third_party/task/items",
        Box::new(data),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Cannot get response body");
    assert!(
        body.contains("No validated Todoist integration connection"),
        "expected message about missing Todoist connection, got: {body}"
    );
}

#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_rejects_notification_only_kind(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;

    let data = ThirdPartyItemData::WebPage(Box::new(WebPage {
        url: "https://example.com".parse().unwrap(),
        title: "example".to_string(),
        timestamp: Utc::now(),
        source: APISource::UniversalInboxExtension,
        favicon: None,
    }));

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "third_party/task/items",
        Box::new(data),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("Cannot get response body");
    assert!(
        body.contains("Cannot create a task item"),
        "expected message about unsupported kind, got: {body}"
    );
}
