use std::str::FromStr;

use chrono::{Datelike, Duration, Utc};
use cucumber::{Parameter, given, then, when};
use email_address::EmailAddress;
use pretty_assertions::assert_eq;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::google_drive::{GoogleDriveConfig, GoogleDriveContext},
        provider::{IntegrationConnectionContext, IntegrationProvider},
    },
    notification::{
        NotificationSourceKind, NotificationStatus, NotificationWithTask,
        service::NotificationPatch,
    },
    pii::Pii,
    third_party::{integrations::google_drive::GoogleDriveComment, item::ThirdPartyItemData},
};
use universal_inbox_api::integrations::google_drive::{
    GoogleDriveAboutResponse, GoogleDriveCommentList, GoogleDriveFileList, GoogleDriveUserInfo,
    RawGoogleDriveComment, RawGoogleDriveCommentReply,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection, google_account_id,
            google_drive_oauth_credential,
        },
        notification::{
            google_drive::{
                assert_sync_notifications, create_notification_from_google_drive_comment,
                google_drive_comment_123, google_drive_comment_456, google_drive_comments_list,
                google_drive_files_list, mock_google_drive_comments_list_404_service,
                mock_google_drive_comments_list_service, mock_google_drive_files_list_service,
                mock_google_drive_get_user_info_service,
            },
            sync_notifications,
        },
        rest::get_resource,
    },
    state::CommentsOfFile,
    steps::{
        integration_connections::stored_connection, kept_triage::read_notification,
        notifications::update, notion_sync::Mention, status::UserStatus,
    },
    world::ApiWorld,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Newness {
    Newer,
    Older,
}

impl Parameter for Newness {
    const REGEX: &'static str = "newer|older";
    const NAME: &'static str = "newness";
}

impl FromStr for Newness {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "newer" => Ok(Self::Newer),
            "older" => Ok(Self::Older),
            other => Err(format!("Unknown newness {other:?}")),
        }
    }
}

fn about(name: &str, email: &str) -> GoogleDriveAboutResponse {
    GoogleDriveAboutResponse {
        user: GoogleDriveUserInfo {
            email_address: Pii::new(email.to_string()),
            display_name: Pii::new(name.to_string()),
        },
    }
}

fn page_size() -> usize {
    settings()
        .integrations
        .get("google_drive")
        .unwrap()
        .page_size
        .unwrap()
}

#[given("I am connected to Google Drive")]
async fn i_am_connected_to_google_drive(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
        &settings(),
        google_drive_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.google_drive_connection = Some(connection);
}

async fn connect_with_cached_user(world: &mut ApiWorld, google_account: Option<String>) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::GoogleDrive(GoogleDriveConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            provider_user_id: google_account.as_deref().map(google_account_id),
            ..google_drive_oauth_credential()
        },
        None,
        Some(IntegrationConnectionContext::GoogleDrive(
            GoogleDriveContext {
                user_email_address: Pii::<EmailAddress>::from_str("Previous@example.com").unwrap(),
                user_display_name: Pii::new("Previous User".to_string()),
            },
        )),
    )
    .await;
    world.integrations.google_drive_connection = Some(connection);
}

#[given(
    expr = "I am connected to Google Drive, which cached me as \"Previous User\" and is pinned to the Google account of {string}"
)]
async fn connected_with_a_pinned_account(world: &mut ApiWorld, google_account: String) {
    connect_with_cached_user(world, Some(google_account)).await;
}

#[given(
    expr = "I am connected to Google Drive, which cached me as \"Previous User\" and is not pinned to a Google account yet"
)]
async fn connected_without_a_pinned_account(world: &mut ApiWorld) {
    connect_with_cached_user(world, None).await;
}

#[given(expr = "Google Drive knows me as {string}, with the address {string}")]
fn google_drive_knows_me(world: &mut ApiWorld, name: String, email: String) {
    world.google_drive_sync.about = Some((name, email));
}

#[given("Google Drive lists no file")]
fn google_drive_lists_no_file(world: &mut ApiWorld) {
    world.google_drive_sync.files = Some(GoogleDriveFileList {
        files: None,
        incomplete_search: None,
        next_page_token: None,
    });
}

#[given("Google Drive lists two files, over two pages")]
fn google_drive_lists_two_files(world: &mut ApiWorld) {
    world.google_drive_sync.files = Some(google_drive_files_list());
}

#[given("Google Drive lists two files, on a single page")]
fn google_drive_lists_two_files_on_one_page(world: &mut ApiWorld) {
    world.google_drive_sync.files = Some(GoogleDriveFileList {
        next_page_token: None,
        ..google_drive_files_list()
    });
}

#[given("Google Drive lists a single file")]
fn google_drive_lists_a_single_file(world: &mut ApiWorld) {
    let list = google_drive_files_list();
    world.google_drive_sync.files = Some(GoogleDriveFileList {
        files: Some(vec![list.files.unwrap().remove(0)]),
        next_page_token: None,
        ..list
    });
}

#[given("Google Drive reports the fixture comments on the first file")]
fn google_drive_reports_the_comments(world: &mut ApiWorld) {
    world.google_drive_sync.comments.push((
        0,
        CommentsOfFile::Listed(GoogleDriveCommentList {
            next_page_token: None,
            ..google_drive_comments_list()
        }),
    ));
}

#[given(
    "Google Drive reports the fixture comments on the first file, both modified in five minutes"
)]
fn google_drive_reports_the_modified_comments(world: &mut ApiWorld) {
    let modified_time = Utc::now() + Duration::minutes(5);
    let list = google_drive_comments_list();
    let comments = list.comments.clone().unwrap();
    world.google_drive_sync.modified_time = Some(modified_time);
    world.google_drive_sync.comments.push((
        0,
        CommentsOfFile::Listed(GoogleDriveCommentList {
            comments: Some(vec![
                comments[0].clone(),
                RawGoogleDriveComment {
                    modified_time,
                    ..comments[1].clone()
                },
            ]),
            ..list
        }),
    ));
}

#[given("Google Drive reports no comment on the second file")]
fn google_drive_reports_no_comment_on_the_second_file(world: &mut ApiWorld) {
    world.google_drive_sync.comments.push((
        1,
        CommentsOfFile::Listed(GoogleDriveCommentList {
            comments: None,
            next_page_token: None,
        }),
    ));
}

#[given("Google Drive answers that the second file has no comments resource")]
fn google_drive_answers_not_found(world: &mut ApiWorld) {
    world
        .google_drive_sync
        .comments
        .push((1, CommentsOfFile::NotFound));
}

#[given(
    expr = "Google Drive reports a single comment on the first file, whose only reply is {mention} and {newness} than my notification"
)]
fn google_drive_reports_a_reply(world: &mut ApiWorld, mention: Mention, newness: Newness) {
    let list = google_drive_comments_list();
    let comment = list.comments.as_ref().unwrap()[0].clone();
    let reply = comment.replies.as_ref().unwrap()[0].clone();
    let content = match mention {
        Mention::Mentioning => "Hello @jane.doe@example.com".to_string(),
        Mention::NotMentioning => "Hello".to_string(),
    };
    let now = Utc::now();
    let modified_time = match newness {
        Newness::Newer => now.with_year(now.year() + 1).unwrap(),
        Newness::Older => now.with_year(now.year() - 1).unwrap(),
    };
    world.google_drive_sync.comments.push((
        0,
        CommentsOfFile::Listed(GoogleDriveCommentList {
            comments: Some(vec![RawGoogleDriveComment {
                replies: Some(vec![RawGoogleDriveCommentReply {
                    content,
                    modified_time,
                    ..reply
                }]),
                ..comment
            }]),
            next_page_token: None,
        }),
    ));
}

#[given(expr = "I already have the Google Drive notification {string} of the comment 456")]
async fn i_already_have_the_notification_456(world: &mut ApiWorld, name: String) {
    create_known_notification(world, name, google_drive_comment_456()).await;
}

#[given(expr = "I already have the Google Drive notification {string} of the comment 123")]
async fn i_already_have_the_notification_123(world: &mut ApiWorld, name: String) {
    create_known_notification(world, name, google_drive_comment_123()).await;
}

async fn create_known_notification(
    world: &mut ApiWorld,
    name: String,
    comment: GoogleDriveComment,
) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .google_drive_connection
        .as_ref()
        .unwrap()
        .id;
    let app = world.app.get().await;
    let notification =
        create_notification_from_google_drive_comment(app, &comment, user_id, connection_id).await;
    world.notifications.by_name.insert(name, *notification);
}

async fn mount_google_drive(world: &mut ApiWorld) {
    let sync = &world.google_drive_sync;
    let about = sync.about.clone().map(|(name, email)| about(&name, &email));
    let files = sync.files.clone();
    let comments: Vec<(usize, CommentsOfFile)> = sync
        .comments
        .iter()
        .map(|(index, outcome)| {
            (
                *index,
                match outcome {
                    CommentsOfFile::Listed(list) => CommentsOfFile::Listed(list.clone()),
                    CommentsOfFile::NotFound => CommentsOfFile::NotFound,
                },
            )
        })
        .collect();
    let created_at = world
        .integrations
        .google_drive_connection
        .as_ref()
        .unwrap()
        .created_at;
    let app = world.app.get().await;
    let server = &app.google_drive_mock_server;
    if let Some(about) = about {
        mock_google_drive_get_user_info_service(server, &about).await;
    }
    let Some(files) = files else {
        return;
    };
    mock_google_drive_files_list_service(server, None, page_size(), created_at, &files).await;
    if files.next_page_token.is_some() {
        mock_google_drive_files_list_service(
            server,
            Some("next_token"),
            page_size(),
            created_at,
            &GoogleDriveFileList {
                files: None,
                incomplete_search: None,
                next_page_token: None,
            },
        )
        .await;
    }
    let file_ids: Vec<String> = files
        .files
        .iter()
        .flatten()
        .map(|file| file.id.clone())
        .collect();
    for (index, outcome) in comments {
        let file_id = &file_ids[index];
        match outcome {
            CommentsOfFile::Listed(list) => {
                mock_google_drive_comments_list_service(server, None, page_size(), file_id, &list)
                    .await;
                if list.next_page_token.is_some() {
                    mock_google_drive_comments_list_service(
                        server,
                        Some("next_token"),
                        page_size(),
                        file_id,
                        &GoogleDriveCommentList {
                            comments: None,
                            next_page_token: None,
                        },
                    )
                    .await;
                }
            }
            CommentsOfFile::NotFound => {
                mock_google_drive_comments_list_404_service(server, file_id).await;
            }
        }
    }
}

#[when("I sync my Google Drive notifications")]
async fn i_sync_my_google_drive_notifications(world: &mut ApiWorld) {
    mount_google_drive(world).await;
    let client = world.client();
    let app = world.app.get().await;
    let synced = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::GoogleDrive),
        false,
    )
    .await;
    world.google_drive_sync.synced = synced;
}

fn reported_comment_456(world: &ApiWorld) -> GoogleDriveComment {
    let mut comment = google_drive_comment_456();
    if let Some(modified_time) = world.google_drive_sync.modified_time {
        comment.modified_time = modified_time;
    }
    comment
}

#[then("my notifications are the comments Google Drive reported, for the user Jane Doe")]
fn my_notifications_are_the_reported_comments(world: &mut ApiWorld) {
    let user_id = world.user().id;
    assert_eq!(world.google_drive_sync.synced.len(), 2);
    assert_sync_notifications(
        &world.google_drive_sync.synced,
        &google_drive_comment_123(),
        &reported_comment_456(world),
        user_id,
        "jane.doe@example.com",
        "Jane Doe",
    );
}

#[then(
    expr = "{string} is deleted, as Jane Doe already answered, and now holds the comment with her name"
)]
async fn the_known_notification_is_deleted(world: &mut ApiWorld, name: String) {
    let original = world.notifications.named(&name).clone();
    let comment_456 = reported_comment_456(world);
    let notification = read_notification(world, &name).await;
    assert_eq!(notification.id, original.id);
    assert_eq!(notification.kind, NotificationSourceKind::GoogleDrive);
    assert_eq!(
        notification.source_item.source_id,
        original.source_item.source_id
    );
    assert_eq!(notification.status, NotificationStatus::Deleted);
    assert_eq!(notification.last_read_at, None);
    assert_eq!(
        notification.source_item.data,
        ThirdPartyItemData::GoogleDriveComment(Box::new(GoogleDriveComment {
            user_email_address: Some(Pii::new("jane.doe@example.com".to_string())),
            user_display_name: Some(Pii::new("Jane Doe".to_string())),
            ..comment_456
        }))
    );
}

#[then(expr = "my Google Drive connection knows me as {string}, with the address {string}")]
async fn my_connection_knows_me(world: &mut ApiWorld, name: String, email: String) {
    let id = world
        .integrations
        .google_drive_connection
        .as_ref()
        .unwrap()
        .id;
    let connection = stored_connection(world, id).await;
    assert_eq!(
        connection.provider,
        IntegrationProvider::GoogleDrive {
            context: Some(GoogleDriveContext {
                user_email_address: Pii::<EmailAddress>::from_str(&email).unwrap(),
                user_display_name: Pii::new(name),
            }),
            config: GoogleDriveConfig::enabled(),
        }
    );
}

#[then(expr = "the sync returned {int} notification(s)")]
fn the_sync_returned_notifications(world: &mut ApiWorld, count: usize) {
    assert_eq!(world.google_drive_sync.synced.len(), count);
}

#[then("the sync returned one unread Google Drive notification")]
fn the_sync_returned_one_unread_notification(world: &mut ApiWorld) {
    let synced = &world.google_drive_sync.synced;
    assert_eq!(synced.len(), 1);
    assert_eq!(synced[0].kind, NotificationSourceKind::GoogleDrive);
    assert_eq!(synced[0].status, NotificationStatus::Unread);
}

#[then(expr = "my Google Drive notification {string} is {status}")]
async fn my_notification_is(world: &mut ApiWorld, name: String, status: UserStatus) {
    let id = world.notifications.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    assert_eq!(notification.status, status.0);
}

#[then(expr = "the number of user info requests Google Drive received is {int}")]
async fn google_drive_received_user_info_requests(world: &mut ApiWorld, expected: usize) {
    let app = world.app.get().await;
    let calls = app
        .google_drive_mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == "/about")
        .count();
    assert_eq!(calls, expected);
}

#[given(expr = "I unsubscribe from {string}")]
async fn i_unsubscribe_from(world: &mut ApiWorld, name: String) {
    update(
        world,
        &name,
        NotificationPatch {
            status: Some(NotificationStatus::Unsubscribed),
            ..Default::default()
        },
    )
    .await;
}
