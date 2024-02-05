use anyhow::Context;
use url::Url;

use universal_inbox::notification::{
    integrations::github::{
        GithubActor, GithubBotSummary, GithubCheckConclusionState, GithubCheckRun,
        GithubCheckStatusState, GithubCheckSuite, GithubCheckSuiteApp, GithubCommitChecks,
        GithubIssueComment, GithubLabel, GithubMannequinSummary, GithubMergeStateStatus,
        GithubMergeableState, GithubPullRequest, GithubPullRequestReview,
        GithubPullRequestReviewDecision, GithubPullRequestReviewState, GithubPullRequestState,
        GithubRepositorySummary, GithubReviewer, GithubTeamSummary, GithubUserSummary,
        GithubWorkflow,
    },
    NotificationDetails,
};

use crate::{
    integrations::github::graphql::pull_request_query, universal_inbox::UniversalInboxError,
};

impl From<pull_request_query::PullRequestState> for GithubPullRequestState {
    fn from(value: pull_request_query::PullRequestState) -> Self {
        match value {
            pull_request_query::PullRequestState::CLOSED => GithubPullRequestState::Closed,
            pull_request_query::PullRequestState::MERGED => GithubPullRequestState::Merged,
            pull_request_query::PullRequestState::OPEN => GithubPullRequestState::Open,
            pull_request_query::PullRequestState::Other(_) => GithubPullRequestState::Open,
        }
    }
}

impl From<pull_request_query::MergeableState> for GithubMergeableState {
    fn from(value: pull_request_query::MergeableState) -> Self {
        match value {
            pull_request_query::MergeableState::CONFLICTING => GithubMergeableState::Conflicting,
            pull_request_query::MergeableState::UNKNOWN => GithubMergeableState::Unknown,
            pull_request_query::MergeableState::MERGEABLE => GithubMergeableState::Mergeable,
            pull_request_query::MergeableState::Other(_) => GithubMergeableState::Unknown,
        }
    }
}

impl From<pull_request_query::MergeStateStatus> for GithubMergeStateStatus {
    fn from(value: pull_request_query::MergeStateStatus) -> Self {
        match value {
            pull_request_query::MergeStateStatus::BEHIND => GithubMergeStateStatus::Behind,
            pull_request_query::MergeStateStatus::BLOCKED => GithubMergeStateStatus::Blocked,
            pull_request_query::MergeStateStatus::CLEAN => GithubMergeStateStatus::Clean,
            pull_request_query::MergeStateStatus::DIRTY => GithubMergeStateStatus::Dirty,
            pull_request_query::MergeStateStatus::DRAFT => GithubMergeStateStatus::Draft,
            pull_request_query::MergeStateStatus::HAS_HOOKS => GithubMergeStateStatus::HasHook,
            pull_request_query::MergeStateStatus::UNSTABLE => GithubMergeStateStatus::Unstable,
            pull_request_query::MergeStateStatus::UNKNOWN => GithubMergeStateStatus::Unknown,
            pull_request_query::MergeStateStatus::Other(_) => GithubMergeStateStatus::Unknown,
        }
    }
}

impl From<pull_request_query::CheckStatusState> for GithubCheckStatusState {
    fn from(value: pull_request_query::CheckStatusState) -> Self {
        match value {
            pull_request_query::CheckStatusState::COMPLETED => GithubCheckStatusState::Completed,
            pull_request_query::CheckStatusState::IN_PROGRESS => GithubCheckStatusState::InProgress,
            pull_request_query::CheckStatusState::PENDING => GithubCheckStatusState::Pending,
            pull_request_query::CheckStatusState::QUEUED => GithubCheckStatusState::Queued,
            pull_request_query::CheckStatusState::REQUESTED => GithubCheckStatusState::Requested,
            pull_request_query::CheckStatusState::WAITING => GithubCheckStatusState::Waiting,
            pull_request_query::CheckStatusState::Other(_) => GithubCheckStatusState::Queued,
        }
    }
}

impl From<pull_request_query::CheckConclusionState> for GithubCheckConclusionState {
    fn from(value: pull_request_query::CheckConclusionState) -> Self {
        match value {
            pull_request_query::CheckConclusionState::ACTION_REQUIRED => {
                GithubCheckConclusionState::ActionRequired
            }
            pull_request_query::CheckConclusionState::CANCELLED => {
                GithubCheckConclusionState::Cancelled
            }
            pull_request_query::CheckConclusionState::FAILURE => {
                GithubCheckConclusionState::Failure
            }
            pull_request_query::CheckConclusionState::NEUTRAL => {
                GithubCheckConclusionState::Neutral
            }
            pull_request_query::CheckConclusionState::SKIPPED => {
                GithubCheckConclusionState::Skipped
            }
            pull_request_query::CheckConclusionState::STALE => GithubCheckConclusionState::Stale,
            pull_request_query::CheckConclusionState::STARTUP_FAILURE => {
                GithubCheckConclusionState::StartupFailure
            }
            pull_request_query::CheckConclusionState::SUCCESS => {
                GithubCheckConclusionState::Success
            }
            pull_request_query::CheckConclusionState::TIMED_OUT => {
                GithubCheckConclusionState::TimedOut
            }
            pull_request_query::CheckConclusionState::Other(_) => {
                GithubCheckConclusionState::Neutral
            }
        }
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestCommitsNodesCommitCheckSuitesNodesCheckRunsNodes>
for GithubCheckRun {
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestCommitsNodesCommitCheckSuitesNodesCheckRunsNodes
    ) -> Result<Self, Self::Error> {
        Ok(GithubCheckRun {
            name: value.name,
            conclusion: value.conclusion.map(|conclusion| conclusion.into()),
            url: value.details_url.map(|details_url| {
                details_url
                    .parse::<Url>()
                    .with_context(|| format!("Github check run details URL could not be parsed: {:?}", details_url))
            })
                .transpose()?,
            status: value.status.into(),
        })
    }
}

impl
    TryFrom<
        pull_request_query::PullRequestQueryRepositoryPullRequestCommitsNodesCommitCheckSuitesNodes,
    > for GithubCheckSuite
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestCommitsNodesCommitCheckSuitesNodes,
    ) -> Result<Self, Self::Error> {
        Ok(GithubCheckSuite {
            check_runs: value
                .check_runs
                .map(|check_runs| {
                    check_runs
                        .nodes
                        .map(|nodes| {
                            nodes
                                .into_iter()
                                .filter_map(|node| node.map(|check_run| check_run.try_into()))
                                .collect::<Result<Vec<GithubCheckRun>, UniversalInboxError>>()
                        })
                        .unwrap_or(Ok(Vec::new()))
                })
                .unwrap_or(Ok(Vec::new()))?,
            conclusion: value.conclusion.map(|conclusion| conclusion.into()),
            status: value.status.into(),
            workflow: value
                .workflow_run
                .map(|workflow_run| {
                    Ok::<GithubWorkflow, UniversalInboxError>(GithubWorkflow {
                        name: workflow_run.workflow.name,
                        url: workflow_run.workflow.url.parse::<Url>().with_context(|| {
                            format!(
                                "Github workflow should have a valid URL: {:?}",
                                workflow_run.workflow.url
                            )
                        })?,
                    })
                })
                .transpose()?,
            app: value
                .app
                .map(|app| {
                    Ok::<GithubCheckSuiteApp, UniversalInboxError>(GithubCheckSuiteApp {
                        name: app.name,
                        url: app.url.parse::<Url>().with_context(|| {
                            format!(
                                "Github check suite application should have a valid URL: {:?}",
                                app.url
                            )
                        })?,
                        logo_url: app.logo_url.parse::<Url>().ok(),
                    })
                })
                .transpose()?,
        })
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestCommitsNodesCommit>
    for GithubCommitChecks
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestCommitsNodesCommit,
    ) -> Result<Self, Self::Error> {
        Ok(GithubCommitChecks {
            git_commit_id: value.oid,
            check_suites: value
                .check_suites
                .map(|check_suites| {
                    check_suites
                        .nodes
                        .map(|nodes| {
                            nodes
                                .into_iter()
                                .filter_map(|node| node.map(|node| node.try_into()))
                                .collect::<Result<Vec<GithubCheckSuite>, UniversalInboxError>>()
                        })
                        .unwrap_or_else(|| Ok(Vec::new()))
                })
                .transpose()?,
        })
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestAuthor> for GithubActor {
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestAuthor,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            pull_request_query::PullRequestQueryRepositoryPullRequestAuthorOn::User(user) => {
                GithubActor::User(GithubUserSummary {
                    login: value.login,
                    name: user.name,
                    avatar_url: value.avatar_url.parse::<Url>().with_context(|| {
                        format!(
                            "Github actor should have a valid avatar URL: {:?}",
                            value.avatar_url
                        )
                    })?,
                })
            }
            // Simplification: any other users are considered as bot. May be revisited in the future
            _ => GithubActor::Bot(GithubBotSummary {
                login: value.login,
                avatar_url: value.avatar_url.parse::<Url>().with_context(|| {
                    format!(
                        "Github actor should have a valid avatar URL: {:?}",
                        value.avatar_url
                    )
                })?,
            }),
        })
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestMergedBy> for GithubActor {
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestMergedBy,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            pull_request_query::PullRequestQueryRepositoryPullRequestMergedByOn::User(user) => {
                GithubActor::User(GithubUserSummary {
                    login: value.login,
                    name: user.name,
                    avatar_url: value.avatar_url.parse::<Url>().with_context(|| {
                        format!(
                            "Github actor should have a valid avatar URL: {:?}",
                            value.avatar_url
                        )
                    })?,
                })
            }
            // Simplification: any other users are considered as bot. May be revisited in the future
            _ => GithubActor::Bot(GithubBotSummary {
                login: value.login,
                avatar_url: value.avatar_url.parse::<Url>().with_context(|| {
                    format!(
                        "Github actor should have a valid avatar URL: {:?}",
                        value.avatar_url
                    )
                })?,
            }),
        })
    }
}

impl From<pull_request_query::PullRequestQueryRepositoryPullRequestLabels> for Vec<GithubLabel> {
    fn from(value: pull_request_query::PullRequestQueryRepositoryPullRequestLabels) -> Self {
        value
            .nodes
            .map(|labels| {
                labels
                    .into_iter()
                    .filter_map(|label| {
                        label.map(|label| GithubLabel {
                            name: label.name,
                            color: label.color,
                            description: label.description,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestCommits>
    for Option<GithubCommitChecks>
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestCommits,
    ) -> Result<Self, Self::Error> {
        value
            .nodes
            .and_then(|nodes| {
                nodes
                    .into_iter()
                    .filter_map(|node| {
                        node.map(|node| TryInto::<GithubCommitChecks>::try_into(node.commit))
                    })
                    .collect::<Vec<Result<GithubCommitChecks, UniversalInboxError>>>()
                    .pop()
            })
            .transpose()
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestAssignees>
    for Vec<GithubActor>
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestAssignees,
    ) -> Result<Self, Self::Error> {
        value
            .nodes
            .map(|nodes| {
                nodes
                    .into_iter()
                    .filter_map(|node| {
                        node.map(|node| {
                            Ok(GithubActor::User(GithubUserSummary {
                                name: node.name,
                                login: node.login,
                                avatar_url: node.avatar_url.parse::<Url>().with_context(|| {
                                    format!(
                                        "Github actor should have a valid avatar URL: {:?}",
                                        node.avatar_url
                                    )
                                })?,
                            }))
                        })
                    })
                    .collect::<Result<Vec<GithubActor>, UniversalInboxError>>()
            })
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

impl From<pull_request_query::PullRequestReviewDecision> for GithubPullRequestReviewDecision {
    fn from(value: pull_request_query::PullRequestReviewDecision) -> Self {
        match value {
            pull_request_query::PullRequestReviewDecision::APPROVED => {
                GithubPullRequestReviewDecision::Approved
            }
            pull_request_query::PullRequestReviewDecision::CHANGES_REQUESTED => {
                GithubPullRequestReviewDecision::ChangesRequested
            }
            pull_request_query::PullRequestReviewDecision::REVIEW_REQUIRED => {
                GithubPullRequestReviewDecision::ReviewRequired
            }
            pull_request_query::PullRequestReviewDecision::Other(_) => {
                GithubPullRequestReviewDecision::ReviewRequired
            }
        }
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestReviewsNodesAuthor>
    for GithubActor
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestReviewsNodesAuthor,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            pull_request_query::PullRequestQueryRepositoryPullRequestReviewsNodesAuthorOn::User(
                user,
            ) => GithubActor::User(GithubUserSummary {
                login: value.login,
                name: user.name,
                avatar_url: value.avatar_url.parse::<Url>().with_context(|| {
                    format!(
                        "Github actor should have a valid avatar URL: {:?}",
                        value.avatar_url
                    )
                })?,
            }),
            // Simplification: any other users are considered as bot. May be revisited in the future
            _ => GithubActor::Bot(GithubBotSummary {
                login: value.login,
                avatar_url: value.avatar_url.parse::<Url>().with_context(|| {
                    format!(
                        "Github actor should have a valid avatar URL: {:?}",
                        value.avatar_url
                    )
                })?,
            }),
        })
    }
}

impl From<pull_request_query::PullRequestReviewState> for GithubPullRequestReviewState {
    fn from(value: pull_request_query::PullRequestReviewState) -> Self {
        match value {
            pull_request_query::PullRequestReviewState::APPROVED => {
                GithubPullRequestReviewState::Approved
            }
            pull_request_query::PullRequestReviewState::CHANGES_REQUESTED => {
                GithubPullRequestReviewState::ChangesRequested
            }
            pull_request_query::PullRequestReviewState::COMMENTED => {
                GithubPullRequestReviewState::Commented
            }
            pull_request_query::PullRequestReviewState::DISMISSED => {
                GithubPullRequestReviewState::Dismissed
            }
            pull_request_query::PullRequestReviewState::PENDING => {
                GithubPullRequestReviewState::Pending
            }
            pull_request_query::PullRequestReviewState::Other(_) => {
                GithubPullRequestReviewState::Pending
            }
        }
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestReviews>
    for Vec<GithubPullRequestReview>
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestReviews,
    ) -> Result<Self, Self::Error> {
        value
            .nodes
            .map(|nodes| {
                nodes
                    .into_iter()
                    .filter_map(|node| {
                        node.map(|node| {
                            Ok(GithubPullRequestReview {
                                author: node.author.map(|author| author.try_into()).transpose()?,
                                body: node.body_html,
                                state: node.state.into(),
                            })
                        })
                    })
                    .collect::<Result<Vec<GithubPullRequestReview>, UniversalInboxError>>()
            })
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnUser>
    for GithubReviewer
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnUser,
    ) -> Result<Self, Self::Error> {
        Ok(
            GithubReviewer::User(GithubUserSummary {
                name: value.user_name,
                login: value.user_login,
                avatar_url: value.user_avatar_url.parse::<Url>()
                    .with_context(|| format!("Github actor should have a valid avatar URL: {:?}", value.user_avatar_url))?,
            })
        )
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnBot>
    for GithubReviewer
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnBot,
    ) -> Result<Self, Self::Error> {
        Ok(
            GithubReviewer::Bot(GithubBotSummary {
                login: value.bot_login,
                avatar_url: value.bot_avatar_url.parse::<Url>()
                    .with_context(|| format!("Github actor should have a valid avatar URL: {:?}", value.bot_avatar_url))?,
            })
        )
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnTeam>
    for GithubReviewer
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnTeam,
    ) -> Result<Self, Self::Error> {
        Ok(
            GithubReviewer::Team(GithubTeamSummary {
                name: value.team_name,
                avatar_url: value.team_avatar_url.map(|url| {
                    url.parse::<Url>()
                        .with_context(|| format!("Github actor should have a valid avatar URL: {url:?}"))
                }).transpose()?,
            })
        )
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnMannequin>
    for GithubReviewer
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewerOnMannequin,
    ) -> Result<Self, Self::Error> {
        Ok(
            GithubReviewer::Mannequin(GithubMannequinSummary {
                login: value.mannequin_login,
                avatar_url: value.mannequin_avatar_url.parse::<Url>()
                    .with_context(|| format!("Github actor should have a valid avatar URL: {:?}", value.mannequin_avatar_url))?,
            })
        )
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequests>
    for Vec<GithubReviewer>
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequests,
    ) -> Result<Self, Self::Error> {
        value
            .nodes
            .map(|nodes| {
                nodes
                    .into_iter()
                    .filter_map(|node| {
                        node.and_then(|node| node.requested_reviewer.map(|reviewer| match reviewer {
                            pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewer::User(user) => user.try_into(),
                            pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewer::Bot(bot) => bot.try_into(),
                            pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewer::Team(team) => team.try_into(),
                            pull_request_query::PullRequestQueryRepositoryPullRequestReviewRequestsNodesRequestedReviewer::Mannequin(mannequin) => mannequin.try_into(),
                        }
                        ))
                    })
                    .collect::<Result<Vec<GithubReviewer>, UniversalInboxError>>()
            })
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestCommentsNodes>
    for GithubIssueComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestCommentsNodes,
    ) -> Result<Self, Self::Error> {
        Ok(GithubIssueComment {
            url: value.url.parse().with_context(|| {
                format!("Unable to parse Github issue comment URL: {:?}", value.url)
            })?,
            body: value.body_html,
            created_at: value.created_at,
            author: value.author.map(|author| author.try_into()).transpose()?,
        })
    }
}

impl TryFrom<pull_request_query::PullRequestQueryRepositoryPullRequestCommentsNodesAuthor>
    for GithubActor
{
    type Error = UniversalInboxError;

    fn try_from(
        value: pull_request_query::PullRequestQueryRepositoryPullRequestCommentsNodesAuthor,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            pull_request_query::PullRequestQueryRepositoryPullRequestCommentsNodesAuthorOn::User(
                user,
            ) => GithubActor::User(GithubUserSummary {
                login: value.login,
                name: user.name,
                avatar_url: value
                    .avatar_url
                    .parse::<Url>()
                    .with_context(|| format!("Github actor should have a valid avatar URL: {:?}", value.avatar_url))?,
            }),
            _ => GithubActor::Bot(GithubBotSummary {
                login: value.login,
                avatar_url: value
                    .avatar_url
                    .parse::<Url>()
                    .with_context(|| format!("Github actor should have a valid avatar URL: {:?}", value.avatar_url))?,
            })
        })
    }
}
impl TryFrom<pull_request_query::ResponseData> for NotificationDetails {
    type Error = UniversalInboxError;

    fn try_from(value: pull_request_query::ResponseData) -> Result<Self, Self::Error> {
        let pr = value
            .repository
            .context("Github repository not found")?
            .pull_request
            .context("Github pull request not found")?;
        let pr_url: Url = pr
            .url
            .parse()
            .with_context(|| format!("Unable to parse Github pull request URL: {:?}", pr.url))?;

        Ok(NotificationDetails::GithubPullRequest(GithubPullRequest {
            id: pr.id,
            number: pr.number,
            url: pr_url.clone(),
            title: pr.title_html,
            body: pr.body_html,
            state: pr.state.into(),
            is_draft: pr.is_draft,
            closed_at: pr.closed_at,
            created_at: pr.created_at,
            updated_at: pr.updated_at,
            merged_at: pr.merged_at,
            mergeable_state: pr.mergeable.into(),
            merge_state_status: pr.merge_state_status.into(),
            merged_by: pr
                .merged_by
                .map(|merged_by| merged_by.try_into())
                .transpose()?,
            deletions: pr.deletions,
            additions: pr.additions,
            changed_files: pr.changed_files,
            labels: pr.labels.map(|labels| labels.into()).unwrap_or_default(),
            comments_count: pr.comments.total_count,
            comments: pr
                .comments
                .nodes
                .map(|nodes| {
                    nodes
                        .into_iter()
                        .filter_map(|node| node.and_then(|node| node.try_into().ok()))
                        .collect::<Vec<GithubIssueComment>>()
                })
                .unwrap_or_default(),
            latest_commit: TryInto::<Option<GithubCommitChecks>>::try_into(pr.commits)?
                .with_context(|| {
                    format!(
                        "Expected at least 1 commit associated with a Github pull request {pr_url}"
                    )
                })?,
            base_ref_name: pr.base_ref_name,
            base_repository: pr
                .base_repository
                .map(|repo| {
                    Ok::<GithubRepositorySummary, UniversalInboxError>(GithubRepositorySummary {
                        name_with_owner: repo.name_with_owner,
                        url: repo.url.parse().with_context(|| {
                            format!(
                                "Unable to parse Github pull request base repository URL: {:?}",
                                repo.url
                            )
                        })?,
                    })
                })
                .transpose()?,
            head_ref_name: pr.head_ref_name,
            head_repository: pr
                .head_repository
                .map(|repo| {
                    Ok::<GithubRepositorySummary, UniversalInboxError>(GithubRepositorySummary {
                        name_with_owner: repo.name_with_owner,
                        url: repo.url.parse().with_context(|| {
                            format!(
                                "Unable to parse Github pull request base repository URL: {:?}",
                                repo.url
                            )
                        })?,
                    })
                })
                .transpose()?,
            author: pr.author.map(|author| author.try_into()).transpose()?,
            assignees: pr.assignees.try_into()?,
            review_decision: pr.review_decision.map(|decision| decision.into()),
            reviews: pr
                .reviews
                .map(|reviews| reviews.try_into())
                .transpose()?
                .unwrap_or_default(),
            review_requests: pr
                .review_requests
                .map(|review_requests| review_requests.try_into())
                .transpose()?
                .unwrap_or_default(),
        }))
    }
}
