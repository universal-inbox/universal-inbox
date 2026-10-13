use chrono::{TimeDelta, TimeZone, Utc};
use reqwest::Client;
use rstest::*;
use secrecy::ExposeSecret;
use serde_json::json;
use uuid::Uuid;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox::{
    HasHtmlUrl, Page, PageToken,
    audit::{
        AuditAction, AuditActor, AuditEntityKind, AuditEvent, AuditEventGroup, AuditUserChannel,
    },
    auth::auth_token::AuthenticationToken,
    integration_connection::{
        IntegrationConnectionId,
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, todoist::TodoistConfig},
        provider::IntegrationProviderKind,
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus,
        service::{NotificationPatch, PatchNotificationsRequest},
    },
    task::{DueDate, TaskCreation, TaskSourceKind},
    third_party::integrations::{github::GithubNotification, todoist::TodoistItem},
};
use universal_inbox_api::{configuration::Settings, integrations::todoist::TodoistSyncResponse};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
        todoist_oauth_credential,
    },
    notification::{
        create_task_from_notification,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::{patch_resource, patch_resource_collection},
    settings,
    task::{
        sync_tasks,
        todoist::{
            mock_todoist_get_item_service, mock_todoist_item_add_service,
            mock_todoist_sync_resources_service, sync_todoist_items_response,
            sync_todoist_projects_response, todoist_item,
        },
    },
    user::front_origin_header,
};

async fn list_audit_events(client: &Client, app: &TestedApp, query: &str) -> Page<AuditEvent> {
    client
        .get(format!("{}audit-events?{query}", app.api_address))
        .send()
        .await
        .expect("Failed to list audit events")
        .json()
        .await
        .expect("Failed to parse audit events")
}

async fn list_audit_event_groups(
    client: &Client,
    app: &TestedApp,
    query: &str,
) -> Page<AuditEventGroup> {
    client
        .get(format!("{}audit-events/groups?{query}", app.api_address))
        .send()
        .await
        .expect("Failed to list audit event groups")
        .json()
        .await
        .expect("Failed to parse audit event groups")
}

async fn create_api_key(app: &AuthenticatedApp) -> String {
    let api_key: AuthenticationToken = app
        .client
        .post(format!(
            "{}users/me/authentication-tokens",
            app.app.api_address
        ))
        .header(reqwest::header::ORIGIN, front_origin_header(&app.app))
        .send()
        .await
        .expect("Failed to create API key")
        .json()
        .await
        .expect("Failed to deserialize API key response");
    api_key.jwt_token.expose_secret().0.clone()
}

async fn github_notification_for(
    app: &AuthenticatedApp,
    settings: &Settings,
    github_notification: &GithubNotification,
    github_oauth_credential: OAuthCredentialFixture,
) -> (Box<Notification>, IntegrationConnectionId) {
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let notification = create_notification_from_github_notification(
        &app.app,
        github_notification,
        app.user.id,
        integration_connection.id,
    )
    .await;
    (notification, integration_connection.id)
}

fn snooze_patch() -> NotificationPatch {
    NotificationPatch {
        snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, 1, 9, 0, 0).unwrap()),
        ..Default::default()
    }
}

mod actors {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_web_user_change_is_recorded_with_its_from_and_to_values(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (notification, _) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;

        let _: Box<Notification> = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
            &snooze_patch(),
        )
        .await;

        let events = list_audit_events(
            &app.client,
            &app.app,
            &format!("entity_id={}", notification.id),
        )
        .await;

        // Newest first: the snooze, then the creation by the test fixture
        let snooze = &events.content[0];
        assert_eq!(snooze.entity_kind, AuditEntityKind::Notification);
        assert_eq!(snooze.entity_id, notification.id.0);
        assert_eq!(snooze.action, AuditAction::Updated);
        assert_eq!(snooze.title, notification.title);
        assert_eq!(snooze.source_kind.as_deref(), Some("Github"));
        assert_eq!(
            snooze.actor,
            AuditActor::User {
                via: AuditUserChannel::Web
            }
        );
        assert_eq!(
            serde_json::to_value(&snooze.changes).unwrap(),
            json!({ "snoozed_until": { "from": null, "to": "2064-01-01T09:00:00" } })
        );
        let creation = events.content.last().unwrap();
        assert_eq!(creation.action, AuditAction::Created);
        assert_eq!(
            serde_json::to_value(&creation.changes).unwrap(),
            json!({ "status": { "from": null, "to": "Unread" } })
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_api_key_and_extension_changes_are_told_apart(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (notification, _) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        let api_key = create_api_key(&app).await;
        let bearer_client = Client::new();
        let url = format!("{}notifications/{}", app.app.api_address, notification.id);

        let response = bearer_client
            .patch(&url)
            .bearer_auth(&api_key)
            .json(&snooze_patch())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let response = bearer_client
            .patch(&url)
            .bearer_auth(&api_key)
            .header("X-Universal-Inbox-Client", "extension/1.4.2")
            .json(&NotificationPatch {
                snoozed_until: Some(Utc.with_ymd_and_hms(2065, 1, 1, 9, 0, 0).unwrap()),
                ..Default::default()
            })
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let events = list_audit_events(
            &app.client,
            &app.app,
            &format!("entity_id={}&change=Snoozed", notification.id),
        )
        .await;

        let actors: Vec<AuditActor> = events.content.into_iter().map(|e| e.actor).collect();
        assert_eq!(
            actors,
            vec![
                AuditActor::Extension {
                    version: "1.4.2".to_string()
                },
                AuditActor::ApiKey,
            ]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_sync_changes_are_recorded_as_a_system_job_triggered_by_the_user(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_items_response: TodoistSyncResponse,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        create_and_mock_integration_connection(
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
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "items",
            &sync_todoist_items_response,
            None,
        )
        .await;

        let _: Vec<universal_inbox::task::TaskCreationResult> = sync_tasks(
            &app.client,
            &app.app.api_address,
            Some(TaskSourceKind::Todoist),
            false,
        )
        .await;

        let events =
            list_audit_events(&app.client, &app.app, "entity_kind=Task&change=Created").await;

        assert!(!events.content.is_empty());
        for event in events.content {
            assert_eq!(
                event.actor,
                AuditActor::System {
                    job: "SyncTasks".to_string(),
                    source: Some(IntegrationProviderKind::Todoist),
                    triggered_by: Some(Box::new(AuditActor::User {
                        via: AuditUserChannel::Web
                    })),
                }
            );
        }
    }
}

mod changes {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_bulk_change_events_share_their_transaction(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (_, integration_connection_id) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        let mut second_github_notification = *github_notification.clone();
        second_github_notification.id = "2".to_string();
        create_notification_from_github_notification(
            &app.app,
            &second_github_notification,
            app.user.id,
            integration_connection_id,
        )
        .await;
        for thread in ["1", "2"] {
            Mock::given(method("DELETE"))
                .and(path(format!("/notifications/threads/{thread}")))
                .respond_with(ResponseTemplate::new(205))
                .mount(&app.app.github_mock_server)
                .await;
        }

        let _: Vec<Notification> = patch_resource_collection(
            &app.client,
            &app.app.api_address,
            "notifications",
            &PatchNotificationsRequest {
                status: vec![NotificationStatus::Unread],
                sources: vec![NotificationSourceKind::Github],
                patch: NotificationPatch {
                    status: Some(NotificationStatus::Deleted),
                    ..Default::default()
                },
            },
        )
        .await;

        let events =
            list_audit_events(&app.client, &app.app, "change=Status&actor_kinds=User").await;

        assert_eq!(events.total, 2);
        assert_eq!(
            events.content[0].transaction_id,
            events.content[1].transaction_id
        );
        for event in &events.content {
            assert_eq!(
                serde_json::to_value(&event.changes).unwrap(),
                json!({ "status": { "from": "Unread", "to": "Deleted" } })
            );
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_task_creation_records_its_parameters_but_not_its_body(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_item: Box<TodoistItem>,
        todoist_oauth_credential: OAuthCredentialFixture,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (notification, _) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
        // Existing project in sync_todoist_projects_response
        // Stored, encrypted, as the task body: it must not be copied into the audit log
        let secret_description = "Secret description of the task".to_string();
        let todoist_item = Box::new(TodoistItem {
            project_id: "2222".to_string(),
            description: secret_description.clone(),
            ..(*todoist_item).clone()
        });
        let due_at: Option<DueDate> = todoist_item.due.as_ref().map(|due| due.into());
        let body = format!(
            "- [{}]({})",
            notification.title,
            notification.get_html_url().as_ref()
        );
        Mock::given(method("DELETE"))
            .and(path("/notifications/threads/1"))
            .respond_with(ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;
        mock_todoist_item_add_service(
            &app.app.todoist_mock_server,
            &todoist_item.id,
            todoist_item.content.clone(),
            Some(body.clone()),
            Some(todoist_item.project_id.clone()),
            due_at.as_ref().map(|due_at| due_at.into()),
            todoist_item.priority,
        )
        .await;
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone()).await;

        let notification_with_task = create_task_from_notification(
            &app.client,
            &app.app.api_address,
            notification.id,
            Some(TaskCreation {
                title: todoist_item.content.clone(),
                body: Some(body.clone()),
                project_name: Some("Project2".to_string()),
                due_at,
                priority: todoist_item.priority.into(),
                task_provider_kind: None,
                time_config: None,
            }),
        )
        .await
        .unwrap();
        let task = notification_with_task.task.unwrap();

        let response = app
            .client
            .get(format!(
                "{}audit-events?entity_id={}",
                app.app.api_address, task.id
            ))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(task.body, secret_description);
        assert!(
            !response.contains(&secret_description),
            "the task body leaked: {response}"
        );
        let events: Page<AuditEvent> = serde_json::from_str(&response).unwrap();

        let task_creation = events
            .content
            .iter()
            .find(|event| event.entity_kind == AuditEntityKind::Task)
            .expect("Expected the task creation event");
        assert_eq!(task_creation.action, AuditAction::Created);
        assert_eq!(
            task_creation.actor,
            AuditActor::User {
                via: AuditUserChannel::Web
            }
        );
        let params = task_creation.params.as_ref().unwrap();
        assert_eq!(params["title"], json!(todoist_item.content));
        assert_eq!(params["project"], json!("Project2"));
        assert_eq!(params["kind"], json!("Todoist"));
        assert_eq!(params["body_present"], json!(true));

        // The task history includes the notification it was created from
        let task_link = events
            .content
            .iter()
            .find(|event| event.entity_kind == AuditEntityKind::Notification)
            .expect("Expected the notification linked to the task");
        assert_eq!(task_link.entity_id, notification.id.0);
        assert_eq!(task_link.related_entity_id, Some(task.id.0));
        assert_eq!(
            task_link.changes["task_id"].to,
            Some(json!(task.id.0.to_string()))
        );
    }
}

mod listing {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_events_of_another_user_are_never_listed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (notification, _) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        let (other_client, _) =
            authenticate_user(&app.app, "5678", "Jane", "Roe", "jane@example.com").await;

        let events = list_audit_events(&other_client, &app.app, "").await;
        assert_eq!(events.total, 0);
        let events = list_audit_events(
            &other_client,
            &app.app,
            &format!("entity_id={}", notification.id),
        )
        .await;
        assert_eq!(events.total, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_events_are_paged_and_filtered_by_title(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (notification, _) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        for day in 1..=3 {
            let _: Box<Notification> = patch_resource(
                &app.client,
                &app.app.api_address,
                "notifications",
                notification.id.into(),
                &NotificationPatch {
                    snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, day, 9, 0, 0).unwrap()),
                    ..Default::default()
                },
            )
            .await;
        }

        let first_page = list_audit_events(&app.client, &app.app, "per_page=2").await;
        assert_eq!(first_page.total, 4);
        assert_eq!(first_page.content.len(), 2);
        assert_eq!(first_page.next_page_token, Some(PageToken::Offset(2)));
        let second_page = list_audit_events(
            &app.client,
            &app.app,
            &format!(
                "per_page=2&page_token={}",
                PageToken::Offset(2).to_url_parameter().unwrap()
            ),
        )
        .await;
        assert_eq!(second_page.content.len(), 2);
        assert_eq!(second_page.next_page_token, None);
        assert_eq!(second_page.content[1].action, AuditAction::Created);

        let title_word = notification.title.split_whitespace().next().unwrap();
        let matching = list_audit_events(
            &app.client,
            &app.app,
            &format!("query={}", title_word.to_lowercase()),
        )
        .await;
        assert_eq!(matching.total, 4);
        let not_matching = list_audit_events(&app.client, &app.app, "query=no%25such_title").await;
        assert_eq!(not_matching.total, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_groups_are_paged_without_splitting_a_bulk_action(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (notification, integration_connection_id) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        for thread in ["2", "3"] {
            let mut other_github_notification = *github_notification.clone();
            other_github_notification.id = thread.to_string();
            create_notification_from_github_notification(
                &app.app,
                &other_github_notification,
                app.user.id,
                integration_connection_id,
            )
            .await;
        }
        for day in 1..=3 {
            let _: Box<Notification> = patch_resource(
                &app.client,
                &app.app.api_address,
                "notifications",
                notification.id.into(),
                &NotificationPatch {
                    snoozed_until: Some(Utc.with_ymd_and_hms(2064, 1, day, 9, 0, 0).unwrap()),
                    ..Default::default()
                },
            )
            .await;
        }
        for thread in [github_notification.id.as_str(), "2", "3"] {
            Mock::given(method("DELETE"))
                .and(path(format!("/notifications/threads/{thread}")))
                .respond_with(ResponseTemplate::new(205))
                .mount(&app.app.github_mock_server)
                .await;
        }
        let _: Vec<Notification> = patch_resource_collection(
            &app.client,
            &app.app.api_address,
            "notifications",
            &PatchNotificationsRequest {
                status: vec![NotificationStatus::Unread],
                sources: vec![NotificationSourceKind::Github],
                patch: NotificationPatch {
                    status: Some(NotificationStatus::Deleted),
                    ..Default::default()
                },
            },
        )
        .await;

        // The web user made 6 changes: 3 snoozes, then 1 bulk deletion of 3 notifications
        let events = list_audit_events(&app.client, &app.app, "actor_kinds=User").await;
        assert_eq!(events.total, 6);

        let first_page =
            list_audit_event_groups(&app.client, &app.app, "actor_kinds=User&per_page=2").await;
        assert_eq!(first_page.total, 4);
        assert_eq!(first_page.pages_count, 2);
        assert_eq!(first_page.content.len(), 2);
        assert_eq!(first_page.next_page_token, Some(PageToken::Offset(2)));
        let bulk_deletion = &first_page.content[0].events;
        assert_eq!(bulk_deletion.len(), 3);
        for event in bulk_deletion {
            assert_eq!(event.transaction_id, bulk_deletion[0].transaction_id);
            assert_eq!(
                serde_json::to_value(&event.changes).unwrap(),
                json!({ "status": { "from": "Unread", "to": "Deleted" } })
            );
        }
        assert_eq!(first_page.content[1].events.len(), 1);

        let second_page = list_audit_event_groups(
            &app.client,
            &app.app,
            &format!(
                "actor_kinds=User&per_page=2&page_token={}",
                PageToken::Offset(2).to_url_parameter().unwrap()
            ),
        )
        .await;
        assert_eq!(second_page.content.len(), 2);
        assert_eq!(second_page.next_page_token, None);
        let snoozes: Vec<AuditEvent> = first_page.content[1..]
            .iter()
            .chain(&second_page.content)
            .map(|group| {
                assert_eq!(group.events.len(), 1);
                group.events[0].clone()
            })
            .collect();
        assert_eq!(
            snoozes
                .iter()
                .map(|event| event.changes["snoozed_until"].to.clone())
                .collect::<Vec<_>>(),
            [3, 2, 1]
                .map(|day| Some(json!(format!("2064-01-0{day}T09:00:00"))))
                .to_vec()
        );
    }
}

mod retention {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_purge_deletes_only_expired_events(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        let (notification, _) = github_notification_for(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;
        let _: Box<Notification> = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
            &snooze_patch(),
        )
        .await;
        // Age the creation event past the 7 days retention
        let expired_at = Utc::now() - TimeDelta::days(8);
        sqlx::query("UPDATE audit_event SET created_at = $1 WHERE action = 'Created'")
            .bind(expired_at)
            .execute(&*app.app.repository.pool)
            .await
            .unwrap();

        let deleted = app
            .app
            .audit_service
            .purge_expired_audit_events()
            .await
            .unwrap();

        assert_eq!(deleted, 1);
        let events = list_audit_events(&app.client, &app.app, "").await;
        assert_eq!(events.total, 1);
        assert_eq!(events.content[0].action, AuditAction::Updated);
        // Unused id: nothing to purge for it
        let events = list_audit_events(
            &app.client,
            &app.app,
            &format!("entity_id={}", Uuid::new_v4()),
        )
        .await;
        assert_eq!(events.total, 0);
    }
}
