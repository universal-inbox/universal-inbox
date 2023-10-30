#![allow(clippy::upper_case_acronyms)]

use chrono::{DateTime as ChronoDateTime, Utc};
use graphql_client::GraphQLQuery;
use universal_inbox::notification::integrations::github::GitObjectId;

pub mod discussions_search;
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
    query_path = "src/integrations/github/graphql/discussions_search_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct DiscussionsSearchQuery;
