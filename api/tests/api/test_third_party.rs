//! Tests for third party item creation.
//!
//! Task items are created through `ThirdPartyItemService::create_task_item`
//! only: there is no HTTP route for them. Notification items (web pages sent
//! by API clients such as the browser extension) go through
//! `POST /third_party/notification/items`, which validates the page.

use chrono::Utc;
use http::StatusCode;
use rstest::rstest;
use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    third_party::{
        integrations::{
            api::{APISource, WEB_PAGE_TITLE_MAX_LENGTH, WebPage},
            todoist::TodoistItem,
        },
        item::ThirdPartyItemData,
    },
    user::UserId,
};
use universal_inbox_api::{
    configuration::Settings, integrations::todoist::TodoistSyncResponse,
    universal_inbox::UniversalInboxError,
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, todoist_oauth_credential,
    },
    rest::create_resource_response,
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
    third_party::create_task_third_party_item,
};

fn web_page(url: &str, title: &str, favicon: Option<&str>) -> WebPage {
    WebPage {
        url: url.parse().unwrap(),
        title: title.to_string(),
        timestamp: Utc::now(),
        source: APISource::UniversalInboxExtension,
        favicon: favicon.map(|favicon| favicon.parse().unwrap()),
    }
}

async fn try_create_task_third_party_item(
    app: &TestedApp,
    data: ThirdPartyItemData,
    user_id: UserId,
) -> Result<(), UniversalInboxError> {
    let mut transaction = app.repository.begin().await.unwrap();
    app.third_party_item_service
        .read()
        .await
        .create_task_item(&mut transaction, data, user_id)
        .await
        .map(|_| ())
}

#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_uses_given_user(
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

    let creation = create_task_third_party_item(&app.app, data.clone(), app.user.id).await;

    assert_eq!(creation.third_party_item.user_id, app.user.id);
    assert_eq!(
        creation.third_party_item.integration_connection_id,
        integration_connection.id
    );
    assert_eq!(creation.third_party_item.data, data);
    assert!(creation.task.is_some());
}

#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_requires_validated_integration_connection(
    #[future] authenticated_app: AuthenticatedApp,
    todoist_item: Box<TodoistItem>,
) {
    let app = authenticated_app.await;

    let data = ThirdPartyItemData::TodoistItem(Box::new(*todoist_item.clone()));

    let result = try_create_task_third_party_item(&app.app, data, app.user.id).await;

    assert!(
        matches!(
            &result,
            Err(UniversalInboxError::UnsupportedAction(message))
                if message.contains("No validated Todoist integration connection")
        ),
        "expected an error about the missing Todoist connection, got: {result:?}"
    );
}

#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_rejects_notification_only_kind(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;

    let data =
        ThirdPartyItemData::WebPage(Box::new(web_page("https://example.com", "example", None)));

    let result = try_create_task_third_party_item(&app.app, data, app.user.id).await;

    assert!(
        matches!(
            &result,
            Err(UniversalInboxError::UnsupportedAction(message))
                if message.contains("Cannot create a task item")
        ),
        "expected an error about the unsupported kind, got: {result:?}"
    );
}

#[rstest]
#[tokio::test]
async fn test_create_task_third_party_item_has_no_http_route(
    #[future] authenticated_app: AuthenticatedApp,
    todoist_item: Box<TodoistItem>,
) {
    let app = authenticated_app.await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "third_party/task/items",
        Box::new(ThirdPartyItemData::TodoistItem(todoist_item)),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[rstest]
#[case::non_web_url(web_page("javascript:alert(1)", "example", None))]
#[case::non_web_favicon(web_page(
    "https://example.com",
    "example",
    Some("data:image/png;base64,AAAA")
))]
#[case::title_too_long(web_page(
    "https://example.com",
    &"a".repeat(WEB_PAGE_TITLE_MAX_LENGTH + 1),
    None
))]
#[tokio::test]
async fn test_create_notification_third_party_item_rejects_invalid_web_page(
    #[future] authenticated_app: AuthenticatedApp,
    #[case] page: WebPage,
) {
    let app = authenticated_app.await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "third_party/notification/items",
        Box::new(ThirdPartyItemData::WebPage(Box::new(page))),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
