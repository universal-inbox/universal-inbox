use anyhow::Context;
use url::Url;

use universal_inbox::third_party::integrations::github::{
    GithubActor, GithubBotSummary, GithubDiscussion, GithubDiscussionComment,
    GithubDiscussionStateReason, GithubLabel, GithubRepositorySummary, GithubUserSummary,
};

use crate::{
    integrations::github::graphql::discussion_query, universal_inbox::UniversalInboxError,
};

impl From<discussion_query::DiscussionQueryRepositoryDiscussionLabels> for Vec<GithubLabel> {
    fn from(value: discussion_query::DiscussionQueryRepositoryDiscussionLabels) -> Self {
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

impl TryFrom<discussion_query::DiscussionQueryRepositoryDiscussionAuthor> for GithubActor {
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_query::DiscussionQueryRepositoryDiscussionAuthor,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            discussion_query::DiscussionQueryRepositoryDiscussionAuthorOn::User(user) => {
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

impl TryFrom<discussion_query::DiscussionQueryRepositoryDiscussionAnswerAuthor> for GithubActor {
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_query::DiscussionQueryRepositoryDiscussionAnswerAuthor,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            discussion_query::DiscussionQueryRepositoryDiscussionAnswerAuthorOn::User(user) => {
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

impl TryFrom<discussion_query::DiscussionQueryRepositoryDiscussionAnswerChosenBy> for GithubActor {
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_query::DiscussionQueryRepositoryDiscussionAnswerChosenBy,
    ) -> Result<Self, Self::Error> {
        Ok(match value.on {
            discussion_query::DiscussionQueryRepositoryDiscussionAnswerChosenByOn::User(user) => {
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

impl From<discussion_query::DiscussionStateReason> for GithubDiscussionStateReason {
    fn from(value: discussion_query::DiscussionStateReason) -> Self {
        match value {
            discussion_query::DiscussionStateReason::DUPLICATE => {
                GithubDiscussionStateReason::Duplicate
            }
            discussion_query::DiscussionStateReason::OUTDATED => {
                GithubDiscussionStateReason::Outdated
            }
            discussion_query::DiscussionStateReason::REOPENED => {
                GithubDiscussionStateReason::Reopened
            }
            discussion_query::DiscussionStateReason::RESOLVED => {
                GithubDiscussionStateReason::Resolved
            }
            discussion_query::DiscussionStateReason::Other(_) => {
                GithubDiscussionStateReason::Resolved
            }
        }
    }
}

impl TryFrom<discussion_query::DiscussionQueryRepositoryDiscussionAnswer>
    for GithubDiscussionComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_query::DiscussionQueryRepositoryDiscussionAnswer,
    ) -> Result<Self, Self::Error> {
        Ok(GithubDiscussionComment {
            url: value.url.parse().with_context(|| {
                format!(
                    "Unable to parse Github discussion comment URL: {:?}",
                    value.url
                )
            })?,
            body: value.body_html,
            created_at: value.created_at,
            author: value.author.map(|author| author.try_into()).transpose()?,
        })
    }
}

impl TryFrom<discussion_query::DiscussionQueryRepositoryDiscussionRepository>
    for GithubRepositorySummary
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_query::DiscussionQueryRepositoryDiscussionRepository,
    ) -> Result<Self, Self::Error> {
        Ok(GithubRepositorySummary {
            url: value.url.parse().with_context(|| {
                format!("Unable to parse Github repository URL: {:?}", value.url)
            })?,
            name_with_owner: value.name_with_owner,
        })
    }
}

impl TryFrom<discussion_query::ResponseData> for GithubDiscussion {
    type Error = UniversalInboxError;

    fn try_from(value: discussion_query::ResponseData) -> Result<Self, Self::Error> {
        let discussion = value
            .repository
            .context("Github repository not found")?
            .discussion
            .context("Github discussion not found")?;

        Ok(GithubDiscussion {
            id: discussion.id,
            number: discussion.number,
            url: discussion.url.parse().with_context(|| {
                format!(
                    "Unable to parse Github discussion URL: {:?}",
                    discussion.url
                )
            })?,
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
        })
    }
}
