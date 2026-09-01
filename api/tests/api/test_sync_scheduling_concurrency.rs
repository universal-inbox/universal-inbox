//! Regression tests for the sync-scheduling deadlock/latency incident on
//! `GET /api/notifications` (Datadog: p50=232s/p95=802s, 186 `deadlock detected` 500s in 2
//! days — see Plans/kind-pondering-lamport.md in alan-apps for the full root cause).
//!
//! Two independent deadlock counterparties were identified:
//! - Candidate A: two concurrent authenticated requests each scheduling syncs for the same
//!   user's several integration connections, locking rows in different orders.
//! - Candidate B: the *unauthenticated* global sync-trigger endpoint (`for_user_id: None`),
//!   whose sync-status UPDATE could match every connection for every user, racing against
//!   any authenticated request touching one of those same rows.
//!
//! These tests fire real concurrent HTTP requests against the in-process test server and
//! assert nothing 500s — the strongest available proof against a race that, by construction,
//! only reproduces under real concurrency.

use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, linear::LinearConfig, todoist::TodoistConfig},
    },
    notification::{NotificationSourceKind, NotificationStatus},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
        linear_oauth_credential, todoist_oauth_credential,
    },
    notification::{list_notifications_response, sync_notifications_response},
    settings,
};
use universal_inbox_api::configuration::Settings;

/// A user with several validated integration connections (spanning notifications and
/// tasks) so the sync-scheduling loop actually touches more than one `integration_connection`
/// row per request — the shape candidate A's deadlock needs.
async fn setup_user_with_several_connections(
    app: &AuthenticatedApp,
    settings: &Settings,
    github_oauth_credential: OAuthCredentialFixture,
    linear_oauth_credential: OAuthCredentialFixture,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Linear(LinearConfig::enabled()),
        settings,
        linear_oauth_credential,
        None,
        None,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
}

#[rstest]
#[tokio::test]
async fn test_concurrent_list_notifications_never_deadlocks(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_oauth_credential: OAuthCredentialFixture,
    linear_oauth_credential: OAuthCredentialFixture,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    setup_user_with_several_connections(
        &app,
        &settings,
        github_oauth_credential,
        linear_oauth_credential,
        todoist_oauth_credential,
    )
    .await;

    // Several rounds of a wide concurrent fan-out: each `GET` (with `trigger_sync=true`)
    // schedules a sync for every one of this user's 3 connections. Before the fix, two
    // overlapping requests could lock those rows in different orders (no `ORDER BY` on the
    // connection fetch) and deadlock (SQLSTATE 40P01), surfacing as an HTTP 500.
    for _ in 0..5 {
        let requests = (0..16).map(|_| {
            list_notifications_response(
                &app.client,
                &app.app.api_address,
                vec![NotificationStatus::Unread, NotificationStatus::Read],
                false,
                None,
                None,
                true, // trigger_sync
            )
        });

        let statuses: Vec<reqwest::StatusCode> = futures::future::join_all(requests)
            .await
            .into_iter()
            .map(|response| response.status())
            .collect();

        for status in statuses {
            assert_eq!(
                status,
                reqwest::StatusCode::OK,
                "GET /api/notifications must never 500 under concurrent sync scheduling"
            );
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_unauthenticated_global_sync_does_not_deadlock_with_list(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_oauth_credential: OAuthCredentialFixture,
    linear_oauth_credential: OAuthCredentialFixture,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    setup_user_with_several_connections(
        &app,
        &settings,
        github_oauth_credential,
        linear_oauth_credential,
        todoist_oauth_credential,
    )
    .await;

    // The unauthenticated branch of `POST /api/notifications/sync` schedules a sync across
    // every user's connections for the given (or every) source — candidate B. Firing it
    // concurrently with authenticated reads on the same rows must not deadlock either.
    let unauthenticated_client = reqwest::Client::new();

    let mut sync_statuses = Vec::new();
    let mut list_statuses = Vec::new();

    for _ in 0..5 {
        let sync_requests = (0..8).map(|_| {
            sync_notifications_response(
                &unauthenticated_client,
                &app.app.api_address,
                Some(NotificationSourceKind::Github),
                true, // asynchronous
            )
        });
        let list_requests = (0..8).map(|_| {
            list_notifications_response(
                &app.client,
                &app.app.api_address,
                vec![NotificationStatus::Unread, NotificationStatus::Read],
                false,
                None,
                None,
                true, // trigger_sync
            )
        });

        let (sync_responses, list_responses) = tokio::join!(
            futures::future::join_all(sync_requests),
            futures::future::join_all(list_requests),
        );

        sync_statuses.extend(sync_responses.into_iter().map(|response| response.status()));
        list_statuses.extend(list_responses.into_iter().map(|response| response.status()));
    }

    for status in sync_statuses {
        assert_eq!(
            status,
            reqwest::StatusCode::CREATED,
            "unauthenticated POST /api/notifications/sync must never 500 under concurrent load"
        );
    }
    for status in list_statuses {
        assert_eq!(
            status,
            reqwest::StatusCode::OK,
            "GET /api/notifications must never 500 while the unauthenticated global sync trigger is running concurrently"
        );
    }
}
