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
    steps::notifications::parse_time,
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
    match discussion {
        Some(Discussion::Failing) => mount_failing_discussion(app).await,
        Some(Discussion::Paged) => mount_discussion(app, 2).await,
        Some(Discussion::LoopingBackToTheSamePage) => mount_discussion(app, 1).await,
        None => {}
    }
}

async fn mount_failing_discussion(app: &TestedApp) {
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
}

async fn mount_discussion(app: &TestedApp, second_comments_page: usize) {
    mock_github_discussion_query(
        &app.github_mock_server,
        OWNER.to_string(),
        REPOSITORY.to_string(),
        123,
        &github_discussion_123_response(),
    )
    .await;
    for (after, page) in [
        (None, 1),
        (Some("comments-cursor-1".to_string()), second_comments_page),
    ] {
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

fn reports_on(world: &mut ApiWorld, path: &str, kind: &str, title: Option<&str>) {
    let url = format!("https://api.github.com/repos/{OWNER}/{REPOSITORY}/{path}");
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
    reports_on(world, "pulls/123", "PullRequest", None);
}

#[given(expr = "GitHub reports one notification, on the discussion 123 titled {string}")]
fn github_reports_one_discussion_notification(world: &mut ApiWorld, title: String) {
    reports_on(world, "discussions/123", "Discussion", Some(&title));
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

fn table_rows(step: &Step, header: &[&str]) -> Vec<Vec<String>> {
    let rows = &step.table.as_ref().expect("A table is expected").rows;
    assert_eq!(rows[0], header, "The table has unexpected columns");
    rows[1..].to_vec()
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
    assert_eq!(comments, table_rows(step, &["id", "body"]));
}

#[then(
    expr = "that pull request holds one review, {string}, saying {string} and submitted at {string}"
)]
async fn the_pull_request_holds_one_review(
    world: &mut ApiWorld,
    id: String,
    body: String,
    submitted_at: String,
) {
    let pull_request = the_pull_request(world).await;
    assert_eq!(pull_request.reviews.len(), 1);
    assert_eq!(pull_request.reviews[0].id.as_deref(), Some(id.as_str()));
    assert_eq!(pull_request.reviews[0].body, body);
    assert_eq!(
        pull_request.reviews[0].submitted_at,
        Some(parse_time(&submitted_at))
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
    assert_eq!(threads, table_rows(step, &["thread", "comments"]));
}

#[then(
    expr = "its review thread {int} is on {string}, lines {int} to {int}, starts with the hunk {string} and is not resolved"
)]
async fn an_open_review_thread(
    world: &mut ApiWorld,
    position: usize,
    path: String,
    start_line: i64,
    line: i64,
    hunk: String,
) {
    let pull_request = the_pull_request(world).await;
    let thread = &pull_request.review_threads[position - 1];
    assert_eq!(thread.path, path);
    assert_eq!(
        (thread.start_line, thread.line),
        (Some(start_line), Some(line))
    );
    assert!(thread.diff_hunk.starts_with(&hunk));
    assert!(!thread.is_resolved);
}

#[then(
    expr = "its review thread {int} is resolved and outdated, and falls back to its original line {int}"
)]
async fn a_resolved_outdated_review_thread(world: &mut ApiWorld, position: usize, line: i64) {
    let pull_request = the_pull_request(world).await;
    let thread = &pull_request.review_threads[position - 1];
    assert!(thread.is_resolved && thread.is_outdated);
    assert_eq!(thread.line, Some(line));
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

#[then(expr = "its comment {int} is the answer and holds no reply")]
async fn a_discussion_comment_is_the_answer(world: &mut ApiWorld, position: usize) {
    let discussion = the_discussion(world).await;
    assert!(discussion.comments[position - 1].is_answer);
    assert!(discussion.comments[position - 1].replies.is_empty());
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

#[then(expr = "my only unread notification is {string}")]
async fn my_only_unread_notification_is(world: &mut ApiWorld, name: String) {
    let expected = world.notifications.named(&name).id;
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
    assert_eq!(
        notifications.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![expected]
    );
}
