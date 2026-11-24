use chrono::{TimeZone, Utc};
use cucumber::{gherkin::Step, given, then, when};
use graphql_client::{Error, Response};
use pretty_assertions::assert_eq;

use universal_inbox::{
    integration_connection::IntegrationConnectionStatus,
    notification::{NotificationSourceKind, NotificationStatus},
    third_party::{
        integrations::github::{
            GithubDiscussion, GithubNotificationItem, GithubNotificationSubject, GithubPullRequest,
        },
        item::ThirdPartyItemData,
    },
};

use crate::{
    helpers::{
        TestedApp,
        notification::{
            github::{
                github_discussion_123_comments_response, github_discussion_123_response,
                github_discussion_comment_1_replies_page_2_response,
                github_pull_request_123_comments_page_2_response,
                github_pull_request_123_review_threads_page_2_response,
                github_pull_request_review_thread_1_comments_page_2_response,
                mock_github_discussion_comment_replies_query,
                mock_github_discussion_comments_query, mock_github_discussion_query,
                mock_github_pull_request_comments_query,
                mock_github_pull_request_review_thread_comments_query,
                mock_github_pull_request_review_threads_query,
            },
            list_notifications, sync_notifications_response,
        },
    },
    state::{Discussion, PullRequest},
    world::{ApiWorld, Response as HttpResponse},
};

const OWNER: &str = "octokit";
const REPOSITORY: &str = "octokit.rb";

pub async fn mount_details(
    app: &TestedApp,
    pull_request: Option<PullRequest>,
    discussion: Option<Discussion>,
) {
    if let Some(PullRequest::WithCommentsAndReviewThreads) = pull_request {
        mock_github_pull_request_comments_query(
            &app.github_mock_server,
            OWNER.to_string(),
            REPOSITORY.to_string(),
            123,
            Some("comments-cursor-1".to_string()),
            &github_pull_request_123_comments_page_2_response(),
        )
        .await;
        mock_github_pull_request_review_threads_query(
            &app.github_mock_server,
            OWNER.to_string(),
            REPOSITORY.to_string(),
            123,
            Some("threads-cursor-1".to_string()),
            &github_pull_request_123_review_threads_page_2_response(),
        )
        .await;
        mock_github_pull_request_review_thread_comments_query(
            &app.github_mock_server,
            "PRRT_1".to_string(),
            Some("thread-comments-cursor-1".to_string()),
            &github_pull_request_review_thread_1_comments_page_2_response(),
        )
        .await;
    }
    let Some(discussion) = discussion else {
        return;
    };
    match discussion {
        Discussion::Failing => {
            let error_response = Response {
                data: None,
                errors: Some(vec![Error {
                    message: "Something went wrong".to_string(),
                    locations: None,
                    path: None,
                    extensions: None,
                }]),
                extensions: None,
            };
            mock_github_discussion_query(
                &app.github_mock_server,
                OWNER.to_string(),
                REPOSITORY.to_string(),
                123,
                &error_response,
            )
            .await;
            return;
        }
        Discussion::Paged | Discussion::LoopingBackToTheSamePage => {
            mock_github_discussion_query(
                &app.github_mock_server,
                OWNER.to_string(),
                REPOSITORY.to_string(),
                123,
                &github_discussion_123_response(),
            )
            .await;
        }
    }
    let comment_pages = match discussion {
        Discussion::Paged => [(None, 1), (Some("comments-cursor-1".to_string()), 2)],
        _ => [(None, 1), (Some("comments-cursor-1".to_string()), 1)],
    };
    for (after, page) in comment_pages {
        mock_github_discussion_comments_query(
            &app.github_mock_server,
            OWNER.to_string(),
            REPOSITORY.to_string(),
            123,
            after,
            &github_discussion_123_comments_response(page),
        )
        .await;
    }
    mock_github_discussion_comment_replies_query(
        &app.github_mock_server,
        "DC_1".to_string(),
        Some("replies-cursor-1".to_string()),
        &github_discussion_comment_1_replies_page_2_response(),
    )
    .await;
}

fn reports_on(world: &mut ApiWorld, url: &str, kind: &str, title: Option<&str>) {
    let mut notification = crate::helpers::notification::github::github_notification();
    let title = title
        .map(str::to_string)
        .unwrap_or(notification.subject.title.clone());
    notification.subject = GithubNotificationSubject {
        title,
        url: Some(url.parse().unwrap()),
        latest_comment_url: None,
        r#type: kind.to_string(),
    };
    world.github_sync.reported = vec![*notification];
}

#[given("GitHub reports one notification, on pull request 123")]
fn github_reports_one_pull_request_notification(world: &mut ApiWorld) {
    reports_on(
        world,
        "https://api.github.com/repos/octokit/octokit.rb/pulls/123",
        "PullRequest",
        None,
    );
}

#[given(expr = "GitHub reports one notification, on the discussion 123 titled {string}")]
fn github_reports_one_discussion_notification(world: &mut ApiWorld, title: String) {
    reports_on(
        world,
        "https://api.github.com/repos/octokit/octokit.rb/discussions/123",
        "Discussion",
        Some(&title),
    );
}

#[given(
    "GitHub knows pull request 123, with comments, reviews and review threads over several pages"
)]
fn github_knows_pull_request_123_with_details(world: &mut ApiWorld) {
    world.github_sync.pull_request = Some(PullRequest::WithCommentsAndReviewThreads);
}

#[given("GitHub knows the discussion 123, with comments and replies over several pages")]
fn github_knows_the_discussion(world: &mut ApiWorld) {
    world.github_sync.discussion = Some(Discussion::Paged);
}

#[given("GitHub knows the discussion 123, whose second page of comments is the first one again")]
fn github_knows_a_looping_discussion(world: &mut ApiWorld) {
    world.github_sync.discussion = Some(Discussion::LoopingBackToTheSamePage);
}

#[given("GitHub fails to give the discussion 123 with the error \"Something went wrong\"")]
fn github_fails_to_give_the_discussion(world: &mut ApiWorld) {
    world.github_sync.discussion = Some(Discussion::Failing);
}

#[when("I ask for a synchronous sync of my GitHub notifications, which fails")]
async fn i_ask_for_a_sync_that_fails(world: &mut ApiWorld) {
    super::github_sync::mount_github(world).await;
    let client = world.client();
    let app = world.app.get().await;
    let response = sync_notifications_response(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;
    world.http.record(HttpResponse::read(response).await);
}

async fn the_only_unread_notification(world: &mut ApiWorld) -> GithubNotificationItem {
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].kind, NotificationSourceKind::Github);
    let ThirdPartyItemData::GithubNotification(github_notification) =
        notifications[0].source_item.data.clone()
    else {
        panic!("Expected a GitHub notification");
    };
    github_notification
        .item
        .expect("The GitHub notification has no details")
}

async fn the_discussion(world: &mut ApiWorld) -> GithubDiscussion {
    match the_only_unread_notification(world).await {
        GithubNotificationItem::GithubDiscussion(discussion) => discussion,
        other => panic!("Expected a GitHub discussion, got {other:?}"),
    }
}

async fn the_pull_request(world: &mut ApiWorld) -> GithubPullRequest {
    match the_only_unread_notification(world).await {
        GithubNotificationItem::GithubPullRequest(pull_request) => pull_request,
        other => panic!("Expected a GitHub pull request, got {other:?}"),
    }
}

fn table_rows(step: &Step) -> Vec<Vec<String>> {
    step.table
        .as_ref()
        .expect("A table is expected")
        .rows
        .iter()
        .skip(1)
        .cloned()
        .collect()
}

#[then(
    "I have one unread GitHub notification, whose pull request holds these comments, minimized ones dropped:"
)]
async fn the_pull_request_holds_these_comments(world: &mut ApiWorld, step: &Step) {
    let pull_request = the_pull_request(world).await;
    let comments: Vec<Vec<String>> = pull_request
        .comments
        .iter()
        .map(|comment| vec![comment.id.clone().unwrap_or_default(), comment.body.clone()])
        .collect();
    assert_eq!(comments, table_rows(step));
}

#[then(
    "that pull request holds one review, PRR_1, saying \"<p>Looks good overall</p>\" and submitted at 2014-11-07T22:03:00Z"
)]
async fn the_pull_request_holds_one_review(world: &mut ApiWorld) {
    let pull_request = the_pull_request(world).await;
    assert_eq!(pull_request.reviews.len(), 1);
    assert_eq!(pull_request.reviews[0].id.as_deref(), Some("PRR_1"));
    assert_eq!(pull_request.reviews[0].body, "<p>Looks good overall</p>");
    assert_eq!(
        pull_request.reviews[0].submitted_at,
        Some(Utc.with_ymd_and_hms(2014, 11, 7, 22, 3, 0).unwrap())
    );
}

#[then("that pull request holds these review threads, with all their comment pages:")]
async fn the_pull_request_holds_these_threads(world: &mut ApiWorld, step: &Step) {
    let pull_request = the_pull_request(world).await;
    let threads: Vec<Vec<String>> = pull_request
        .review_threads
        .iter()
        .map(|thread| {
            vec![
                thread.id.clone(),
                thread
                    .comments
                    .iter()
                    .map(|comment| comment.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ]
        })
        .collect();
    assert_eq!(threads, table_rows(step));
}

#[then(
    "its first review thread is on lib/octokit/authenticator.rb, lines 13 to 15, starts with the hunk \"@@ -10,6 +10,8 @@\" and is not resolved"
)]
async fn the_first_review_thread(world: &mut ApiWorld) {
    let pull_request = the_pull_request(world).await;
    let thread = &pull_request.review_threads[0];
    assert_eq!(thread.path, "lib/octokit/authenticator.rb");
    assert_eq!((thread.start_line, thread.line), (Some(13), Some(15)));
    assert!(thread.diff_hunk.starts_with("@@ -10,6 +10,8 @@"));
    assert!(!thread.is_resolved);
}

#[then("its second review thread is resolved and outdated, and falls back to its original line 20")]
async fn the_second_review_thread(world: &mut ApiWorld) {
    let pull_request = the_pull_request(world).await;
    let thread = &pull_request.review_threads[1];
    assert!(thread.is_resolved && thread.is_outdated);
    assert_eq!(thread.line, Some(20));
}

#[then(
    expr = "I have one unread GitHub notification, whose discussion is {string} at {string}, in the answerable category {string}, emoji {string}, slug {string}"
)]
async fn the_discussion_and_its_category(
    world: &mut ApiWorld,
    title: String,
    url: String,
    category: String,
    emoji: String,
    slug: String,
) {
    let discussion = the_discussion(world).await;
    assert_eq!(discussion.title, title);
    assert_eq!(discussion.url, url.parse().unwrap());
    let found = discussion
        .category
        .as_ref()
        .expect("Discussion should have a category");
    assert_eq!(found.name, category);
    assert_eq!(found.emoji.as_deref(), Some(emoji.as_str()));
    assert_eq!(found.slug, slug);
    assert!(found.is_answerable);
}

#[then(expr = "that discussion holds the comments {string}")]
async fn the_discussion_holds_the_comments(world: &mut ApiWorld, ids: String) {
    let discussion = the_discussion(world).await;
    let found: Vec<&str> = discussion.comments.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(found.join(", "), ids);
}

#[then(
    expr = "its first comment says {string}, is not the answer, has {int} replies and holds the replies {string}"
)]
async fn the_first_discussion_comment(
    world: &mut ApiWorld,
    body: String,
    replies_count: i64,
    replies: String,
) {
    let discussion = the_discussion(world).await;
    let first = &discussion.comments[0];
    assert_eq!(first.comment.body, body);
    assert!(!first.is_answer);
    assert_eq!(first.replies_count, replies_count);
    let held: Vec<&str> = first.replies.iter().map(|r| r.body.as_str()).collect();
    assert_eq!(held.join(", "), replies);
}

#[then("its second comment is the answer and holds no reply")]
async fn the_second_discussion_comment(world: &mut ApiWorld) {
    let discussion = the_discussion(world).await;
    assert!(discussion.comments[1].is_answer);
    assert!(discussion.comments[1].replies.is_empty());
}

#[then("I have no unread notification")]
async fn i_have_no_unread_notification(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
    assert_eq!(notifications.len(), 0);
}

#[then("my GitHub connection recorded its first failed sync, and is still validated")]
async fn my_connection_recorded_its_first_failed_sync(world: &mut ApiWorld) {
    let connection = super::github_sync::github_connection_of(world).await;
    assert!(connection.last_notifications_sync_started_at.is_some());
    assert!(connection.last_notifications_sync_completed_at.is_none());
    assert!(connection.last_notifications_sync_failed_at.is_some());
    assert_eq!(
        connection
            .last_notifications_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch notifications from Github"
    );
    assert_eq!(connection.notifications_sync_failures, 1);
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
}
