#![allow(clippy::all, warnings)]
pub struct NotificationsQuery;
pub mod notifications_query {
    #![allow(dead_code)]
    use std::result::Result;
    pub const OPERATION_NAME: &str = "NotificationsQuery";
    pub const QUERY : & str = "query NotificationsQuery {\n  notifications {\n    nodes {\n      __typename\n      id\n      type\n      readAt\n      updatedAt\n      snoozedUntilAt\n      ... on IssueNotification {\n        issue {\n          id\n          createdAt\n          updatedAt\n          startedAt\n          completedAt\n          canceledAt\n          dueDate\n          identifier\n          title\n          url\n          priority\n          project {\n            id\n            name\n            url\n            description\n            icon\n            color\n            state\n            progress\n          }\n          projectMilestone {\n            name\n            description\n          }\n          creator {\n            name\n            avatarUrl\n            url\n          }\n          assignee {\n            name\n            avatarUrl\n            url\n          }\n          state {\n            name\n            color\n            description\n            type\n          }\n          labels {\n            nodes {\n              name\n              description\n              color\n            }\n          }\n          description\n          team {\n            id\n            key\n            name\n          }\n        }\n      }\n      ... on ProjectNotification {\n        project {\n          id\n          name\n          url\n          description\n          icon\n          color\n          state\n          progress\n        }\n      }\n    }\n  }\n\n  organization {\n    name\n    urlKey\n    logoUrl\n  }\n}\n" ;
    use super::*;
    use serde::{Deserialize, Serialize};
    #[allow(dead_code)]
    type Boolean = bool;
    #[allow(dead_code)]
    type Float = f64;
    #[allow(dead_code)]
    type Int = i64;
    #[allow(dead_code)]
    type ID = String;
    type DateTime = super::DateTime;
    type TimelessDate = super::TimelessDate;
    #[derive(Serialize)]
    pub struct Variables;
    #[derive(Deserialize)]
    pub struct ResponseData {
        pub notifications: NotificationsQueryNotifications,
        pub organization: NotificationsQueryOrganization,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotifications {
        pub nodes: Vec<NotificationsQueryNotificationsNodes>,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodes {
        pub id: ID,
        #[serde(rename = "type")]
        pub type_: String,
        #[serde(rename = "readAt")]
        pub read_at: Option<DateTime>,
        #[serde(rename = "updatedAt")]
        pub updated_at: DateTime,
        #[serde(rename = "snoozedUntilAt")]
        pub snoozed_until_at: Option<DateTime>,
        #[serde(flatten)]
        pub on: NotificationsQueryNotificationsNodesOn,
    }
    #[derive(Deserialize)]
    #[serde(tag = "__typename")]
    pub enum NotificationsQueryNotificationsNodesOn {
        IssueNotification(NotificationsQueryNotificationsNodesOnIssueNotification),
        ProjectNotification(NotificationsQueryNotificationsNodesOnProjectNotification),
        OauthClientApprovalNotification,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotification {
        pub issue: NotificationsQueryNotificationsNodesOnIssueNotificationIssue,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssue {
        pub id: ID,
        #[serde(rename = "createdAt")]
        pub created_at: DateTime,
        #[serde(rename = "updatedAt")]
        pub updated_at: DateTime,
        #[serde(rename = "startedAt")]
        pub started_at: Option<DateTime>,
        #[serde(rename = "completedAt")]
        pub completed_at: Option<DateTime>,
        #[serde(rename = "canceledAt")]
        pub canceled_at: Option<DateTime>,
        #[serde(rename = "dueDate")]
        pub due_date: Option<TimelessDate>,
        pub identifier: String,
        pub title: String,
        pub url: String,
        pub priority: Float,
        pub project: Option<NotificationsQueryNotificationsNodesOnIssueNotificationIssueProject>,
        #[serde(rename = "projectMilestone")]
        pub project_milestone:
            Option<NotificationsQueryNotificationsNodesOnIssueNotificationIssueProjectMilestone>,
        pub creator: Option<NotificationsQueryNotificationsNodesOnIssueNotificationIssueCreator>,
        pub assignee: Option<NotificationsQueryNotificationsNodesOnIssueNotificationIssueAssignee>,
        pub state: NotificationsQueryNotificationsNodesOnIssueNotificationIssueState,
        pub labels: NotificationsQueryNotificationsNodesOnIssueNotificationIssueLabels,
        pub description: Option<String>,
        pub team: NotificationsQueryNotificationsNodesOnIssueNotificationIssueTeam,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueProject {
        pub id: ID,
        pub name: String,
        pub url: String,
        pub description: String,
        pub icon: Option<String>,
        pub color: String,
        pub state: String,
        pub progress: Float,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueProjectMilestone {
        pub name: String,
        pub description: Option<String>,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueCreator {
        pub name: String,
        #[serde(rename = "avatarUrl")]
        pub avatar_url: Option<String>,
        pub url: String,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueAssignee {
        pub name: String,
        #[serde(rename = "avatarUrl")]
        pub avatar_url: Option<String>,
        pub url: String,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueState {
        pub name: String,
        pub color: String,
        pub description: Option<String>,
        #[serde(rename = "type")]
        pub type_: String,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueLabels {
        pub nodes: Vec<NotificationsQueryNotificationsNodesOnIssueNotificationIssueLabelsNodes>,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueLabelsNodes {
        pub name: String,
        pub description: Option<String>,
        pub color: String,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnIssueNotificationIssueTeam {
        pub id: ID,
        pub key: String,
        pub name: String,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnProjectNotification {
        pub project: NotificationsQueryNotificationsNodesOnProjectNotificationProject,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryNotificationsNodesOnProjectNotificationProject {
        pub id: ID,
        pub name: String,
        pub url: String,
        pub description: String,
        pub icon: Option<String>,
        pub color: String,
        pub state: String,
        pub progress: Float,
    }
    #[derive(Deserialize)]
    pub struct NotificationsQueryOrganization {
        pub name: String,
        #[serde(rename = "urlKey")]
        pub url_key: String,
        #[serde(rename = "logoUrl")]
        pub logo_url: Option<String>,
    }
}
impl graphql_client::GraphQLQuery for NotificationsQuery {
    type Variables = notifications_query::Variables;
    type ResponseData = notifications_query::ResponseData;
    fn build_query(variables: Self::Variables) -> ::graphql_client::QueryBody<Self::Variables> {
        graphql_client::QueryBody {
            variables,
            query: notifications_query::QUERY,
            operation_name: notifications_query::OPERATION_NAME,
        }
    }
}
