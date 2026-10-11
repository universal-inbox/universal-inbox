use anyhow::Context;
use redis_apalis::AsyncCommands;
use rstest::*;
use serde_json::{Value, json};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::integrations::notion::NotionConfig,
    notification::{Notification, NotificationSourceKind, NotificationStatus},
    third_party::item::ThirdPartyItemData,
};
use universal_inbox_api::{
    configuration::Settings, integrations::notion::webhook::NotionWebhookEvent,
    jobs::notion::handle_notion_comment_event,
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticate_user, authenticated_app},
    notification::list_notifications,
    notion::{
        create_notion_integration_connection, mock_notion_discussion, mock_notion_not_accessible,
        notion_comment, notion_comment_event, post_signed_notion_event, text_segment,
        user_mention_segment,
    },
    settings,
};

const ME: &str = "notion-user-me";
const MY_TOKEN: &str = "notion-token-me";
const SOMEONE: &str = "notion-user-someone";
const PAGE_ID: &str = "3c612f56-fdd0-4a30-a4d6-bda7d7426309";
const BLOCK_ID: &str = "b1b2b3b4-0000-4000-8000-0000000000bb";
const DISCUSSION_ID: &str = "a2b6c3d4-0000-4000-8000-0000000000aa";

/// A workspace id unique to the test, so the cached workspace lookup of one
/// test never answers for another.
fn new_workspace_id() -> String {
    Uuid::new_v4().to_string()
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

    /// Counts the jobs pushed to the test's queue, whatever their state (see
    /// `test_slack_webhook_message.rs`).
    async fn pushed_jobs_count(app: &TestedApp) -> usize {
        app.redis_storage
            .get_connection()
            .clone()
            .hlen(app.redis_storage.get_config().job_data_hash())
            .await
            .expect("Failed to get jobs count")
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
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled()).await;
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
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled()).await;
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
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled()).await;
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
        .await;
        mock_mentioning_discussion(&app, MY_TOKEN).await;

        handle(&app, &workspace_id, "comment-1", SOMEONE, &[ME]).await;

        assert!(notion_notifications(&app, all_statuses()).await.is_empty());
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
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled()).await;
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
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled()).await;
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
        let workspace_id = connect_me(&app, &settings, NotionConfig::enabled()).await;
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
        let workspace_id = new_workspace_id();
        let (_, other_user) = authenticate_user(
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
            &settings,
            SOMEONE,
            "notion-token-someone",
            &workspace_id,
            NotionConfig::enabled(),
        )
        .await;
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
        mock_notion_not_accessible(&app.app.notion_mock_server, "notion-token-someone").await;
        mock_notion_discussion(
            &app.app.notion_mock_server,
            MY_TOKEN,
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

    async fn connect_me(
        app: &AuthenticatedApp,
        settings: &Settings,
        config: NotionConfig,
    ) -> String {
        let workspace_id = new_workspace_id();
        create_notion_integration_connection(
            &app.app,
            app.user.id,
            settings,
            ME,
            MY_TOKEN,
            &workspace_id,
            config,
        )
        .await;
        workspace_id
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
        app.app.notion_mock_server.reset().await;
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

        handle_notion_comment_event(
            &mut transaction,
            &event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();
    }

    fn all_statuses() -> Vec<NotificationStatus> {
        vec![
            NotificationStatus::Unread,
            NotificationStatus::Read,
            NotificationStatus::Deleted,
            NotificationStatus::Unsubscribed,
        ]
    }

    async fn notion_notifications(
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
