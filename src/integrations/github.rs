use chrono::{DateTime, Utc};
use git_url_parse::GitUrl;
use http::Uri;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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

    #[test]
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