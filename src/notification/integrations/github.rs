use std::fmt;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use git_url_parse::GitUrl;
use http::{uri::Authority, Uri};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use uuid::Uuid;

use crate::{
    notification::{Notification, NotificationMetadata, NotificationStatus},
    user::UserId,
};

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubNotificationSubject {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub url: Option<Uri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub latest_comment_url: Option<Uri>,
    pub r#type: String,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubCodeOfConduct {
    pub key: String,
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub html_url: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubLicense {
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub url: Option<Uri>,
    pub spdx_id: Option<String>,
    pub node_id: String,
    #[serde_as(as = "DisplayFromStr")]
    pub html_url: Uri,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubPermissions {
    pub admin: bool,
    pub maintain: bool,
    pub push: bool,
    pub triage: bool,
    pub pull: bool,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubUser {
    pub name: Option<String>,
    pub email: Option<String>,
    pub login: String,
    pub id: u64,
    pub node_id: String,
    #[serde_as(as = "DisplayFromStr")]
    pub avatar_url: Uri,
    pub gravatar_id: Option<String>,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub html_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub followers_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub following_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub gists_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub starred_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub subscriptions_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub organizations_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub repos_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub events_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub received_events_url: Uri,
    pub r#type: String,
    pub site_admin: bool,
    pub starred_at: Option<DateTime<Utc>>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubRepository {
    pub id: u64,
    pub node_id: String,
    pub name: String,
    pub full_name: String,
    pub owner: GithubUser,
    pub private: bool,
    #[serde_as(as = "DisplayFromStr")]
    pub html_url: Uri,
    pub description: Option<String>,
    pub fork: bool,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub archive_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub assignees_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub blobs_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub branches_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub collaborators_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub comments_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub commits_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub compare_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub contents_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub contributors_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub deployments_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub downloads_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub events_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub forks_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub git_commits_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub git_refs_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub git_tags_url: Uri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub git_url: Option<GitUrl>,
    #[serde_as(as = "DisplayFromStr")]
    pub issue_comment_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub issue_events_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub issues_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub keys_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub labels_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub languages_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub merges_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub milestones_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub notifications_url: Uri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub pulls_urls: Option<Uri>,
    #[serde_as(as = "DisplayFromStr")]
    pub releases_url: Uri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub ssh_url: Option<GitUrl>,
    #[serde_as(as = "DisplayFromStr")]
    pub stargazers_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub statuses_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub subscribers_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub subscription_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub tags_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub teams_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub trees_url: Uri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub clone_url: Option<Uri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub mirror_url: Option<Uri>,
    #[serde_as(as = "DisplayFromStr")]
    pub hooks_url: Uri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub svn_url: Option<Uri>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub homepage: Option<Uri>,
    pub language: Option<String>,
    pub forks_count: Option<u32>,
    pub stargazers_count: Option<u32>,
    pub watchers_count: Option<u32>,
    pub size: Option<u32>,
    pub default_branch: Option<String>,
    pub open_issues_count: Option<u32>,
    pub is_template: Option<bool>,
    pub topics: Option<Vec<String>>,
    pub has_issues: Option<bool>,
    pub has_projects: Option<bool>,
    pub has_wiki: Option<bool>,
    pub has_pages: Option<bool>,
    pub has_downloads: Option<bool>,
    pub archived: Option<bool>,
    pub disabled: Option<bool>,
    pub visibility: Option<String>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub permissions: Option<GithubPermissions>,
    pub role_name: Option<String>,
    pub template_repository: Option<GithubRepositoryTemplate>,
    pub temp_clone_token: Option<String>,
    pub delete_branch_on_merge: Option<bool>,
    pub subscribers_count: Option<u32>,
    pub network_count: Option<u32>,
    pub code_of_conduct: Option<GithubCodeOfConduct>,
    pub license: Option<GithubLicense>,
    pub forks: Option<u32>,
    pub open_issues: Option<u32>,
    pub watchers: Option<u32>,
    pub allow_forking: Option<bool>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubRepositoryTemplate {
    pub id: u32,
    pub node_id: String,
    pub name: String,
    pub full_name: String,
    pub license: Option<GithubLicense>,
    pub organization: Option<GithubUser>,
    pub forks: u32,
    pub permissions: GithubPermissions,
    pub owner: GithubUser,
    pub private: bool,
    #[serde_as(as = "DisplayFromStr")]
    pub html_url: Uri,
    pub description: Option<String>,
    pub fork: bool,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub archive_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub assignees_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub blobs_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub branches_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub collaborators_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub comments_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub commits_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub compare_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub contents_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub contributors_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub deployments_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub downloads_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub events_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub forks_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub git_commits_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub git_refs_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub git_tags_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub git_url: GitUrl,
    #[serde_as(as = "DisplayFromStr")]
    pub issue_comment_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub issue_events_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub issues_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub keys_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub labels_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub languages_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub merges_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub milestones_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub notifications_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub pulls_urls: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub releases_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub ssh_url: GitUrl,
    #[serde_as(as = "DisplayFromStr")]
    pub stargazers_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub statuses_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub subscribers_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub subscription_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub tags_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub teams_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub trees_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub clone_url: Uri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub mirror_url: Option<Uri>,
    #[serde_as(as = "DisplayFromStr")]
    pub hooks_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub svn_url: Uri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub homepage: Option<Uri>,
    pub language: Option<String>,
    pub forks_count: u32,
    pub stargazers_count: u32,
    pub watchers_count: u32,
    pub size: u32,
    pub default_branch: String,
    pub open_issues_count: u32,
    pub is_template: bool,
    pub topics: Vec<String>,
    pub has_issues: bool,
    pub has_projects: bool,
    pub has_wiki: bool,
    pub has_pages: bool,
    pub has_downloads: bool,
    pub archived: bool,
    pub disabled: bool,
    pub visibility: String,
    pub pushed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub allow_rebase_merge: bool,
    pub template_repository: Option<Box<GithubRepositoryTemplate>>,
    pub temp_clone_token: String,
    pub allow_squash_merge: bool,
    pub allow_auto_merge: bool,
    pub delete_branch_on_merge: bool,
    pub allow_update_branch: bool,
    pub use_squash_pr_title_as_default: bool,
    pub allow_merge_commit: bool,
    pub allow_forking: bool,
    pub subscribers_count: u32,
    pub network_count: u32,
    pub open_issues: u32,
    pub watchers: u32,
    pub master_branch: String,
    pub starred_at: DateTime<Utc>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubNotification {
    pub id: String,
    pub repository: GithubRepository,
    pub subject: GithubNotificationSubject,
    pub reason: String,
    pub unread: bool,
    pub updated_at: DateTime<Utc>,
    pub last_read_at: Option<DateTime<Utc>>,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub subscription_url: Uri,
}

impl GithubNotification {
    pub fn extract_id(&self) -> Option<String> {
        let url = self.subject.url.clone()?;
        let mut url_parts = url.path().split('/').collect::<Vec<_>>();
        let id = url_parts.pop()?;
        Some(id.to_string())
    }

    pub fn get_html_url_from_api_url(api_url: &Option<Uri>) -> Option<Uri> {
        api_url.as_ref().and_then(|uri| {
            if uri.host() == Some("api.github.com") && uri.path().starts_with("/repos") {
                let mut uri_parts = uri.clone().into_parts();
                uri_parts.authority = Some(Authority::from_static("github.com"));
                uri_parts.path_and_query = uri_parts
                    .path_and_query
                    .and_then(|pq| pq.as_str().trim_start_matches("/repos").parse().ok());
                return Uri::from_parts(uri_parts).ok();
            }
            None
        })
    }

    pub fn get_html_url_from_metadata(&self) -> Uri {
        match self.subject.r#type.as_str() {
            // There is no enough information in the notification to link to the source
            "CheckSuite" => {
                let mut uri_parts = self.repository.html_url.clone().into_parts();
                uri_parts.path_and_query = uri_parts
                    .path_and_query
                    .and_then(|pq| format!("{}/actions", pq.as_str()).parse().ok());
                Uri::from_parts(uri_parts).unwrap()
            }
            "Discussion" => {
                let mut uri_parts = self.repository.html_url.clone().into_parts();
                uri_parts.path_and_query = uri_parts.path_and_query.and_then(|pq| {
                    format!(
                        "{}/discussions?{}",
                        pq.as_str(),
                        serde_urlencoded::to_string([(
                            "discussions_q",
                            self.subject.title.clone()
                        )])
                        .unwrap_or_default()
                    )
                    .parse()
                    .ok()
                });
                Uri::from_parts(uri_parts).unwrap()
            }
            _ => self.repository.html_url.clone(),
        }
    }

    pub fn into_notification(self, user_id: UserId) -> Notification {
        let source_html_url = GithubNotification::get_html_url_from_api_url(&self.subject.url);

        Notification {
            id: Uuid::new_v4().into(),
            title: self.subject.title.clone(),
            source_id: self.id.clone(),
            source_html_url,
            status: if self.unread {
                NotificationStatus::Unread
            } else {
                NotificationStatus::Read
            },
            metadata: NotificationMetadata::Github(self.clone()),
            updated_at: self.updated_at,
            last_read_at: self.last_read_at,
            snoozed_until: None,
            user_id,
            details: None,
            task_id: None,
        }
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubPullRequest {
    pub id: String,
    pub number: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    pub title: String,
    pub body: String,
    pub state: GithubPullRequestState,
    pub is_draft: bool,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub merged_at: Option<DateTime<Utc>>,
    pub mergeable_state: GithubMergeableState,
    pub merge_state_status: GithubMergeStateStatus,
    pub merged_by: Option<GithubActor>,
    pub deletions: i64,
    pub additions: i64,
    pub changed_files: i64,
    pub labels: Vec<GithubLabel>,
    pub comments_count: i64,
    pub latest_commit: GithubCommitChecks,
    pub base_ref_name: String,
    pub base_repository: Option<GithubRepositorySummary>,
    pub head_ref_name: String,
    pub head_repository: Option<GithubRepositorySummary>,
    pub author: Option<GithubActor>,
    pub assignees: Vec<GithubActor>,
    pub review_decision: Option<GithubPullRequestReviewDecision>,
    pub reviews: Vec<GithubPullRequestReview>,
    pub review_requests: Vec<GithubReviewer>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubRepositorySummary {
    pub name_with_owner: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum GithubPullRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum GithubMergeableState {
    Conflicting,
    Mergeable,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum GithubMergeStateStatus {
    Behind,
    Blocked,
    Clean,
    Dirty,
    Draft,
    HasHook,
    Unknown,
    Unstable,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum GithubPullRequestReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "type", content = "content")]
pub enum GithubActor {
    User(GithubUserSummary),
    Bot(GithubBotSummary),
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubLabel {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubUserSummary {
    pub login: String,
    #[serde_as(as = "DisplayFromStr")]
    pub avatar_url: Uri,
    pub name: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubBotSummary {
    pub login: String,
    #[serde_as(as = "DisplayFromStr")]
    pub avatar_url: Uri,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubTeamSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub avatar_url: Option<Uri>,
    pub name: String,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubMannequinSummary {
    #[serde_as(as = "DisplayFromStr")]
    pub avatar_url: Uri,
    pub login: String,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubCommitChecks {
    pub git_commit_id: GitObjectId,
    pub check_suites: Option<Vec<GithubCheckSuite>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash)]
#[serde(transparent)]
pub struct GitObjectId(String);

impl fmt::Display for GitObjectId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for GitObjectId {
    fn from(string: String) -> Self {
        Self(string)
    }
}

impl From<GitObjectId> for String {
    fn from(git_object_id: GitObjectId) -> Self {
        git_object_id.0
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct GithubCheckSuite {
    pub check_runs: Vec<GithubCheckRun>,
    pub conclusion: Option<GithubCheckConclusionState>,
    pub status: GithubCheckStatusState,
    pub workflow: Option<GithubWorkflow>,
    pub app: Option<GithubCheckSuiteApp>,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct GithubCheckRun {
    pub name: String,
    pub conclusion: Option<GithubCheckConclusionState>,
    pub status: GithubCheckStatusState,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub url: Option<Uri>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum GithubCheckConclusionState {
    ActionRequired,
    Cancelled,
    Failure,
    Neutral,
    Skipped,
    Stale,
    StartupFailure,
    Success,
    TimedOut,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Default)]
pub enum GithubCheckStatusState {
    Completed,
    InProgress,
    #[default]
    Pending,
    Queued,
    Requested,
    Waiting,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubCheckSuiteApp {
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub logo_url: Uri,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubWorkflow {
    pub name: String,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct GithubPullRequestReview {
    pub author: Option<GithubActor>,
    pub body: String,
    pub state: GithubPullRequestReviewState,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq, Default)]
pub enum GithubPullRequestReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    #[default]
    Pending,
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "type", content = "content")]
pub enum GithubReviewer {
    User(GithubUserSummary),
    Bot(GithubBotSummary),
    Team(GithubTeamSummary),
    Mannequin(GithubMannequinSummary),
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubDiscussion {
    pub id: String,
    pub number: i64,
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    pub title: String,
    pub body: String,
    pub repository: GithubRepositorySummary,
    pub state_reason: Option<GithubDiscussionStateReason>,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub labels: Vec<GithubLabel>,
    pub comments_count: i64,
    pub author: Option<GithubActor>,
    pub answer_chosen_at: Option<DateTime<Utc>>,
    pub answer_chosen_by: Option<GithubActor>,
    pub answer: Option<GithubDiscussionComment>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Copy, Clone, Eq)]
pub enum GithubDiscussionStateReason {
    Duplicate,
    Outdated,
    Reopened,
    Resolved,
}

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct GithubDiscussionComment {
    #[serde_as(as = "DisplayFromStr")]
    pub url: Uri,
    pub body: String,
    pub author: Option<GithubActor>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq)]
pub enum GithubUri {
    PullRequest {
        owner: String,
        repository: String,
        number: i64,
    },
}

impl GithubUri {
    pub fn try_from_api_uri(resource_uri: &Uri) -> Result<Self> {
        if resource_uri.host() != Some("api.github.com") {
            return Err(
                anyhow!(
                    "Failed to parse Github API resource URI: it must be hosted on api.github.com, found: {:?}", resource_uri.host()
                )
            );
        }

        if let &["", "repos", owner, repository, "pulls", number] = resource_uri
            .path()
            .split('/')
            .collect::<Vec<&str>>()
            .as_slice()
        {
            return Ok(GithubUri::PullRequest {
                owner: owner.to_string(),
                repository: repository.to_string(),
                number: number.parse()
                    .with_context(|| {
                        format!(
                            "Failed to parse Github API resource URI: Pull request number must be an integer: {}", number)
                    })?
            });
        }

        Err(anyhow!(
            "Failed to parse Github API resource URI: unknown resource type: {}",
            resource_uri
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    mod url_serialization {
        use super::*;

        #[rstest]
        fn test_uri_serialization_config() {
            assert_eq!(
                r#"{"key":"key1","name":"name1","url":"https://api.github.com/1","body":"body1"}"#,
                serde_json::to_string(&GithubCodeOfConduct {
                    key: "key1".to_string(),
                    name: "name1".to_string(),
                    url: "https://api.github.com/1".try_into().unwrap(),
                    body: "body1".to_string(),
                    html_url: None,
                })
                .unwrap()
            );

            assert_eq!(
                r#"{"key":"key1","name":"name1","url":"https://api.github.com/1","body":"body1","html_url":"https://api.github.com/1.html"}"#,
                serde_json::to_string(&GithubCodeOfConduct {
                    key: "key1".to_string(),
                    name: "name1".to_string(),
                    url: "https://api.github.com/1".try_into().unwrap(),
                    body: "body1".to_string(),
                    html_url: Some("https://api.github.com/1.html".try_into().unwrap()),
                })
                .unwrap()
            );
        }

        #[rstest]
        fn test_uri_deserialization_config() {
            assert_eq!(
            GithubCodeOfConduct {
                key: "key1".to_string(),
                name: "name1".to_string(),
                url: "https://api.github.com/1".try_into().unwrap(),
                body: "body1".to_string(),
                html_url: None,
            },
            serde_json::from_str(
                r#"{"key":"key1","name":"name1","url":"https://api.github.com/1","body":"body1"}"#,
            )
            .unwrap()
        );

            assert_eq!(
            GithubCodeOfConduct {
                key: "key1".to_string(),
                name: "name1".to_string(),
                url: "https://api.github.com/1".try_into().unwrap(),
                body: "body1".to_string(),
                html_url: Some("https://api.github.com/1.html".try_into().unwrap()),
            },
            serde_json::from_str(r#"{"key":"key1","name":"name1","url":"https://api.github.com/1","body":"body1","html_url":"https://api.github.com/1.html"}"#)
            .unwrap()
            );
        }
    }

    mod get_html_url_from_api_url {
        use super::*;

        #[rstest]
        #[case(
            "https://api.github.com/repos/octokit/octokit.rb/issues/123",
            "https://github.com/octokit/octokit.rb/issues/123"
        )]
        #[case(
            "https://api.github.com/repos/octokit/octokit.rb/pulls/123",
            "https://github.com/octokit/octokit.rb/pulls/123"
        )]
        fn test_get_html_url_from_api_url_with_valid_api_url(
            #[case] api_url: &str,
            #[case] expected_html_url: &str,
        ) {
            assert_eq!(
                GithubNotification::get_html_url_from_api_url(&Some(
                    api_url.parse::<Uri>().unwrap()
                )),
                Some(expected_html_url.parse::<Uri>().unwrap())
            );
        }

        #[rstest]
        fn test_get_html_url_from_api_url_with_invalid_github_api_url(
            #[values(
                None,
                Some("https://api.github.com/octokit/octokit.rb/issues/123"),
                Some("https://github.com/repos/octokit/octokit.rb/issues/123"),
                Some("https://github.com/octokit/octokit.rb/issues/123"),
                Some("https://google.com")
            )]
            api_url: Option<&str>,
        ) {
            assert_eq!(
                GithubNotification::get_html_url_from_api_url(
                    &api_url.map(|url| url.parse::<Uri>().unwrap())
                ),
                None
            );
        }
    }

    mod try_from_api_uri {
        use super::*;

        #[rstest]
        #[case::pull_request(
            "https://api.github.com/repos/octokit/octokit.rb/pulls/123",
            GithubUri::PullRequest {
                owner: "octokit".to_string(),
                repository: "octokit.rb".to_string(),
                number: 123
            }
        )]
        fn test_try_from_api_uri(
            #[case] resource_uri: &str,
            #[case] expected_github_uri: GithubUri,
        ) {
            assert_eq!(
                GithubUri::try_from_api_uri(&resource_uri.parse::<Uri>().unwrap()).unwrap(),
                expected_github_uri
            );
        }

        #[rstest]
        fn test_try_from_api_uri_from_non_api_domain() {
            assert!(GithubUri::try_from_api_uri(
                &"https://github.com/octokit/octokit.rb/pulls/123"
                    .parse::<Uri>()
                    .unwrap()
            )
            .is_err());
        }

        #[rstest]
        fn test_try_from_api_uri_from_unknown_resource() {
            assert!(GithubUri::try_from_api_uri(
                &"https://api.github.com/unknown/123".parse::<Uri>().unwrap()
            )
            .is_err());
        }

        #[rstest]
        fn test_try_from_api_uri_from_invalid_pull_request_number() {
            assert!(GithubUri::try_from_api_uri(
                &"https://api.github.com/repos/octokit/octokit.rb/abc"
                    .parse::<Uri>()
                    .unwrap()
            )
            .is_err());
        }
    }
}
