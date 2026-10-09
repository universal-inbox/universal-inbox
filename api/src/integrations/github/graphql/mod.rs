#![allow(clippy::upper_case_acronyms)]

use chrono::{DateTime as ChronoDateTime, Utc};
use graphql_client::GraphQLQuery;

use universal_inbox::third_party::integrations::github::GitObjectId;

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

pub mod discussion;
pub mod pull_request;

// Define some GraphQL types used in the Github API
type DateTime = ChronoDateTime<Utc>;
type HTML = String;
type URI = String;
type GitObjectID = GitObjectId;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/github/graphql/schema.graphql",
    query_path = "src/integrations/github/graphql/pull_request_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct PullRequestQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/github/graphql/schema.graphql",
    query_path = "src/integrations/github/graphql/pull_request_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct PullRequestCommentsQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/github/graphql/schema.graphql",
    query_path = "src/integrations/github/graphql/pull_request_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct PullRequestReviewThreadsQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/github/graphql/schema.graphql",
    query_path = "src/integrations/github/graphql/pull_request_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct PullRequestReviewThreadCommentsQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/github/graphql/schema.graphql",
    query_path = "src/integrations/github/graphql/discussion_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct DiscussionQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/github/graphql/schema.graphql",
    query_path = "src/integrations/github/graphql/discussion_comments_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct DiscussionCommentsQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/github/graphql/schema.graphql",
    query_path = "src/integrations/github/graphql/discussion_comment_replies_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct DiscussionCommentRepliesQuery;
