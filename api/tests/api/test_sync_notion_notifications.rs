use chrono::{DateTime, TimeDelta, Utc};
use rstest::*;
use serde_json::{Value, json};

use universal_inbox::{
    integration_connection::{IntegrationConnection, integrations::notion::NotionConfig},
    notification::{
        Notification, NotificationSourceKind, NotificationStatus, NotificationSyncSourceKind,
        service::NotificationPatch,
    },
    third_party::{
        integrations::notion::{
            NotionComment, NotionCommentMessage, NotionCommentParent, NotionPage, NotionUser,
            to_plain_text,
        },
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::{
    configuration::Settings,
    integrations::notion::{NotionService, webhook::NotionWebhookEvent},
    jobs::notion::handle_notion_page_content_event,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::get_integration_connection,
    notification::{
        create_notification_from_source_item, list_notifications, sync_notifications,
        update_notification,
    },
    notion::{
        BLOCK_ID, DISCUSSION_ID, ME, MY_TOKEN, PAGE_ID, SOMEONE, connect_me, mock_my_discussion,
        mock_notion_block_children, mock_notion_blocks, mock_notion_page_and_users,
        mock_notion_search, notion_page_content_event, notion_search_page, text_segment,
        user_mention_segment,
    },
    settings,
};

fn minutes_ago(minutes: i64) -> DateTime<Utc> {
    Utc::now() - TimeDelta::minutes(minutes)
}

fn paragraph(
    last_edited_by: &str,
    last_edited_time: DateTime<Utc>,
    rich_text: Vec<Value>,
) -> Value {
    json!({
        "object": "block",
        "id": BLOCK_ID,
        "parent": { "type": "page_id", "page_id": PAGE_ID },
        "type": "paragraph",
        "has_children": false,
        "last_edited_by": { "object": "user", "id": last_edited_by },
        "last_edited_time": last_edited_time.to_rfc3339(),
        "paragraph": { "rich_text": rich_text }
    })
}

fn mentioning_paragraph(last_edited_by: &str, last_edited_time: DateTime<Utc>) -> Value {
    paragraph(
        last_edited_by,
        last_edited_time,
        vec![
            text_segment("Can you check "),
            user_mention_segment(ME, "Me"),
        ],
    )
}

/// Replace the mocked Notion workspace: `PAGE_ID`, edited `page_edited`
/// minutes ago, holds `blocks`.
async fn mock_page(app: &AuthenticatedApp, page_edited: i64, blocks: Vec<Value>) {
    let server = &app.app.notion_mock_server;
    server.reset().await;
    mock_notion_search(
        server,
        MY_TOKEN,
        vec![notion_search_page(PAGE_ID, minutes_ago(page_edited))],
        None,
    )
    .await;
    mock_notion_block_children(server, MY_TOKEN, PAGE_ID, blocks).await;
    mock_notion_page_and_users(server, MY_TOKEN, PAGE_ID).await;
}

fn mentions_only() -> NotionConfig {
    NotionConfig {
        sync_comments_enabled: false,
        sync_mentions_enabled: true,
    }
}

fn comments_only() -> NotionConfig {
    NotionConfig {
        sync_comments_enabled: true,
        sync_mentions_enabled: false,
    }
}

/// Sync Notion, ignoring the minimum interval between syncs.
async fn force_sync(app: &AuthenticatedApp) {
    app.app
        .notification_service
        .read()
        .await
        .sync_notifications_with_transaction(NotificationSyncSourceKind::Notion, app.user.id, true)
        .await
        .unwrap();
}

async fn notion_notifications(app: &AuthenticatedApp) -> Vec<Notification> {
    list_notifications(
        &app.client,
        &app.app.api_address,
        vec![
            NotificationStatus::Unread,
            NotificationStatus::Read,
            NotificationStatus::Deleted,
        ],
        false,
        None,
        Some(NotificationSourceKind::Notion),
        false,
    )
    .await
}

async fn requests_count(app: &AuthenticatedApp, path_prefix: &str) -> usize {
    app.app
        .notion_mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path().starts_with(path_prefix))
        .count()
}

async fn mark_as_read(app: &AuthenticatedApp, notification: &Notification) {
    update_notification(
        app,
        notification.id,
        &NotificationPatch {
            status: Some(NotificationStatus::Read),
            ..Default::default()
        },
        app.user.id,
    )
    .await;
}

mod mentions {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_sync_creates_a_missed_mention(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, mentions_only())
            .await
            .connection;
        mock_page(&app, 1, vec![mentioning_paragraph(SOMEONE, minutes_ago(1))]).await;

        let synced = sync_notifications(
            &app.client,
            &app.app.api_address,
            Some(NotificationSourceKind::Notion),
            false,
        )
        .await;

        assert_eq!(synced.len(), 1);
        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
        assert_eq!(
            notifications[0].title,
            format!("Mentioned by User {SOMEONE} on Roadmap")
        );
        let ThirdPartyItemData::NotionBlock(block) = &notifications[0].source_item.data else {
            panic!("Expected a NotionBlock item");
        };
        assert_eq!(block.page.id, PAGE_ID);
        assert_eq!(block.user_notion_id.as_deref(), Some(ME));
        assert_eq!(to_plain_text(&block.rich_text), "Can you check @Me");
        let connection = get_integration_connection(&app, connection.id)
            .await
            .unwrap();
        assert!(connection.last_notifications_sync_completed_at.is_some());
    }

    /// The webhook saw the mention first: the hourly sync finds the same
    /// block and must not notify it a second time.
    #[rstest]
    #[tokio::test]
    async fn test_webhook_mention_is_not_duplicated_by_the_sync(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let workspace_id = connect_me(&app, &settings, mentions_only())
            .await
            .workspace_id;
        let block = mentioning_paragraph(SOMEONE, minutes_ago(1));
        mock_page(&app, 1, vec![block.clone()]).await;
        mock_notion_blocks(&app.app.notion_mock_server, MY_TOKEN, PAGE_ID, vec![block]).await;
        handle_page_content_event(&app, &workspace_id).await;
        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        mark_as_read(&app, &notifications[0]).await;

        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Read);
    }

    async fn handle_page_content_event(app: &AuthenticatedApp, workspace_id: &str) {
        let event: NotionWebhookEvent = serde_json::from_value(notion_page_content_event(
            workspace_id,
            "event-1",
            PAGE_ID,
            &[BLOCK_ID],
            &[ME],
        ))
        .unwrap();
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();
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
        transaction.commit().await.unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn test_known_mention_is_not_notified_again(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, mentions_only()).await;
        mock_page(&app, 1, vec![mentioning_paragraph(SOMEONE, minutes_ago(1))]).await;
        force_sync(&app).await;
        mark_as_read(&app, &notion_notifications(&app).await[0]).await;

        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Read);
    }

    /// The stored blocks of a page are read together: each keeps its own
    /// notification.
    #[rstest]
    #[tokio::test]
    async fn test_known_mentions_of_one_page_are_not_notified_again(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, mentions_only()).await;
        let mut other_block = mentioning_paragraph(SOMEONE, minutes_ago(1));
        other_block["id"] = json!("b1b2b3b4-0000-4000-8000-0000000000bc");
        let blocks = vec![mentioning_paragraph(SOMEONE, minutes_ago(1)), other_block];
        mock_page(&app, 1, blocks.clone()).await;
        force_sync(&app).await;
        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 2);
        for notification in &notifications {
            mark_as_read(&app, notification).await;
        }

        mock_page(&app, 1, blocks).await;
        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 2);
        assert!(
            notifications
                .iter()
                .all(|notification| notification.status == NotificationStatus::Read)
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_removed_mention_deletes_the_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, mentions_only()).await;
        mock_page(&app, 1, vec![mentioning_paragraph(SOMEONE, minutes_ago(1))]).await;
        force_sync(&app).await;

        mock_page(
            &app,
            0,
            vec![paragraph(
                SOMEONE,
                minutes_ago(0),
                vec![text_segment("Never mind")],
            )],
        )
        .await;
        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_block_moved_to_the_trash_deletes_the_notification(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, mentions_only()).await;
        mock_page(&app, 1, vec![mentioning_paragraph(SOMEONE, minutes_ago(1))]).await;
        force_sync(&app).await;
        mark_as_read(&app, &notion_notifications(&app).await[0]).await;

        let mut trashed_block = mentioning_paragraph(SOMEONE, minutes_ago(0));
        trashed_block["in_trash"] = json!(true);
        mock_page(&app, 0, vec![trashed_block]).await;
        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_mention_written_by_me_is_ignored(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, mentions_only()).await;
        mock_page(&app, 1, vec![mentioning_paragraph(ME, minutes_ago(1))]).await;

        force_sync(&app).await;

        assert!(notion_notifications(&app).await.is_empty());
    }

    /// A block edited before the connection was created is not a missed
    /// mention.
    #[rstest]
    #[tokio::test]
    async fn test_block_edited_before_the_cursor_is_ignored(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, mentions_only()).await;
        mock_page(
            &app,
            1,
            vec![mentioning_paragraph(SOMEONE, minutes_ago(60))],
        )
        .await;

        force_sync(&app).await;

        assert!(notion_notifications(&app).await.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_search_stops_at_the_first_page_older_than_the_cursor(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, mentions_only()).await;
        let server = &app.app.notion_mock_server;
        let old_page_id = "d0d0d0d0-0000-4000-8000-0000000000dd";
        mock_notion_search(
            server,
            MY_TOKEN,
            vec![
                notion_search_page(PAGE_ID, minutes_ago(1)),
                notion_search_page(old_page_id, minutes_ago(60)),
            ],
            Some("next-search-page"),
        )
        .await;
        mock_notion_block_children(server, MY_TOKEN, PAGE_ID, vec![]).await;
        mock_notion_page_and_users(server, MY_TOKEN, PAGE_ID).await;

        force_sync(&app).await;

        assert_eq!(requests_count(&app, "/v1/search").await, 1);
        assert_eq!(
            requests_count(&app, &format!("/v1/blocks/{old_page_id}")).await,
            0
        );
        assert_eq!(
            requests_count(&app, &format!("/v1/blocks/{PAGE_ID}/children")).await,
            1
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_disabled_mentions_are_not_searched(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        connect_me(&app, &settings, comments_only()).await;
        mock_page(&app, 1, vec![mentioning_paragraph(SOMEONE, minutes_ago(1))]).await;

        force_sync(&app).await;

        assert_eq!(requests_count(&app, "/v1/search").await, 0);
        assert!(notion_notifications(&app).await.is_empty());
    }
}

mod comments {
    use super::*;
    use pretty_assertions::assert_eq;

    const OTHER_DISCUSSION_ID: &str = "a2b6c3d4-0000-4000-8000-0000000000ab";

    fn comment(id: &str, author_id: &str, time: DateTime<Utc>, text: &str) -> Value {
        comment_in(DISCUSSION_ID, id, author_id, time, text)
    }

    fn comment_in(
        discussion_id: &str,
        id: &str,
        author_id: &str,
        time: DateTime<Utc>,
        text: &str,
    ) -> Value {
        json!({
            "object": "comment",
            "id": id,
            "parent": { "type": "block_id", "block_id": BLOCK_ID },
            "discussion_id": discussion_id,
            "created_time": time.to_rfc3339(),
            "last_edited_time": time.to_rfc3339(),
            "created_by": { "object": "user", "id": author_id },
            "rich_text": [text_segment(text)]
        })
    }

    /// Store the discussion as the webhook left it: one comment from
    /// someone, notified and then read.
    async fn track_discussion(
        app: &AuthenticatedApp,
        connection: &IntegrationConnection,
        discussion_id: &str,
        first_comment_time: DateTime<Utc>,
    ) {
        let discussion = NotionComment {
            discussion_id: discussion_id.to_string(),
            page: NotionPage {
                id: PAGE_ID.to_string(),
                title: "Roadmap".to_string(),
                url: "https://www.notion.so/Roadmap-3c612f56".parse().unwrap(),
                created_by: Some(SOMEONE.to_string()),
            },
            parent: NotionCommentParent::Block {
                block_id: BLOCK_ID.to_string(),
            },
            quoted_content: None,
            parent_block_created_by: Some(ME.to_string()),
            comments: vec1::vec1![NotionCommentMessage {
                id: "comment-1".to_string(),
                author: NotionUser {
                    id: SOMEONE.to_string(),
                    name: None,
                    avatar_url: None,
                },
                rich_text: vec![],
                created_time: first_comment_time,
                last_edited_time: first_comment_time,
            }],
            user_notion_id: Some(ME.to_string()),
        };
        let notification = create_notification_from_source_item::<NotionComment, NotionService>(
            &app.app,
            discussion_id.to_string(),
            ThirdPartyItemData::NotionComment(Box::new(discussion)),
            app.app
                .notification_service
                .read()
                .await
                .notion_service
                .clone(),
            app.user.id,
            connection.id,
        )
        .await;
        mark_as_read(app, &notification).await;
    }

    async fn mock_remote_discussion(app: &AuthenticatedApp, comments: Vec<Value>) {
        mock_my_discussion(app, ME, comments).await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_missed_reply_reopens_the_discussion(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, comments_only())
            .await
            .connection;
        let first_comment_time = minutes_ago(120);
        track_discussion(&app, &connection, DISCUSSION_ID, first_comment_time).await;
        mock_remote_discussion(
            &app,
            vec![
                comment("comment-1", SOMEONE, first_comment_time, "Looks good"),
                comment("comment-2", SOMEONE, minutes_ago(10), "Any news?"),
            ],
        )
        .await;

        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
        let ThirdPartyItemData::NotionComment(discussion) = &notifications[0].source_item.data
        else {
            panic!("Expected a NotionComment item");
        };
        assert_eq!(discussion.comments.len(), 2);
    }

    #[rstest]
    #[case::reply(false, NotificationStatus::Unsubscribed)]
    #[case::reply_mentioning_me(true, NotificationStatus::Unread)]
    #[tokio::test]
    async fn test_missed_reply_in_an_unsubscribed_discussion(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        #[case] reply_mentions_me: bool,
        #[case] expected_status: NotificationStatus,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, comments_only())
            .await
            .connection;
        let first_comment_time = minutes_ago(120);
        track_discussion(&app, &connection, DISCUSSION_ID, first_comment_time).await;
        update_notification(
            &app,
            notion_notifications(&app).await[0].id,
            &NotificationPatch {
                status: Some(NotificationStatus::Unsubscribed),
                ..Default::default()
            },
            app.user.id,
        )
        .await;
        let mut reply = comment("comment-2", SOMEONE, minutes_ago(10), "Any news ");
        if reply_mentions_me {
            reply["rich_text"]
                .as_array_mut()
                .unwrap()
                .push(user_mention_segment(ME, "Me"));
        }
        mock_remote_discussion(
            &app,
            vec![
                comment("comment-1", SOMEONE, first_comment_time, "Looks good"),
                reply,
            ],
        )
        .await;

        force_sync(&app).await;

        let notifications = list_notifications(
            &app.client,
            &app.app.api_address,
            vec![expected_status],
            false,
            None,
            Some(NotificationSourceKind::Notion),
            false,
        )
        .await;
        assert_eq!(notifications.len(), 1);
    }

    /// Discussions on the same page read it once.
    #[rstest]
    #[tokio::test]
    async fn test_missed_replies_on_one_page_read_it_once(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, comments_only())
            .await
            .connection;
        let first_comment_time = minutes_ago(120);
        track_discussion(&app, &connection, DISCUSSION_ID, first_comment_time).await;
        track_discussion(&app, &connection, OTHER_DISCUSSION_ID, first_comment_time).await;
        mock_remote_discussion(
            &app,
            vec![
                comment("comment-1", SOMEONE, first_comment_time, "Looks good"),
                comment_in(
                    OTHER_DISCUSSION_ID,
                    "comment-3",
                    SOMEONE,
                    first_comment_time,
                    "Typo here",
                ),
                comment("comment-2", SOMEONE, minutes_ago(10), "Any news?"),
                comment_in(
                    OTHER_DISCUSSION_ID,
                    "comment-4",
                    SOMEONE,
                    minutes_ago(10),
                    "Fixed?",
                ),
            ],
        )
        .await;

        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 2);
        assert!(
            notifications
                .iter()
                .all(|notification| notification.status == NotificationStatus::Unread)
        );
        assert_eq!(requests_count(&app, "/v1/pages/").await, 1);
        assert_eq!(requests_count(&app, "/v1/users/").await, 1);
    }

    #[rstest]
    #[tokio::test]
    async fn test_discussion_without_new_reply_is_untouched(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, comments_only())
            .await
            .connection;
        let first_comment_time = minutes_ago(120);
        track_discussion(&app, &connection, DISCUSSION_ID, first_comment_time).await;
        mock_remote_discussion(
            &app,
            vec![comment(
                "comment-1",
                SOMEONE,
                first_comment_time,
                "Looks good",
            )],
        )
        .await;

        force_sync(&app).await;

        let notifications = notion_notifications(&app).await;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].status, NotificationStatus::Read);
        assert!(requests_count(&app, "/v1/comments").await > 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_discussions_are_re_read_at_most_hourly(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, comments_only())
            .await
            .connection;
        let first_comment_time = minutes_ago(120);
        track_discussion(&app, &connection, DISCUSSION_ID, first_comment_time).await;
        mock_remote_discussion(
            &app,
            vec![comment(
                "comment-1",
                SOMEONE,
                first_comment_time,
                "Looks good",
            )],
        )
        .await;
        force_sync(&app).await;
        let first_sync_reads = requests_count(&app, "/v1/comments").await;

        force_sync(&app).await;

        assert!(first_sync_reads > 0);
        assert_eq!(requests_count(&app, "/v1/comments").await, first_sync_reads);
    }

    #[rstest]
    #[tokio::test]
    async fn test_disabled_comments_are_not_re_read(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, mentions_only())
            .await
            .connection;
        track_discussion(&app, &connection, DISCUSSION_ID, minutes_ago(120)).await;
        mock_page(&app, 1, vec![]).await;

        force_sync(&app).await;

        assert_eq!(requests_count(&app, "/v1/comments").await, 0);
    }

    #[rstest]
    #[tokio::test]
    async fn test_discussions_inactive_for_two_weeks_are_not_re_read(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let connection = connect_me(&app, &settings, comments_only())
            .await
            .connection;
        let old_time = Utc::now() - TimeDelta::days(15);
        track_discussion(&app, &connection, DISCUSSION_ID, old_time).await;
        mock_remote_discussion(
            &app,
            vec![
                comment("comment-1", SOMEONE, old_time, "Looks good"),
                comment("comment-2", SOMEONE, minutes_ago(10), "Any news?"),
            ],
        )
        .await;

        force_sync(&app).await;

        assert_eq!(requests_count(&app, "/v1/comments").await, 0);
        assert_eq!(
            notion_notifications(&app).await[0].status,
            NotificationStatus::Read
        );
    }
}
