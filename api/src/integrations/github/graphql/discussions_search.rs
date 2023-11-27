use anyhow::{anyhow, Context};
use http::Uri;

use universal_inbox::notification::{
    integrations::github::{
        GithubActor, GithubBotSummary, GithubDiscussion, GithubDiscussionComment,
        GithubDiscussionStateReason, GithubLabel, GithubRepositorySummary, GithubUserSummary,
    },
    NotificationDetails,
};

use crate::{
    integrations::github::graphql::discussions_search_query, universal_inbox::UniversalInboxError,
};

impl From<discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionLabels>
    for Vec<GithubLabel>
{
    fn from(
        value: discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionLabels,
    ) -> Self {
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

impl TryFrom<discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAuthor>
    for GithubActor
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAuthor,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAuthorOn::User(
                user,
            ) => GithubActor::User(GithubUserSummary {
                login: value.login,
                name: user.name,
                avatar_url: value
                    .avatar_url
                    .parse::<Uri>()
                    .context("Github actor should have a valid avatar URL")?,
            }),
            _ => GithubActor::Bot(GithubBotSummary {
                login: value.login,
                avatar_url: value
                    .avatar_url
                    .parse::<Uri>()
                    .context("Github actor should have a valid avatar URL")?,
            })
        })
    }
}

impl TryFrom<discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswerAuthor>
    for GithubActor
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswerAuthor,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswerAuthorOn::User(
                user,
            ) => GithubActor::User(GithubUserSummary {
                login: value.login,
                name: user.name,
                avatar_url: value
                    .avatar_url
                    .parse::<Uri>()
                    .context("Github actor should have a valid avatar URL")?,
            }),
            _ => GithubActor::Bot(GithubBotSummary {
                login: value.login,
                avatar_url: value
                    .avatar_url
                    .parse::<Uri>()
                    .context("Github actor should have a valid avatar URL")?,
            })
        })
    }
}

impl TryFrom<discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswerChosenBy>
    for GithubActor
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswerChosenBy,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswerChosenByOn::User(
                user,
            ) => GithubActor::User(GithubUserSummary {
                login: value.login,
                name: user.name,
                avatar_url: value
                    .avatar_url
                    .parse::<Uri>()
                    .context("Github actor should have a valid avatar URL")?,
            }),
            _ => GithubActor::Bot(GithubBotSummary {
                login: value.login,
                avatar_url: value
                    .avatar_url
                    .parse::<Uri>()
                    .context("Github actor should have a valid avatar URL")?,
            })
        })
    }
}

impl From<discussions_search_query::DiscussionStateReason> for GithubDiscussionStateReason {
    fn from(value: discussions_search_query::DiscussionStateReason) -> Self {
        match value {
            discussions_search_query::DiscussionStateReason::DUPLICATE => {
                GithubDiscussionStateReason::Duplicate
            }
            discussions_search_query::DiscussionStateReason::OUTDATED => {
                GithubDiscussionStateReason::Outdated
            }
            discussions_search_query::DiscussionStateReason::REOPENED => {
                GithubDiscussionStateReason::Reopened
            }
            discussions_search_query::DiscussionStateReason::RESOLVED => {
                GithubDiscussionStateReason::Resolved
            }
            discussions_search_query::DiscussionStateReason::Other(_) => {
                GithubDiscussionStateReason::Resolved
            }
        }
    }
}

impl TryFrom<discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswer>
    for GithubDiscussionComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionAnswer,
    ) -> Result<Self, Self::Error> {
        Ok(GithubDiscussionComment {
            url: value
                .url
                .parse()
                .context("Unable to parse Github discussion comment URL")?,
            body: value.body_html,
            created_at: value.created_at,
            author: value.author.map(|author| author.try_into()).transpose()?,
        })
    }
}

impl TryFrom<discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionRepository>
    for GithubRepositorySummary
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussions_search_query::DiscussionsSearchQuerySearchNodesOnDiscussionRepository,
    ) -> Result<Self, Self::Error> {
        Ok(GithubRepositorySummary {
            url: value
                .url
                .parse()
                .context("Unable to parse Github repository URL")?,
            name_with_owner: value.name_with_owner,
        })
    }
}

impl TryFrom<discussions_search_query::ResponseData> for NotificationDetails {
    type Error = UniversalInboxError;

    fn try_from(value: discussions_search_query::ResponseData) -> Result<Self, Self::Error> {
        let Some(
            [Some(discussions_search_query::DiscussionsSearchQuerySearchNodes::Discussion(
                discussion,
            ))],
        ) = value.search.nodes.as_deref()
        else {
            return Err(UniversalInboxError::Recoverable(anyhow!(
                "Github discussion not found"
            )));
        };
        let discussion = discussion.clone();

        Ok(NotificationDetails::GithubDiscussion(GithubDiscussion {
            id: discussion.id,
            number: discussion.number,
            url: discussion
                .url
                .parse()
                .context("Unable to parse Github pull request URL")?,
            title: discussion.title,
            body: discussion.body_html,
            state_reason: discussion
                .state_reason
                .map(|state_reason| state_reason.into()),

            closed_at: discussion.closed_at,
            created_at: discussion.created_at,
            updated_at: discussion.updated_at,

            repository: discussion.repository.try_into()?,

            answer: discussion
                .answer
                .map(|answer| answer.try_into())
                .transpose()?,
            answer_chosen_at: discussion.answer_chosen_at,
            answer_chosen_by: discussion
                .answer_chosen_by
                .map(|answer_chosen_by| answer_chosen_by.try_into())
                .transpose()?,

            comments_count: discussion.comments.total_count,
            labels: discussion
                .labels
                .map(|labels| labels.into())
                .unwrap_or_default(),
            author: discussion
                .author
                .map(|author| author.try_into())
                .transpose()?,
        }))
    }
}
