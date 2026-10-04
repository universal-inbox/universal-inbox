//! Integration tests for `GET /users/me/export`: an authenticated user
//! downloads all their data as a JSON file, without any secret in it.

use graphql_client::Response;
use pretty_assertions::assert_eq;
use reqwest::{Client, StatusCode, header};
use rstest::*;
use serde_json::Value;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig,
            linear::{LinearConfig, LinearSyncTaskConfig},
            todoist::TodoistConfig,
        },
    },
    task::ProjectSummary,
    third_party::integrations::{github::GithubNotification, linear::LinearIssue},
};
use universal_inbox_api::{
    configuration::Settings, integrations::linear::graphql::assigned_issues_query,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
        linear_oauth_credential, todoist_oauth_credential,
    },
    notification::{
        github::{create_notification_from_github_notification, github_notification},
        linear::sync_linear_tasks_response,
    },
    settings,
    task::linear::create_linear_task,
    tested_app,
    user::export_user_data_response,
};

fn ids(export: &Value, field: &str) -> Vec<String> {
    export[field]
        .as_array()
        .unwrap_or_else(|| panic!("`{field}` should be an array"))
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

#[rstest]
#[tokio::test]
async fn test_export_requires_authentication(#[future] tested_app: crate::helpers::TestedApp) {
    let app = tested_app.await;

    let response = export_user_data_response(&Client::new(), &app.api_address).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[rstest]
#[tokio::test]
#[allow(clippy::too_many_arguments)]
async fn test_export_contains_user_data_without_secrets(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_notification: Box<GithubNotification>,
    github_oauth_credential: OAuthCredentialFixture,
    linear_oauth_credential: OAuthCredentialFixture,
    todoist_oauth_credential: OAuthCredentialFixture,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
) {
    let app = authenticated_app.await;

    // An API token: its full JWT must not leak into the export
    let response = app
        .client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let auth_token: Value = response.json().await.unwrap();
    let jwt_token = auth_token["jwt_token"].as_str().unwrap().to_string();

    // A notification
    let github_access_token = github_oauth_credential.access_token.0.clone();
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let notification = create_notification_from_github_notification(
        &app.app,
        &github_notification,
        app.user.id,
        github_integration_connection.id,
    )
    .await;

    // A task
    let todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let project = ProjectSummary {
        name: "Project1".to_string(),
        source_id: "1111".into(),
    };
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
                ..Default::default()
            },
        }),
        &settings,
        linear_oauth_credential,
        None,
        None,
    )
    .await;
    let linear_issues: Vec<LinearIssue> =
        sync_linear_tasks_response.data.unwrap().try_into().unwrap();
    let task = create_linear_task(
        &app.app,
        &linear_issues[0],
        project,
        app.user.id,
        linear_integration_connection.id,
        todoist_integration_connection.id,
        "todoist_source_id".to_string(),
    )
    .await;

    // Another user's data must stay out of the export
    let (other_client, other_user) =
        authenticate_user(&app.app, "5678", "Jane", "Doe", "jane@example.com").await;
    let response = other_client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let other_integration_connection = create_and_mock_integration_connection(
        &app.app,
        other_user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        crate::helpers::integration_connection::github_oauth_credential(),
        None,
        None,
    )
    .await;

    let response = export_user_data_response(&app.client, &app.app.api_address).await;

    assert_eq!(response.status(), StatusCode::OK);
    let content_disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        content_disposition.starts_with("attachment; filename=\"universal-inbox-export-"),
        "unexpected Content-Disposition: {content_disposition}"
    );
    let body = response.text().await.unwrap();

    assert!(!body.contains(&jwt_token), "the full API token leaked");
    assert!(
        !body.contains(&github_access_token),
        "the OAuth access token leaked"
    );

    let export: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(export["user"]["id"], app.user.id.to_string());
    assert!(export["user"]["chat_support_email_signature"].is_null());
    assert_eq!(
        ids(&export, "authentication_tokens"),
        vec![auth_token["id"].as_str().unwrap().to_string()]
    );
    let mut integration_connection_ids = ids(&export, "integration_connections");
    integration_connection_ids.sort();
    let mut expected_integration_connection_ids = vec![
        github_integration_connection.id.to_string(),
        todoist_integration_connection.id.to_string(),
        linear_integration_connection.id.to_string(),
    ];
    expected_integration_connection_ids.sort();
    assert_eq!(
        integration_connection_ids,
        expected_integration_connection_ids
    );
    assert!(!integration_connection_ids.contains(&other_integration_connection.id.to_string()));
    assert_eq!(
        ids(&export, "notifications"),
        vec![notification.id.to_string()]
    );
    assert_eq!(ids(&export, "tasks"), vec![task.id.to_string()]);
}

#[rstest]
#[tokio::test]
async fn test_export_has_a_per_user_budget(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;

    // Mirrors `EXPORT_RATE_LIMIT_PER_MINUTE` in `api/src/utils/rate_limit.rs`.
    for i in 0..2 {
        let response = export_user_data_response(&app.client, &app.app.api_address).await;
        assert_eq!(response.status(), StatusCode::OK, "request {i}");
    }

    let response = export_user_data_response(&app.client, &app.app.api_address).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key(header::RETRY_AFTER));
}
