use chrono::{DateTime as ChronoDateTime, NaiveDate, Utc};
use graphql_client::GraphQLQuery;

pub mod issue;
pub mod notification;

type DateTime = ChronoDateTime<Utc>;
type TimelessDate = NaiveDate;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/graphql/schema.json",
    query_path = "src/integrations/linear/graphql/notifications_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct NotificationsQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/graphql/schema.json",
    query_path = "src/integrations/linear/graphql/notification_archive.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct NotificationArchive;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/graphql/schema.json",
    query_path = "src/integrations/linear/graphql/issue_update_subscribers.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct IssueUpdateSubscribers;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/graphql/schema.json",
    query_path = "src/integrations/linear/graphql/notification_update_snoozed_until_at.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct NotificationUpdateSnoozedUntilAt;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/graphql/schema.json",
    query_path = "src/integrations/linear/graphql/notification_subscribers_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct NotificationSubscribersQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/graphql/schema.json",
    query_path = "src/integrations/linear/graphql/assigned_issues_query.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct AssignedIssuesQuery;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/integrations/linear/graphql/schema.json",
    query_path = "src/integrations/linear/graphql/issue_update_state.graphql",
    response_derives = "Debug,Clone,Serialize",
    variables_derives = "Deserialize"
)]
pub struct IssueUpdateState;
