use chrono::Utc;
use rstest::rstest;
use universal_inbox::{
    HasHtmlUrl,
    notification::{NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::api::{APISource, WebPage},
        item::{ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    rest::create_resource,
};

#[rstest]
#[tokio::test]
async fn test_create_universal_inbox_url_notification(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;

    let third_party_data = ThirdPartyItemData::WebPage(Box::new(WebPage {
        url: "https://www.universal-inbox.com".parse().unwrap(),
        title: "Universal Inbox".to_string(),
        timestamp: Utc::now(),
        source: APISource::UniversalInboxExtension,
        favicon: None,
    }));
    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &app.client,
        &app.app.api_address,
        "third_party/notification/items",
        Box::new(third_party_data.clone()),
    )
    .await;

    assert!(creation.task.is_none());
    assert_eq!(creation.third_party_item.data, third_party_data);

    let Some(notification) = creation.notification else {
        unreachable!("Expected a notification to be created");
    };

    assert_eq!(notification.title, "Universal Inbox");
    assert_eq!(
        notification.get_html_url(),
        "https://www.universal-inbox.com".parse().unwrap()
    );
    assert_eq!(notification.status, NotificationStatus::Unread);
    assert_eq!(notification.source_item, creation.third_party_item);
    assert_eq!(notification.source_item.data, third_party_data);
    assert_eq!(notification.kind, NotificationSourceKind::API);

    // Create yet another notification
    let third_party_data = ThirdPartyItemData::WebPage(Box::new(WebPage {
        url: "https://app.universal-inbox.com".parse().unwrap(),
        title: "Universal Inbox app".to_string(),
        timestamp: Utc::now(),
        source: APISource::UniversalInboxExtension,
        favicon: None,
    }));
    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &app.client,
        &app.app.api_address,
        "third_party/notification/items",
        Box::new(third_party_data.clone()),
    )
    .await;

    assert!(creation.task.is_none());
    assert_eq!(creation.third_party_item.data, third_party_data);

    let Some(notification) = creation.notification else {
        unreachable!("Expected a notification to be created");
    };

    assert_eq!(notification.title, "Universal Inbox app");
    assert_eq!(
        notification.get_html_url(),
        "https://app.universal-inbox.com".parse().unwrap()
    );
}
