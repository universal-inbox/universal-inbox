use anyhow::Context;
use url::Url;

use universal_inbox::third_party::integrations::github::{
    GithubActor, GithubBotSummary, GithubDiscussion, GithubDiscussionCategory,
    GithubDiscussionComment, GithubDiscussionStateReason, GithubDiscussionThreadComment,
    GithubLabel, GithubRepositorySummary, GithubUserSummary,
};

use crate::{
    integrations::github::graphql::{
        discussion_comment_replies_query, discussion_comments_query, discussion_query,
    },
    universal_inbox::UniversalInboxError,
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

/// Each actor selection in the query gets its own generated type; they all share
/// the same shape (`login`, `avatar_url` and a `User`-only `name`).
macro_rules! impl_github_actor_try_from {
    ($module:ident, $actor:ident, $actor_on:ident) => {
        impl TryFrom<$module::$actor> for GithubActor {
            type Error = UniversalInboxError;

            fn try_from(value: $module::$actor) -> Result<Self, Self::Error> {
                let avatar_url = value.avatar_url.parse::<Url>().with_context(|| {
                    format!(
                        "Github actor should have a valid avatar URL: {:?}",
                        value.avatar_url
                    )
                })?;
                Ok(match value.on {
                    $module::$actor_on::User(user) => GithubActor::User(GithubUserSummary {
                        login: value.login,
                        name: user.name,
                        avatar_url,
                    }),
                    _ => GithubActor::Bot(GithubBotSummary {
                        login: value.login,
                        avatar_url,
                    }),
                })
            }
        }
    };
}

impl_github_actor_try_from!(
    discussion_query,
    DiscussionQueryRepositoryDiscussionAuthor,
    DiscussionQueryRepositoryDiscussionAuthorOn
);
impl_github_actor_try_from!(
    discussion_query,
    DiscussionQueryRepositoryDiscussionAnswerAuthor,
    DiscussionQueryRepositoryDiscussionAnswerAuthorOn
);
impl_github_actor_try_from!(
    discussion_query,
    DiscussionQueryRepositoryDiscussionAnswerChosenBy,
    DiscussionQueryRepositoryDiscussionAnswerChosenByOn
);
impl_github_actor_try_from!(
    discussion_comments_query,
    DiscussionCommentsQueryRepositoryDiscussionCommentsNodesAuthor,
    DiscussionCommentsQueryRepositoryDiscussionCommentsNodesAuthorOn
);
impl_github_actor_try_from!(
    discussion_comments_query,
    DiscussionCommentsQueryRepositoryDiscussionCommentsNodesRepliesNodesAuthor,
    DiscussionCommentsQueryRepositoryDiscussionCommentsNodesRepliesNodesAuthorOn
);
impl_github_actor_try_from!(
    discussion_comment_replies_query,
    DiscussionCommentRepliesQueryNodeOnDiscussionCommentRepliesNodesAuthor,
    DiscussionCommentRepliesQueryNodeOnDiscussionCommentRepliesNodesAuthorOn
);

impl From<discussion_query::DiscussionQueryRepositoryDiscussionCategory>
    for GithubDiscussionCategory
{
    fn from(value: discussion_query::DiscussionQueryRepositoryDiscussionCategory) -> Self {
        let emoji = if value.emoji.is_empty() {
            None
        } else {
            Some(value.emoji)
        };
        GithubDiscussionCategory {
            name: value.name,
            emoji,
            slug: value.slug,
            is_answerable: value.is_answerable,
        }
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

fn parse_comment_url(url: &str) -> Result<Url, UniversalInboxError> {
    Ok(url
        .parse()
        .with_context(|| format!("Unable to parse Github discussion comment URL: {url:?}"))?)
}

/// One page of top-level discussion comments.
pub struct DiscussionCommentsPage {
    /// Each comment with the cursor of its next replies page, if it has more
    /// replies than the first page.
    pub comments: Vec<(GithubDiscussionThreadComment, Option<String>)>,
    pub next_cursor: Option<String>,
}

/// One page of replies to a discussion comment.
pub struct DiscussionRepliesPage {
    pub replies: Vec<GithubDiscussionComment>,
    pub next_cursor: Option<String>,
}

fn next_cursor(has_next_page: bool, end_cursor: Option<String>) -> Option<String> {
    if has_next_page { end_cursor } else { None }
}

impl TryFrom<discussion_comments_query::DiscussionCommentsQueryRepositoryDiscussionCommentsNodesRepliesNodes>
    for GithubDiscussionComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_comments_query::DiscussionCommentsQueryRepositoryDiscussionCommentsNodesRepliesNodes,
    ) -> Result<Self, Self::Error> {
        Ok(GithubDiscussionComment {
            url: parse_comment_url(&value.url)?,
            body: value.body_html,
            created_at: value.created_at,
            author: value.author.map(|author| author.try_into()).transpose()?,
        })
    }
}

impl TryFrom<discussion_comment_replies_query::DiscussionCommentRepliesQueryNodeOnDiscussionCommentRepliesNodes>
    for GithubDiscussionComment
{
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_comment_replies_query::DiscussionCommentRepliesQueryNodeOnDiscussionCommentRepliesNodes,
    ) -> Result<Self, Self::Error> {
        Ok(GithubDiscussionComment {
            url: parse_comment_url(&value.url)?,
            body: value.body_html,
            created_at: value.created_at,
            author: value.author.map(|author| author.try_into()).transpose()?,
        })
    }
}

impl TryFrom<discussion_comments_query::ResponseData> for DiscussionCommentsPage {
    type Error = UniversalInboxError;

    fn try_from(value: discussion_comments_query::ResponseData) -> Result<Self, Self::Error> {
        let comments = value
            .repository
            .context("Github repository not found")?
            .discussion
            .context("Github discussion not found")?
            .comments;

        Ok(DiscussionCommentsPage {
            comments: comments
                .nodes
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .filter(|comment| !comment.is_minimized)
                .map(|comment| {
                    let replies = comment.replies;
                    Ok((
                        GithubDiscussionThreadComment {
                            id: comment.id,
                            comment: GithubDiscussionComment {
                                url: parse_comment_url(&comment.url)?,
                                body: comment.body_html,
                                created_at: comment.created_at,
                                author: comment
                                    .author
                                    .map(|author| author.try_into())
                                    .transpose()?,
                            },
                            is_answer: comment.is_answer,
                            replies: replies
                                .nodes
                                .unwrap_or_default()
                                .into_iter()
                                .flatten()
                                .filter(|reply| !reply.is_minimized)
                                .map(|reply| reply.try_into())
                                .collect::<Result<Vec<_>, UniversalInboxError>>()?,
                            replies_count: replies.total_count,
                        },
                        next_cursor(
                            replies.page_info.has_next_page,
                            replies.page_info.end_cursor,
                        ),
                    ))
                })
                .collect::<Result<Vec<_>, UniversalInboxError>>()?,
            next_cursor: next_cursor(
                comments.page_info.has_next_page,
                comments.page_info.end_cursor,
            ),
        })
    }
}

impl TryFrom<discussion_comment_replies_query::ResponseData> for DiscussionRepliesPage {
    type Error = UniversalInboxError;

    fn try_from(
        value: discussion_comment_replies_query::ResponseData,
    ) -> Result<Self, Self::Error> {
        let replies = match value.node.context("Github discussion comment not found")? {
            discussion_comment_replies_query::DiscussionCommentRepliesQueryNode::DiscussionComment(
                comment,
            ) => comment.replies,
            _ => return Err(anyhow::anyhow!("Github node is not a discussion comment").into()),
        };

        Ok(DiscussionRepliesPage {
            replies: replies
                .nodes
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .filter(|reply| !reply.is_minimized)
                .map(|reply| reply.try_into())
                .collect::<Result<Vec<_>, _>>()?,
            next_cursor: next_cursor(
                replies.page_info.has_next_page,
                replies.page_info.end_cursor,
            ),
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
            category: Some(discussion.category.into()),
            // Fetched separately, page by page (see `DiscussionCommentsPage`)
            comments: vec![],
        })
    }
}
