use std::sync::Arc;

use anyhow::Context;
use apalis::prelude::Data;
use rstest::*;
use serde_json::{Value, json};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, config::IntegrationConnectionConfig,
        integrations::notion::NotionConfig,
    },
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, service::NotificationPatch,
    },
    third_party::{integrations::notion::to_plain_text, item::ThirdPartyItemData},
};
use universal_inbox_api::{
    configuration::Settings,
    integrations::notion::webhook::NotionWebhookEvent,
    jobs::notion::{
        NotionWebhookEventJob, handle_notion_comment_event, handle_notion_page_content_event,
        handle_notion_webhook_event,
    },
    universal_inbox::{UniversalInboxError, audit::actor_context::with_actor},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    notification::{list_notifications, update_notification},
    notion::{
        BLOCK_ID, DISCUSSION_ID, ME, MY_TOKEN, NotionTestConnection, PAGE_ID, SOMEONE, connect_me,
        create_notion_integration_connection, mock_my_discussion, mock_notion_blocks,
        mock_notion_deleted_block, mock_notion_discussion, mock_notion_failure_after_triage,
        mock_notion_not_accessible, notion_comment, notion_comment_event,
        notion_page_content_event, notion_paragraph_block, notion_webhook_actor,
        post_signed_notion_event, pushed_jobs_count, text_segment, user_mention_segment,
    },
    rest::delete_resource,
    set_audit_actor, settings,
};

/// A workspace id unique to the test, so the cached workspace lookup of one
/// test never answers for another.
fn new_workspace_id() -> String {
    Uuid::new_v4().to_string()
}

const SOMEONE_TOKEN: &str = "notion-token-someone";

/// Connect `ME` and `SOMEONE` (another Universal Inbox user, whose client
/// is returned) to the same new workspace.
async fn connect_me_and_someone(
    app: &AuthenticatedApp,
    settings: &Settings,
) -> (String, reqwest::Client) {
    let workspace_id = new_workspace_id();
    let (other_client, other_user) = authenticate_user(
        &app.app,
        "other-user-auth-id",
        "Other",
        "User",
        "other.user@example.com",
    )
    .await;
    create_notion_integration_connection(
        &app.app,
        other_user.id,
        settings,
        SOMEONE,
        SOMEONE_TOKEN,
        &workspace_id,
        NotionConfig::enabled(),
    )
    .await;
    create_notion_integration_connection(
        &app.app,
        app.user.id,
        settings,
        ME,
        MY_TOKEN,
        &workspace_id,
        NotionConfig::enabled(),
    )
    .await;
    (workspace_id, other_client)
}

/// Run the job as the worker does: its transaction is committed even when
/// a connection failed.
async fn run_job(app: &AuthenticatedApp, event: Value) -> Result<(), UniversalInboxError> {
    with_actor(
        notion_webhook_actor(),
        handle_notion_webhook_event(
            NotionWebhookEventJob(serde_json::from_value(event).unwrap()),
            Data::new(Arc::clone(&app.app.notification_service)),
            Data::new(Arc::clone(&app.app.integration_connection_service)),
            Data::new(Arc::clone(&app.app.third_party_item_service)),
        ),
    )
    .await
}

async fn notion_notifications_of(app: &AuthenticatedApp, client: &reqwest::Client) -> usize {
    list_notifications(
        client,
        &app.app.api_address,
        vec![],
        false,
        None,
        Some(NotificationSourceKind::Notion),
        false,
    )
    .await
    .len()
}

mod route {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_verification_handshake_is_acknowledged(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        let response = app
            .client
            .post(format!("{}hooks/notion/events", app.app.api_address))
            .json(&json!({ "verification_token": "secret_handshake_token" }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(pushed_jobs_count(&app.app).await, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_unsigned_or_badly_signed_event_is_rejected(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let event = comment_event(&new_workspace_id(), "comment.created", "comment-1");

        let unsigned = app
            .client
            .post(format!("{}hooks/notion/events", app.app.api_address))
            .json(&event)
            .send()
            .await
            .unwrap();
        let badly_signed = app
            .client
            .post(format!("{}hooks/notion/events", app.app.api_address))
            .header("X-Notion-Signature", "sha256=00ff")
            .json(&event)
            .send()
            .await
            .unwrap();

        assert_eq!(unsigned.status(), 401);
        assert_eq!(badly_signed.status(), 401);
        assert_eq!(pushed_jobs_count(&app.app).await, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_comment_event_of_a_connected_workspace_is_queued(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = new_workspace_id();
        create_notion_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            ME,
            MY_TOKEN,
            &workspace_id,
            NotionConfig::enabled(),
        )
        .await;

        let response = post_signed_notion_event(
            &app.client,
            &app.app.api_address,
            &comment_event(&workspace_id, "comment.created", "comment-1"),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_eq!(pushed_jobs_count(&app.app).await, 1);
    }

    #[rstest]
    #[tokio::test]
    async fn test_page_content_event_of_a_connected_workspace_is_queued(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = new_workspace_id();
        create_notion_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            ME,
            MY_TOKEN,
            &workspace_id,
            NotionConfig::enabled(),
        )
        .await;

        let response = post_signed_notion_event(
            &app.client,
            &app.app.api_address,
            &notion_page_content_event(&workspace_id, "event-1", PAGE_ID, &[BLOCK_ID], &[ME]),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_eq!(pushed_jobs_count(&app.app).await, 1);
    }

    #[rstest]
    #[tokio::test]
    async fn test_comment_event_of_an_unknown_workspace_is_dropped(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;

        let response = post_signed_notion_event(
            &app.client,
            &app.app.api_address,
            &comment_event(&new_workspace_id(), "comment.created", "comment-1"),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_eq!(pushed_jobs_count(&app.app).await, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_other_event_types_are_dropped(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = new_workspace_id();
        create_notion_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            ME,
            MY_TOKEN,
            &workspace_id,
            NotionConfig::enabled(),
        )
        .await;

        let response = post_signed_notion_event(
            &app.client,
            &app.app.api_address,
            &comment_event(&workspace_id, "page.locked", "comment-1"),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_eq!(pushed_jobs_count(&app.app).await, 0);
    }

    #[rstest]
    #[case::comment_page_id(|workspace_id: &str| notion_comment_event(
        workspace_id, "comment.created", "comment-1", SOMEONE, DISCUSSION_ID, "../v1/users",
        BLOCK_ID, &[ME],
    ))]
    #[case::comment_parent(|workspace_id: &str| notion_comment_event(
        workspace_id, "comment.created", "comment-1", SOMEONE, DISCUSSION_ID, PAGE_ID,
        "../v1/users", &[ME],
    ))]
    #[case::updated_block(|workspace_id: &str| notion_page_content_event(
        workspace_id, "event-1", PAGE_ID, &[BLOCK_ID, "../v1/users"], &[ME],
    ))]
    #[tokio::test]
    async fn test_event_with_a_malformed_resource_id_is_dropped(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        #[case] event: fn(&str) -> Value,
    ) {
        let app = authenticated_app.await;
        let workspace_id = new_workspace_id();
        create_notion_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            ME,
            MY_TOKEN,
            &workspace_id,
            NotionConfig::enabled(),
        )
        .await;

        let response =
            post_signed_notion_event(&app.client, &app.app.api_address, &event(&workspace_id))
                .await;

        assert_eq!(response.status(), 200);
        assert_eq!(pushed_jobs_count(&app.app).await, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_events_of_a_disconnected_workspace_are_dropped(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = new_workspace_id();
        let connection = create_notion_integration_connection(
            &app.app,
            app.user.id,
            &settings,
            ME,
            MY_TOKEN,
            &workspace_id,
            NotionConfig::enabled(),
        )
        .await;
        let event = comment_event(&workspace_id, "comment.created", "comment-1");
        post_signed_notion_event(&app.client, &app.app.api_address, &event).await;
        assert_eq!(pushed_jobs_count(&app.app).await, 1);

        let _: Box<IntegrationConnection> = delete_resource(
            &app.client,
            &app.app.api_address,
            "integration-connections",
            connection.id.into(),
        )
        .await;
        let response = post_signed_notion_event(&app.client, &app.app.api_address, &event).await;

        assert_eq!(response.status(), 200);
        assert_eq!(pushed_jobs_count(&app.app).await, 1);
    }
}

mod job {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_mention_creates_an_unread_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        let comments = vec![notion_comment(
            "comment-1",
            DISCUSSION_ID,
            BLOCK_ID,
            SOMEONE,
            5,
            vec![
                text_segment("What do you think "),
                user_mention_segment(ME, "Me"),
            ],
        )];
        mock_notion_discussion(
            &app.app.notion_mock_server,
            MY_TOKEN,
            PAGE_ID,
            SOMEONE,
            BLOCK_ID,
            SOMEONE,
            comments,
        )
        .await;

        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;

        let notifications = notion_notifications(&app, vec![]).await;
        assert_eq!(notifications.len(), 1);
        let notification = &notifications[0];
        assert_eq!(notification.status, NotificationStatus::Unread);
        assert_eq!(
            notification.title,
            format!("Comment by User {SOMEONE} on Roadmap")
        );
        assert_eq!(
            notification.updated_at.to_rfc3339(),
            "2026-10-10T09:05:00+00:00"
        );
        let ThirdPartyItemData::NotionComment(comment) = &notification.source_item.data else {
            panic!("Expected a NotionComment item");
        };
        assert_eq!(comment.discussion_id, DISCUSSION_ID);
        assert!(comment.quoted_content.is_some());
    }

    #[rstest]
    #[case::page_creator(ME, SOMEONE)]
    #[case::block_creator(SOMEONE, ME)]
    #[tokio::test]
    async fn test_comment_on_my_page_or_block_creates_a_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        #[case] page_created_by: &str,
        #[case] block_created_by: &str,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_notion_discussion(
            &app.app.notion_mock_server,
            MY_TOKEN,
            PAGE_ID,
            page_created_by,
            BLOCK_ID,
            block_created_by,
            vec![notion_comment(
                "comment-1",
                DISCUSSION_ID,
                BLOCK_ID,
                SOMEONE,
                5,
                vec![text_segment("Looks good")],
            )],
        )
        .await;

        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;

        let notifications = notion_notifications(&app, vec![]).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_comment_not_involving_me_is_ignored(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_notion_discussion(
            &app.app.notion_mock_server,
            MY_TOKEN,
            PAGE_ID,
            SOMEONE,
            BLOCK_ID,
            SOMEONE,
            vec![notion_comment(
                "comment-1",
                DISCUSSION_ID,
                BLOCK_ID,
                SOMEONE,
                5,
                vec![text_segment("Note to self")],
            )],
        )
        .await;

        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_disabled_comment_sync_creates_nothing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(
            &app,
            &settings,
            NotionConfig {
                sync_comments_enabled: false,
                sync_mentions_enabled: true,
            },
        )
        .await
        .workspace_id;
        mock_mentioning_discussion(&app, MY_TOKEN).await;

        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_enabled_comment_sync_applies_to_the_next_comment(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let NotionTestConnection {
            workspace_id,
            connection,
        } = connect_me(
            &app,
            &settings,
            NotionConfig {
                sync_comments_enabled: false,
                sync_mentions_enabled: true,
            },
        )
        .await;
        mock_mentioning_discussion(&app, MY_TOKEN).await;
        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;
        assert!(notion_notifications(&app, all_statuses()).await.is_empty());

        let response = app
            .client
            .put(format!(
                "{}integration-connections/{}/config",
                app.app.api_address, connection.id
            ))
            .json(&IntegrationConnectionConfig::Notion(NotionConfig::enabled()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;

        assert_eq!(notion_notifications(&app, all_statuses()).await.len(), 1);
    }

    #[rstest]
    #[tokio::test]
    async fn test_event_of_another_workspace_creates_nothing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, NotionConfig::enabled()).await;
        mock_mentioning_discussion(&app, MY_TOKEN).await;

        handle(&app, &new_workspace_id(), "comment-1", SOMEONE, &[ME]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_page_not_shared_with_me_creates_nothing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_notion_not_accessible(&app.app.notion_mock_server, MY_TOKEN).await;

        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_my_reply_hides_the_notification_until_someone_answers(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        let mention = notion_comment(
            "comment-1",
            DISCUSSION_ID,
            BLOCK_ID,
            SOMEONE,
            5,
            vec![text_segment("Ping "), user_mention_segment(ME, "Me")],
        );
        let my_reply = notion_comment(
            "comment-2",
            DISCUSSION_ID,
            BLOCK_ID,
            ME,
            10,
            vec![text_segment("Done")],
        );
        let their_reply = notion_comment(
            "comment-3",
            DISCUSSION_ID,
            BLOCK_ID,
            SOMEONE,
            15,
            vec![text_segment("Thanks")],
        );

        mock_comments(&app, vec![mention.clone(), my_reply.clone()]).await;
        handle(&app, &workspace_id, "comment-2", ME, &[ME]).await;
        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Deleted);

        // Their reply mentions no one: the followed thread brings it back
        mock_comments(&app, vec![mention, my_reply, their_reply]).await;
        handle(&app, &workspace_id, "comment-3", SOMEONE, &[ME]).await;
        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
        assert_eq!(
            notifications[0].updated_at.to_rfc3339(),
            "2026-10-10T09:15:00+00:00"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_deleted_discussion_deletes_the_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_mentioning_discussion(&app, MY_TOKEN).await;
        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;
        assert_eq!(notion_notifications(&app, vec![]).await.len(), 1);

        mock_comments(&app, vec![]).await;
        handle_event(
            &app,
            notion_comment_event(
                &workspace_id,
                "comment.deleted",
                "comment-1",
                SOMEONE,
                DISCUSSION_ID,
                PAGE_ID,
                BLOCK_ID,
                &[ME],
            ),
        )
        .await;

        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Deleted);
    }

    /// The first workspace connection cannot see the page: the triage tries
    /// the next one, and a user without access does not stop the others.
    #[rstest]
    #[tokio::test]
    async fn test_a_connection_without_access_does_not_stop_the_others(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let (workspace_id, _) = connect_me_and_someone(&app, &settings).await;
        mock_notion_not_accessible(&app.app.notion_mock_server, SOMEONE_TOKEN).await;
        mock_discussion_mentioning_both(&app, MY_TOKEN).await;

        handle(
            &app,
            &workspace_id,
            "comment-1",
            "a-colleague",
            &[SOMEONE, ME],
        )
        .await;

        let notifications = notion_notifications(&app, vec![]).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
    }

    /// A Notion failure other than no access fails the job once the other
    /// connections are saved, so that the job is retried.
    #[rstest]
    #[tokio::test]
    async fn test_a_failing_connection_fails_the_job_after_saving_the_others(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let (workspace_id, someone_client) = connect_me_and_someone(&app, &settings).await;
        mock_notion_failure_after_triage(
            &app.app.notion_mock_server,
            "/v1/comments",
            SOMEONE_TOKEN,
        )
        .await;
        mock_discussion_mentioning_both(&app, SOMEONE_TOKEN).await;
        mock_discussion_mentioning_both(&app, MY_TOKEN).await;
        let event = notion_comment_event(
            &workspace_id,
            "comment.created",
            "comment-1",
            "a-colleague",
            DISCUSSION_ID,
            PAGE_ID,
            BLOCK_ID,
            &[SOMEONE, ME],
        );

        let result = run_job(&app, event.clone()).await;

        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to save the Notion event for 1 connection(s)")
        );
        assert_eq!(notion_notifications_of(&app, &app.client).await, 1);
        assert_eq!(notion_notifications_of(&app, &someone_client).await, 0);

        // The retry, once Notion answers again
        app.app.notion_mock_server.reset().await;
        mock_discussion_mentioning_both(&app, SOMEONE_TOKEN).await;
        mock_discussion_mentioning_both(&app, MY_TOKEN).await;
        run_job(&app, event).await.unwrap();

        assert_eq!(notion_notifications_of(&app, &app.client).await, 1);
        assert_eq!(notion_notifications_of(&app, &someone_client).await, 1);
    }

    async fn mock_discussion_mentioning_both(app: &AuthenticatedApp, access_token: &str) {
        mock_notion_discussion(
            &app.app.notion_mock_server,
            access_token,
            PAGE_ID,
            "page-owner",
            BLOCK_ID,
            "block-owner",
            vec![notion_comment(
                "comment-1",
                DISCUSSION_ID,
                BLOCK_ID,
                "a-colleague",
                5,
                vec![
                    user_mention_segment(SOMEONE, "Someone"),
                    user_mention_segment(ME, "Me"),
                ],
            )],
        )
        .await;
    }

    /// Notion may deliver an event more than once: the discussion stays one
    /// notification, and a redelivery does not bring a read one back.
    #[rstest]
    #[tokio::test]
    async fn test_redelivered_comment_event_creates_one_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_mentioning_discussion(&app, MY_TOKEN).await;

        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;
        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;
        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);

        set_status(&app, &notifications[0], NotificationStatus::Read).await;
        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;
        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Read);
    }

    #[rstest]
    #[case::deleted(NotificationStatus::Deleted, false, NotificationStatus::Unread)]
    #[case::unsubscribed(
        NotificationStatus::Unsubscribed,
        false,
        NotificationStatus::Unsubscribed
    )]
    #[case::unsubscribed_then_mentioned(
        NotificationStatus::Unsubscribed,
        true,
        NotificationStatus::Unread
    )]
    #[tokio::test]
    async fn test_new_reply_after_delete_or_unsubscribe(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        #[case] dismissed_as: NotificationStatus,
        #[case] reply_mentions_me: bool,
        #[case] status_after_reply: NotificationStatus,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        let mention = notion_comment(
            "comment-1",
            DISCUSSION_ID,
            BLOCK_ID,
            SOMEONE,
            5,
            vec![text_segment("Ping "), user_mention_segment(ME, "Me")],
        );
        let reply = notion_comment(
            "comment-2",
            DISCUSSION_ID,
            BLOCK_ID,
            SOMEONE,
            15,
            if reply_mentions_me {
                vec![text_segment("Any news "), user_mention_segment(ME, "Me")]
            } else {
                vec![text_segment("Any news?")]
            },
        );
        mock_comments(&app, vec![mention.clone()]).await;
        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;
        let notifications = notion_notifications(&app, all_statuses()).await;
        set_status(&app, &notifications[0], dismissed_as).await;

        mock_comments(&app, vec![mention, reply]).await;
        handle(&app, &workspace_id, "comment-2", SOMEONE, &[ME]).await;

        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, status_after_reply);
    }

    pub(super) async fn set_status(
        app: &AuthenticatedApp,
        notification: &Notification,
        status: NotificationStatus,
    ) {
        update_notification(
            app,
            notification.id,
            &NotificationPatch {
                status: Some(status),
                ..Default::default()
            },
            app.user.id,
        )
        .await;
    }

    async fn mock_mentioning_discussion(app: &AuthenticatedApp, access_token: &str) {
        mock_notion_discussion(
            &app.app.notion_mock_server,
            access_token,
            PAGE_ID,
            SOMEONE,
            BLOCK_ID,
            SOMEONE,
            vec![notion_comment(
                "comment-1",
                DISCUSSION_ID,
                BLOCK_ID,
                SOMEONE,
                5,
                vec![user_mention_segment(ME, "Me")],
            )],
        )
        .await;
    }

    /// Replace the mocked discussion with `comments`.
    async fn mock_comments(app: &AuthenticatedApp, comments: Vec<Value>) {
        mock_my_discussion(app, SOMEONE, comments).await;
    }

    async fn handle(
        app: &AuthenticatedApp,
        workspace_id: &str,
        comment_id: &str,
        author_id: &str,
        accessible_by: &[&str],
    ) {
        handle_event(
            app,
            notion_comment_event(
                workspace_id,
                "comment.created",
                comment_id,
                author_id,
                DISCUSSION_ID,
                PAGE_ID,
                BLOCK_ID,
                accessible_by,
            ),
        )
        .await;
    }

    async fn handle_event(app: &AuthenticatedApp, event: Value) {
        let event: NotionWebhookEvent = serde_json::from_value(event).unwrap();
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        set_audit_actor(&mut transaction, &notion_webhook_actor()).await;

        handle_notion_comment_event(
            &mut transaction,
            &event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
        )
        .await
        .unwrap()
        .into_result()
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();
    }

    pub(super) fn all_statuses() -> Vec<NotificationStatus> {
        vec![
            NotificationStatus::Unread,
            NotificationStatus::Read,
            NotificationStatus::Deleted,
            NotificationStatus::Unsubscribed,
        ]
    }

    pub(super) async fn notion_notifications(
        app: &AuthenticatedApp,
        status_filter: Vec<NotificationStatus>,
    ) -> Vec<Notification> {
        list_notifications(
            &app.client,
            &app.app.api_address,
            status_filter,
            false,
            None,
            Some(NotificationSourceKind::Notion),
            false,
        )
        .await
    }
}

mod mention {
    use super::{
        job::{all_statuses, notion_notifications},
        *,
    };
    use pretty_assertions::assert_eq;

    const TABLE_BLOCK_ID: &str = "c1c2c3c4-0000-4000-8000-0000000000cc";

    fn in_page() -> Value {
        json!({ "type": "page_id", "page_id": PAGE_ID })
    }

    fn mentioning_block(last_edited_by: &str, minute: u32, text: &str) -> Value {
        notion_paragraph_block(
            BLOCK_ID,
            in_page(),
            last_edited_by,
            minute,
            vec![text_segment(text), user_mention_segment(ME, "Me")],
        )
    }

    fn plain_block(minute: u32, text: &str) -> Value {
        notion_paragraph_block(
            BLOCK_ID,
            in_page(),
            SOMEONE,
            minute,
            vec![text_segment(text)],
        )
    }

    /// A Notion failure other than no access fails the job once the other
    /// connections are saved, so that the job is retried.
    #[rstest]
    #[tokio::test]
    async fn test_a_failing_connection_fails_the_job_after_saving_the_others(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let (workspace_id, someone_client) = connect_me_and_someone(&app, &settings).await;
        let block_path = format!("/v1/blocks/{BLOCK_ID}");
        mock_notion_failure_after_triage(&app.app.notion_mock_server, &block_path, SOMEONE_TOKEN)
            .await;
        mock_block_mentioning_both(&app, SOMEONE_TOKEN).await;
        mock_block_mentioning_both(&app, MY_TOKEN).await;
        let event = notion_page_content_event(
            &workspace_id,
            "event-1",
            PAGE_ID,
            &[BLOCK_ID],
            &[SOMEONE, ME],
        );

        let result = run_job(&app, event.clone()).await;

        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to save the Notion event for 1 connection(s)")
        );
        assert_eq!(notion_notifications_of(&app, &app.client).await, 1);
        assert_eq!(notion_notifications_of(&app, &someone_client).await, 0);

        // The retry, once Notion answers again
        app.app.notion_mock_server.reset().await;
        mock_block_mentioning_both(&app, SOMEONE_TOKEN).await;
        mock_block_mentioning_both(&app, MY_TOKEN).await;
        run_job(&app, event).await.unwrap();

        assert_eq!(notion_notifications_of(&app, &app.client).await, 1);
        assert_eq!(notion_notifications_of(&app, &someone_client).await, 1);
    }

    async fn mock_block_mentioning_both(app: &AuthenticatedApp, access_token: &str) {
        mock_notion_blocks(
            &app.app.notion_mock_server,
            access_token,
            PAGE_ID,
            vec![notion_paragraph_block(
                BLOCK_ID,
                in_page(),
                "a-colleague",
                5,
                vec![
                    user_mention_segment(SOMEONE, "Someone"),
                    user_mention_segment(ME, "Me"),
                ],
            )],
        )
        .await;
    }

    /// Replace the mocked blocks with `blocks`.
    async fn mock_blocks(app: &AuthenticatedApp, blocks: Vec<Value>) {
        app.app.notion_mock_server.reset().await;
        mock_notion_blocks(&app.app.notion_mock_server, MY_TOKEN, PAGE_ID, blocks).await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_mention_creates_an_unread_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_blocks(&app, vec![mentioning_block(SOMEONE, 5, "Can you check ")]).await;

        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        let notifications = notion_notifications(&app, vec![]).await;
        assert_eq!(notifications.len(), 1);
        let notification = &notifications[0];
        assert_eq!(notification.status, NotificationStatus::Unread);
        assert_eq!(
            notification.title,
            format!("Mentioned by User {SOMEONE} on Roadmap")
        );
        assert_eq!(
            notification.updated_at.to_rfc3339(),
            "2026-10-10T09:05:00+00:00"
        );
        let ThirdPartyItemData::NotionBlock(block) = &notification.source_item.data else {
            panic!("Expected a NotionBlock item");
        };
        assert_eq!(block.id, BLOCK_ID);
        assert_eq!(block.page.id, PAGE_ID);
        assert_eq!(to_plain_text(&block.rich_text), "Can you check @Me");
    }

    #[rstest]
    #[tokio::test]
    async fn test_repeated_block_creates_one_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_blocks(&app, vec![mentioning_block(SOMEONE, 5, "Can you check ")]).await;

        handle(&app, &workspace_id, &[BLOCK_ID, BLOCK_ID], &[ME]).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_edit_keeping_the_mention_leaves_the_notification_as_is(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_blocks(&app, vec![mentioning_block(SOMEONE, 5, "Can you check ")]).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;
        let notification = &notion_notifications(&app, vec![]).await[0];
        update_notification(
            &app,
            notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Read),
                ..Default::default()
            },
            app.user.id,
        )
        .await;

        mock_blocks(&app, vec![mentioning_block(SOMEONE, 7, "Could you check ")]).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Read);
        let ThirdPartyItemData::NotionBlock(block) = &notifications[0].source_item.data else {
            panic!("Expected a NotionBlock item");
        };
        assert_eq!(to_plain_text(&block.rich_text), "Could you check @Me");
    }

    #[rstest]
    #[tokio::test]
    async fn test_removed_mention_deletes_the_notification_and_a_new_one_restores_it(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_blocks(&app, vec![mentioning_block(SOMEONE, 5, "Can you check ")]).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        mock_blocks(&app, vec![plain_block(7, "Never mind")]).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;
        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Deleted);

        mock_blocks(&app, vec![mentioning_block(SOMEONE, 9, "Actually, ")]).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;
        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_deleted_block_deletes_the_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_blocks(&app, vec![mentioning_block(SOMEONE, 5, "Can you check ")]).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        app.app.notion_mock_server.reset().await;
        mock_notion_deleted_block(&app.app.notion_mock_server, MY_TOKEN, PAGE_ID, BLOCK_ID).await;
        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        let notifications = notion_notifications(&app, all_statuses()).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_mentioning_myself_creates_nothing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_blocks(&app, vec![mentioning_block(ME, 5, "Reminder for ")]).await;

        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_disabled_mention_sync_creates_nothing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(
            &app,
            &settings,
            NotionConfig {
                sync_comments_enabled: true,
                sync_mentions_enabled: false,
            },
        )
        .await
        .workspace_id;
        mock_blocks(&app, vec![mentioning_block(SOMEONE, 5, "Can you check ")]).await;

        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_page_not_accessible_to_me_creates_nothing(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        mock_blocks(&app, vec![mentioning_block(SOMEONE, 5, "Can you check ")]).await;

        handle(&app, &workspace_id, &[BLOCK_ID], &[SOMEONE]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
    }

    /// A table cell mention: the row is under its table block, itself under
    /// the page.
    #[rstest]
    #[tokio::test]
    async fn test_mention_in_a_nested_table_row_creates_a_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        let table = json!({
            "object": "block",
            "id": TABLE_BLOCK_ID,
            "parent": in_page(),
            "type": "table",
            "table": { "table_width": 2 }
        });
        let row = json!({
            "object": "block",
            "id": BLOCK_ID,
            "parent": { "type": "block_id", "block_id": TABLE_BLOCK_ID },
            "type": "table_row",
            "last_edited_by": { "object": "user", "id": SOMEONE },
            "last_edited_time": "2026-10-10T09:05:00.000Z",
            "table_row": {
                "cells": [
                    [text_segment("Owner")],
                    [user_mention_segment(ME, "Me")]
                ]
            }
        });
        mock_blocks(&app, vec![table, row]).await;

        handle(&app, &workspace_id, &[BLOCK_ID], &[ME]).await;

        let notifications = notion_notifications(&app, vec![]).await;
        assert_eq!(notifications.len(), 1);
        let ThirdPartyItemData::NotionBlock(block) = &notifications[0].source_item.data else {
            panic!("Expected a NotionBlock item");
        };
        assert_eq!(block.block_type, "table_row");
        assert_eq!(block.page.id, PAGE_ID);
        assert_eq!(to_plain_text(&block.rich_text), "Owner | @Me");
    }

    #[rstest]
    #[tokio::test]
    async fn test_only_the_first_blocks_of_a_big_update_are_read(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled())
            .await
            .workspace_id;
        let block_ids: Vec<String> = (0..60)
            .map(|index| format!("d0d0d0d0-0000-4000-8000-{index:012}"))
            .collect();
        let blocks = block_ids
            .iter()
            .map(|block_id| {
                notion_paragraph_block(block_id, in_page(), SOMEONE, 5, vec![text_segment("x")])
            })
            .collect();
        mock_blocks(&app, blocks).await;

        let block_ids: Vec<&str> = block_ids.iter().map(String::as_str).collect();
        handle(&app, &workspace_id, &block_ids, &[ME]).await;

        let block_reads = app
            .app
            .notion_mock_server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path().starts_with("/v1/blocks/"))
            .count();
        assert_eq!(block_reads, 50);
    }

    async fn handle(
        app: &AuthenticatedApp,
        workspace_id: &str,
        block_ids: &[&str],
        accessible_by: &[&str],
    ) {
        let event: NotionWebhookEvent = serde_json::from_value(notion_page_content_event(
            workspace_id,
            &format!("event-{}", Uuid::new_v4()),
            PAGE_ID,
            block_ids,
            accessible_by,
        ))
        .unwrap();
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        set_audit_actor(&mut transaction, &notion_webhook_actor()).await;

        handle_notion_page_content_event(
            &mut transaction,
            &event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
        )
        .await
        .unwrap()
        .into_result()
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();
    }
}

fn comment_event(workspace_id: &str, event_type: &str, comment_id: &str) -> Value {
    notion_comment_event(
        workspace_id,
        event_type,
        comment_id,
        SOMEONE,
        DISCUSSION_ID,
        PAGE_ID,
        BLOCK_ID,
        &[ME],
    )
}
