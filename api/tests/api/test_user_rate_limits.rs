//! Per-user budgets on costly authenticated endpoints: syncs (notifications
//! and tasks share one budget) and bulk notification patches.

use http::StatusCode;
use rstest::rstest;

use universal_inbox::notification::{
    NotificationStatus,
    service::{NotificationPatch, PatchNotificationsRequest},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    notification::sync_notifications_response,
    rest::patch_resource_collection_response,
    task::sync_tasks_response,
};

/// Mirrors `SYNC_RATE_LIMIT_PER_MINUTE` in `api/src/utils/rate_limit.rs`.
const SYNC_BUDGET: usize = 10;
/// Mirrors `BULK_PATCH_RATE_LIMIT_PER_MINUTE` in `api/src/utils/rate_limit.rs`.
const BULK_PATCH_BUDGET: usize = 30;

#[rstest]
#[tokio::test]
async fn test_sync_requests_share_a_per_user_budget(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;

    // Half the budget on notifications, half on tasks: both draw from one budget.
    for i in 0..SYNC_BUDGET {
        let response = if i % 2 == 0 {
            sync_notifications_response(&app.client, &app.app.api_address, None, true).await
        } else {
            sync_tasks_response(&app.client, &app.app.api_address, None, true).await
        };
        assert_eq!(response.status(), StatusCode::CREATED, "request {i}");
    }

    let response = sync_tasks_response(&app.client, &app.app.api_address, None, true).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = response
        .headers()
        .get(http::header::RETRY_AFTER)
        .expect("Retry-After header")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(retry_after >= 1);

    // Another user keeps their own budget.
    let (other_client, _) =
        authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
    let response =
        sync_notifications_response(&other_client, &app.app.api_address, None, true).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[rstest]
#[tokio::test]
async fn test_bulk_notification_patch_has_a_per_user_budget(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    let request = PatchNotificationsRequest {
        status: vec![NotificationStatus::Unread],
        sources: vec![],
        patch: NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
    };

    for i in 0..BULK_PATCH_BUDGET {
        let response = patch_resource_collection_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            &request,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "request {i}");
    }

    let response = patch_resource_collection_response(
        &app.client,
        &app.app.api_address,
        "notifications",
        &request,
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
