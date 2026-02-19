use rstest::*;
use serde_json::json;

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{github::GithubConfig, ticktick::TickTickConfig, todoist::TodoistConfig},
        provider::IntegrationProviderKind,
    },
    notification::{Notification, NotificationStatus, NotificationWithTask},
    task::{Task, TaskCreation},
    third_party::{
        integrations::ticktick::{TickTickItem, TickTickItemPriority},
        item::{ThirdPartyItem, ThirdPartyItemData},
    },
    user::UserPreferences,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, ticktick::TickTickService},
};

use universal_inbox::task::integrations::ticktick::TickTickProject;
use universal_inbox::third_party::integrations::github::GithubNotification;
use universal_inbox_api::integrations::ticktick::TickTickCreateTaskResponse;

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, nango_github_connection, nango_ticktick_connection,
        nango_todoist_connection,
    },
    notification::{
        create_task_from_notification,
        github::{create_notification_from_github_notification, github_notification},
    },
    rest::get_resource,
    settings,
    task::ticktick::{
        mock_ticktick_create_task_service, mock_ticktick_get_task_service,
        mock_ticktick_list_projects_service, ticktick_item, ticktick_projects_response,
    },
};

mod user_preferences {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_get_and_patch_user_preferences(
        _settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        // GET default preferences (no preferences row yet)
        let response = app
            .client
            .get(format!("{}users/me/preferences", app.app.api_address))
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(response.status(), 200);
        let prefs: UserPreferences = response.json().await.expect("Cannot parse JSON result");
        assert_eq!(prefs.user_id, app.user.id);
        assert_eq!(prefs.default_task_manager_provider_kind, None);

        // PATCH to set TickTick as default
        let patch = json!({
            "default_task_manager_provider_kind": "TickTick"
        });
        let response = app
            .client
            .patch(format!("{}users/me/preferences", app.app.api_address))
            .json(&patch)
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(response.status(), 200);
        let patched_prefs: UserPreferences =
            response.json().await.expect("Cannot parse JSON result");
        assert_eq!(patched_prefs.user_id, app.user.id);
        assert_eq!(
            patched_prefs.default_task_manager_provider_kind,
            Some(IntegrationProviderKind::TickTick)
        );

        // GET again to verify persistence
        let response = app
            .client
            .get(format!("{}users/me/preferences", app.app.api_address))
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(response.status(), 200);
        let prefs: UserPreferences = response.json().await.expect("Cannot parse JSON result");
        assert_eq!(
            prefs.default_task_manager_provider_kind,
            Some(IntegrationProviderKind::TickTick)
        );

        // PATCH to change to Todoist
        let patch = json!({
            "default_task_manager_provider_kind": "Todoist"
        });
        let response = app
            .client
            .patch(format!("{}users/me/preferences", app.app.api_address))
            .json(&patch)
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(response.status(), 200);
        let updated_prefs: UserPreferences =
            response.json().await.expect("Cannot parse JSON result");
        assert_eq!(
            updated_prefs.default_task_manager_provider_kind,
            Some(IntegrationProviderKind::Todoist)
        );
    }
}

mod create_task_with_explicit_provider {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    #[allow(clippy::too_many_arguments)]
    async fn test_create_task_from_notification_with_explicit_ticktick_provider(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        ticktick_item: Box<TickTickItem>,
        ticktick_projects_response: Vec<TickTickProject>,
        nango_ticktick_connection: Box<NangoConnection>,
        nango_github_connection: Box<NangoConnection>,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        // Set up GitHub integration connection
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;

        // Set up Todoist integration connection (should NOT be used)
        let _todoist_ic = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
            None,
            None,
        )
        .await;

        // Set up TickTick integration connection (should be used)
        let ticktick_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            &settings,
            nango_ticktick_connection,
            None,
            None,
        )
        .await;

        // Create a GitHub notification
        let notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        // Existing project in ticktick_projects_response
        let project = "Project2".to_string();
        let project_id = "tt_proj_2222".to_string();
        let ticktick_item = Box::new(TickTickItem {
            project_id: project_id.clone(),
            ..*ticktick_item
        });
        let body = Some(format!(
            "- [{}]({})",
            notification.title,
            notification.get_html_url().as_ref()
        ));

        // Mock GitHub notification deletion
        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path("/notifications/threads/1"))
            .respond_with(wiremock::ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;

        // Mock TickTick list projects
        mock_ticktick_list_projects_service(
            &app.app.ticktick_mock_server,
            &ticktick_projects_response,
        )
        .await;

        // Mock TickTick create task
        let create_response = TickTickCreateTaskResponse {
            id: ticktick_item.id.clone(),
            project_id: ticktick_item.project_id.clone(),
            title: ticktick_item.title.clone(),
            content: ticktick_item.content.clone(),
            desc: ticktick_item.desc.clone(),
            all_day: ticktick_item.all_day,
            start_date: ticktick_item.start_date,
            due_date: ticktick_item.due_date,
            time_zone: ticktick_item.time_zone.clone(),
            priority: ticktick_item.priority,
            status: ticktick_item.status,
            tags: ticktick_item.tags.clone(),
        };
        mock_ticktick_create_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.title,
            Some(&project_id),
            ticktick_item.priority,
            &create_response,
        )
        .await;

        // Mock TickTick get task (used to fetch the full item after creation)
        mock_ticktick_get_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.project_id,
            &ticktick_item.id,
            &ticktick_item,
        )
        .await;

        // Create task from notification with explicit TickTick provider
        let notification_with_task = create_task_from_notification(
            &app.client,
            &app.app.api_address,
            notification.id,
            Some(TaskCreation {
                title: ticktick_item.title.clone(),
                body,
                project_name: Some("Project2".to_string()),
                due_at: ticktick_item.get_due_date(),
                priority: ticktick_item.priority.into(),
                task_provider_kind: Some(IntegrationProviderKind::TickTick),
            }),
        )
        .await;

        // Verify the task was created via TickTick
        let new_task_id = notification_with_task
            .as_ref()
            .unwrap()
            .task
            .as_ref()
            .unwrap()
            .id;
        assert_eq!(
            notification_with_task,
            Some(NotificationWithTask::build(
                &Notification {
                    status: NotificationStatus::Deleted,
                    ..*notification
                },
                Some(Task {
                    id: new_task_id,
                    updated_at: notification_with_task
                        .as_ref()
                        .unwrap()
                        .task
                        .as_ref()
                        .unwrap()
                        .updated_at,
                    ..(*TickTickService::build_task_with_project_name(
                        &ticktick_item,
                        project,
                        &ThirdPartyItem {
                            id: notification_with_task
                                .as_ref()
                                .unwrap()
                                .task
                                .as_ref()
                                .unwrap()
                                .source_item
                                .id,
                            source_id: ticktick_item.id.clone(),
                            created_at: notification_with_task
                                .as_ref()
                                .unwrap()
                                .task
                                .as_ref()
                                .unwrap()
                                .source_item
                                .created_at,
                            updated_at: notification_with_task
                                .as_ref()
                                .unwrap()
                                .task
                                .as_ref()
                                .unwrap()
                                .source_item
                                .updated_at,
                            user_id: app.user.id,
                            data: ThirdPartyItemData::TickTickItem(ticktick_item.clone()),
                            integration_connection_id: ticktick_integration_connection.id,
                            source_item: None,
                        },
                        app.user.id
                    )
                    .await)
                        .into()
                })
            ))
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
        assert_eq!(deleted_notification.task_id, Some(new_task_id));
    }
}

mod create_task_with_default_preference {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    #[allow(clippy::too_many_arguments)]
    async fn test_create_task_from_notification_with_ticktick_default_preference(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        ticktick_item: Box<TickTickItem>,
        ticktick_projects_response: Vec<TickTickProject>,
        nango_ticktick_connection: Box<NangoConnection>,
        nango_github_connection: Box<NangoConnection>,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        // Set up GitHub integration connection
        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            &settings,
            nango_github_connection,
            None,
            None,
        )
        .await;

        // Set up Todoist integration connection (should NOT be used)
        let _todoist_ic = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
            None,
            None,
        )
        .await;

        // Set up TickTick integration connection (should be used via default preference)
        let ticktick_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
            &settings,
            nango_ticktick_connection,
            None,
            None,
        )
        .await;

        // Set user preference to default to TickTick
        let patch = json!({
            "default_task_manager_provider_kind": "TickTick"
        });
        let response = app
            .client
            .patch(format!("{}users/me/preferences", app.app.api_address))
            .json(&patch)
            .send()
            .await
            .expect("Failed to execute request");
        assert_eq!(response.status(), 200);

        // Create a GitHub notification
        let notification = create_notification_from_github_notification(
            &app.app,
            &github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        // When task_creation is None and default_project is None,
        // TickTick creates the task in the inbox (no project_id sent).
        // The priority will be TaskPriority::default() (P4) which maps to TickTickItemPriority::None.
        let ticktick_item = Box::new(TickTickItem {
            project_id: ticktick_item.project_id.clone(),
            ..*ticktick_item
        });

        // Mock GitHub notification deletion
        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path("/notifications/threads/1"))
            .respond_with(wiremock::ResponseTemplate::new(205))
            .mount(&app.app.github_mock_server)
            .await;

        // Mock TickTick list projects
        mock_ticktick_list_projects_service(
            &app.app.ticktick_mock_server,
            &ticktick_projects_response,
        )
        .await;

        // Mock TickTick create task with default values (notification title, no project, default priority)
        let create_response = TickTickCreateTaskResponse {
            id: ticktick_item.id.clone(),
            project_id: ticktick_item.project_id.clone(),
            title: notification.title.clone(),
            content: ticktick_item.content.clone(),
            desc: ticktick_item.desc.clone(),
            all_day: ticktick_item.all_day,
            start_date: ticktick_item.start_date,
            due_date: ticktick_item.due_date,
            time_zone: ticktick_item.time_zone.clone(),
            priority: TickTickItemPriority::None,
            status: ticktick_item.status,
            tags: ticktick_item.tags.clone(),
        };
        mock_ticktick_create_task_service(
            &app.app.ticktick_mock_server,
            &notification.title,
            None, // No project_id when default_project is None
            TickTickItemPriority::None,
            &create_response,
        )
        .await;

        // Mock TickTick get task (used to fetch the full item after creation)
        let fetched_ticktick_item = TickTickItem {
            title: notification.title.clone(),
            priority: TickTickItemPriority::None,
            ..*ticktick_item.clone()
        };
        mock_ticktick_get_task_service(
            &app.app.ticktick_mock_server,
            &ticktick_item.project_id,
            &ticktick_item.id,
            &fetched_ticktick_item,
        )
        .await;

        // Create task from notification with no explicit task_creation (use defaults)
        let notification_with_task =
            create_task_from_notification(&app.client, &app.app.api_address, notification.id, None)
                .await;

        // Verify the task was created via TickTick (not Todoist)
        assert!(notification_with_task.is_some());
        let nwt = notification_with_task.as_ref().unwrap();
        assert!(nwt.task.is_some());
        let task = nwt.task.as_ref().unwrap();

        // The task should be linked to the TickTick integration connection
        assert_eq!(
            task.source_item.integration_connection_id,
            ticktick_integration_connection.id
        );

        let deleted_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification.id.into(),
        )
        .await;
        assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
        assert_eq!(deleted_notification.task_id, Some(task.id));
    }
}
