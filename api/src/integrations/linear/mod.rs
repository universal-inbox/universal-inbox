use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use graphql_client::{GraphQLQuery, Response};
use http::{HeaderMap, HeaderValue};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::provider::{IntegrationProviderKind, IntegrationProviderSource},
    notification::{
        integrations::linear::LinearNotification, Notification, NotificationDetails,
        NotificationSource, NotificationSourceKind,
    },
    task::{service::TaskPatch, Task, TaskCreation, TaskSource, TaskSourceKind, TaskStatus},
    third_party::{
        integrations::linear::LinearIssue,
        item::{
            ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource, ThirdPartyItemSource,
            ThirdPartyItemSourceKind,
        },
    },
    user::UserId,
    HasHtmlUrl,
};

use crate::{
    integrations::{
        linear::graphql::{
            assigned_issues_query, issue_update_state, issue_update_subscribers,
            notification_archive, notification_subscribers_query,
            notification_update_snoozed_until_at, notifications_query, AssignedIssuesQuery,
            IssueUpdateState, IssueUpdateSubscribers, NotificationArchive,
            NotificationSubscribersQuery, NotificationUpdateSnoozedUntilAt, NotificationsQuery,
        },
        notification::{NotificationSourceService, NotificationSyncSourceService},
        oauth2::AccessToken,
        task::ThirdPartyTaskService,
        third_party::ThirdPartyItemSourceService,
        APP_USER_AGENT,
    },
    universal_inbox::{
        integration_connection::service::IntegrationConnectionService, UniversalInboxError,
    },
    utils::graphql::assert_no_error_in_graphql_response,
};

pub mod graphql;

#[derive(Clone, Debug)]
pub struct LinearService {
    linear_graphql_url: String,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
}

static LINEAR_GRAPHQL_URL: &str = "https://api.linear.app/graphql";
static LINEAR_GRAPHQL_API_NAME: &str = "Linear";

impl LinearService {
    pub fn new(
        linear_graphql_url: Option<String>,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    ) -> Result<LinearService, UniversalInboxError> {
        Ok(LinearService {
            linear_graphql_url: linear_graphql_url
                .unwrap_or_else(|| LINEAR_GRAPHQL_URL.to_string()),
            integration_connection_service,
        })
    }

    pub async fn query_notifications(
        &self,
        access_token: &AccessToken,
    ) -> Result<notifications_query::ResponseData, UniversalInboxError> {
        let request_body = NotificationsQuery::build_query(notifications_query::Variables {});

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch notifications from Linear API")?
            .text()
            .await
            .context("Failed to fetch notifications response from Linear API")?;

        let notifications_response: Response<notifications_query::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&notifications_response, LINEAR_GRAPHQL_API_NAME)?;

        Ok(notifications_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?)
    }

    pub async fn query_notification_subscribers(
        &self,
        access_token: &AccessToken,
        notification_id: String,
    ) -> Result<notification_subscribers_query::ResponseData, UniversalInboxError> {
        let request_body =
            NotificationSubscribersQuery::build_query(notification_subscribers_query::Variables {
                id: notification_id,
            });

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch notification subscribers from Linear API")?
            .text()
            .await
            .context("Failed to fetch notification subscribers response from Linear API")?;

        let notification_response: Response<notification_subscribers_query::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&notification_response, LINEAR_GRAPHQL_API_NAME)?;

        Ok(notification_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?)
    }

    pub async fn archive_notification(
        &self,
        access_token: &AccessToken,
        notification_id: String,
    ) -> Result<(), UniversalInboxError> {
        let request_body = NotificationArchive::build_query(notification_archive::Variables {
            id: notification_id,
        });

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot delete notification from Linear API")?
            .text()
            .await
            .context("Failed to fetch notification archive response from Linear API")?;

        let archive_response: Response<notification_archive::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&archive_response, LINEAR_GRAPHQL_API_NAME)?;

        let response_data = archive_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?;

        if !response_data.notification_archive.success {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Linear API call failed with an unknown error"
            )));
        }

        Ok(())
    }

    pub async fn update_issue_subscribers(
        &self,
        access_token: &AccessToken,
        issue_id: String,
        subscriber_ids: Vec<String>,
    ) -> Result<(), UniversalInboxError> {
        let request_body =
            IssueUpdateSubscribers::build_query(issue_update_subscribers::Variables {
                id: issue_id,
                subscriber_ids,
            });

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot update issue subscribers from Linear API")?
            .text()
            .await
            .context("Failed to fetch issue update subscribers response from Linear API")?;

        let update_response: Response<issue_update_subscribers::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&update_response, LINEAR_GRAPHQL_API_NAME)?;

        let response_data = update_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?;

        if !response_data.issue_update.success {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Linear API call failed with an unknown error"
            )));
        }

        Ok(())
    }

    pub async fn update_notification_snoozed_until_at(
        &self,
        access_token: &AccessToken,
        notification_id: String,
        snoozed_until_at: DateTime<Utc>,
    ) -> Result<(), UniversalInboxError> {
        let request_body = NotificationUpdateSnoozedUntilAt::build_query(
            notification_update_snoozed_until_at::Variables {
                id: notification_id,
                snoozed_until_at,
            },
        );

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot snooze issue notification from Linear API")?
            .text()
            .await
            .context("Failed to fetch notification update snooze response from Linear API")?;

        let update_response: Response<notification_update_snoozed_until_at::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&update_response, LINEAR_GRAPHQL_API_NAME)?;

        let response_data = update_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?;

        if !response_data.notification_update.success {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Linear API call failed with an unknown error"
            )));
        }

        Ok(())
    }

    pub async fn query_assigned_issues(
        &self,
        access_token: &AccessToken,
    ) -> Result<assigned_issues_query::ResponseData, UniversalInboxError> {
        let request_body = AssignedIssuesQuery::build_query(assigned_issues_query::Variables {});

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot fetch assigned issues from Linear API")?
            .text()
            .await
            .context("Failed to fetch assigned issues response from Linear API")?;

        let assigned_issues_response: Response<assigned_issues_query::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&assigned_issues_response, LINEAR_GRAPHQL_API_NAME)?;

        Ok(assigned_issues_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?)
    }

    pub async fn update_issue_state(
        &self,
        access_token: &AccessToken,
        issue_id: String,
        state_id: String,
    ) -> Result<(), UniversalInboxError> {
        let request_body = IssueUpdateState::build_query(issue_update_state::Variables {
            id: issue_id,
            state_id,
        });

        let response = build_linear_client(access_token)
            .context("Failed to build Linear client")?
            .post(&self.linear_graphql_url)
            .json(&request_body)
            .send()
            .await
            .context("Cannot update issue state from Linear API")?
            .text()
            .await
            .context("Failed to fetch issue update state response from Linear API")?;

        let update_response: Response<issue_update_state::ResponseData> =
            serde_json::from_str(&response)
                .map_err(|err| UniversalInboxError::from_json_serde_error(err, response))?;

        assert_no_error_in_graphql_response(&update_response, LINEAR_GRAPHQL_API_NAME)?;

        let response_data = update_response
            .data
            .ok_or_else(|| anyhow!("Failed to parse `data` from Linear graphql response"))?;

        if !response_data.issue_update.success {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Linear API call failed with an unknown error"
            )));
        }

        Ok(())
    }
}

fn build_linear_client(access_token: &AccessToken) -> Result<ClientWithMiddleware, reqwest::Error> {
    let mut headers = HeaderMap::new();

    let mut auth_header_value: HeaderValue = format!("Bearer {access_token}").parse().unwrap();
    auth_header_value.set_sensitive(true);
    headers.insert("Authorization", auth_header_value);

    let reqwest_client = reqwest::Client::builder()
        .default_headers(headers)
        .user_agent(APP_USER_AGENT)
        .build()?;
    Ok(ClientBuilder::new(reqwest_client)
        .with(TracingMiddleware::<SpanBackendWithUrl>::new())
        .build())
}

#[async_trait]
impl NotificationSyncSourceService for LinearService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn fetch_all_notifications<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<Notification>, UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Linear notifications without an access token"))?;

        let notifications_response = self.query_notifications(&access_token).await?;

        TryInto::<Vec<LinearNotification>>::try_into(notifications_response).map(|linear_notifs| {
            linear_notifs
                .into_iter()
                .map(|linear_notif| linear_notif.into_notification(user_id))
                .collect()
        })
    }
}

#[async_trait]
impl NotificationSourceService for LinearService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.0.to_string()), err)]
    async fn delete_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot delete Linear notification without an access token"))?;

        self.archive_notification(&access_token, notification.source_id.to_string())
            .await?;

        Ok(())
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.0.to_string()), err)]
    async fn unsubscribe_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot unsubscribe from a Linear notification without an access token")
            })?;

        let subscribers_response = self
            .query_notification_subscribers(&access_token, notification.source_id.to_string())
            .await?;

        // Only Issue notification have subscribers and can be unsubscribed
        if let notification_subscribers_query::ResponseData {
            notification: notification_subscribers_query::NotificationSubscribersQueryNotification::IssueNotification(
                notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotification {
                    user: notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotificationUser {
                        id: user_id
                    },
                    issue: notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotificationIssue {
                        subscribers: notification_subscribers_query::NotificationSubscribersQueryNotificationOnIssueNotificationIssueSubscribers {
                            nodes
                        }
                    }
                }
            )
        } = subscribers_response {
            let initial_subscribers_count = nodes.len();
            let subscriber_ids: Vec<String> = nodes
                .into_iter()
                .filter_map(|subscriber|
                            (subscriber.id != user_id).then_some(subscriber.id)
                ).collect();
            if initial_subscribers_count > subscriber_ids.len() {
                self
                    .update_issue_subscribers(&access_token, notification.source_id.to_string(), subscriber_ids)
                    .await?;
            }
        }

        self.delete_notification_from_source(executor, notification, user_id)
            .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor, notification), fields(notification_id = notification.id.0.to_string()), err)]
    async fn snooze_notification_from_source<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        notification: &Notification,
        snoozed_until_at: DateTime<Utc>,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Linear notifications without an access token"))?;

        self.update_notification_snoozed_until_at(
            &access_token,
            notification.source_id.to_string(),
            snoozed_until_at,
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, _executor, _notification), fields(notification_id = _notification.id.to_string()), err)]
    async fn fetch_notification_details<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _notification: &Notification,
        _user_id: UserId,
    ) -> Result<Option<NotificationDetails>, UniversalInboxError> {
        // Linear notification details are fetch as part of the fetch_all_notifications call
        // all details are fetch in a single GraphQL call
        Ok(None)
    }
}

impl IntegrationProviderSource for LinearService {
    fn get_integration_provider_kind(&self) -> IntegrationProviderKind {
        IntegrationProviderKind::Linear
    }
}

impl NotificationSource for LinearService {
    fn get_notification_source_kind(&self) -> NotificationSourceKind {
        NotificationSourceKind::Linear
    }

    fn is_supporting_snoozed_notifications(&self) -> bool {
        true
    }
}

impl TaskSource for LinearService {
    fn get_task_source_kind(&self) -> TaskSourceKind {
        TaskSourceKind::Linear
    }
}

impl ThirdPartyItemSource for LinearService {
    fn get_third_party_item_source_kind(&self) -> ThirdPartyItemSourceKind {
        ThirdPartyItemSourceKind::Linear
    }
}

#[async_trait]
impl ThirdPartyItemSourceService for LinearService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    async fn fetch_items<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<ThirdPartyItem>, UniversalInboxError> {
        let (access_token, integration_connection) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| anyhow!("Cannot fetch Linear task without an access token"))?;

        let assigned_issues_response = self.query_assigned_issues(&access_token).await?;

        TryInto::<Vec<LinearIssue>>::try_into(assigned_issues_response).map(|linear_issues| {
            linear_issues
                .into_iter()
                .map(|linear_issue| {
                    linear_issue.into_third_party_item(user_id, integration_connection.id)
                })
                .collect()
        })
    }

    fn is_sync_incremental(&self) -> bool {
        false
    }
}

#[async_trait]
impl ThirdPartyTaskService<LinearIssue> for LinearService {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, _executor, source, source_third_party_item),
        fields(source_id = source.id.to_string()),
        err
    )]
    async fn third_party_item_into_task<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        source: &LinearIssue,
        source_third_party_item: &ThirdPartyItem,
        task_creation: Option<TaskCreation>,
        user_id: UserId,
    ) -> Result<Box<Task>, UniversalInboxError> {
        let task_creation = task_creation.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Cannot build a Linear task without a task creation"
            ))
        })?;

        Ok(Box::new(Task {
            id: Uuid::new_v4().into(),
            title: format!("[{}]({})", source.title.clone(), source.get_html_url()),
            body: source.description.clone().unwrap_or_default(),
            status: source.state.r#type.into(),
            completed_at: source.completed_at,
            priority: source.priority.into(),
            due_at: source.due_date.map(|due_date| due_date.into()),
            tags: source
                .labels
                .iter()
                .map(|label| label.name.clone())
                .collect(),
            parent_id: None,
            project: task_creation.project.name.clone(),
            is_recurring: false,
            created_at: source.created_at,
            updated_at: source.updated_at,
            kind: TaskSourceKind::Linear,
            source_item: source_third_party_item.clone(),
            sink_item: None,
            user_id,
        }))
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    async fn delete_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot delete task from a Linear notification without an access token")
            })?;

        let ThirdPartyItemData::LinearIssue(linear_issue) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot delete a task without a Linear issue data"
            )));
        };

        let Some(state_id) = linear_issue.get_state_id_for_task_status(TaskStatus::Deleted) else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot delete a task without a Linear state ID for 'Deleted' status"
            )));
        };

        self.update_issue_state(
            &access_token,
            third_party_item.source_id.to_string(),
            state_id.to_string(),
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    async fn complete_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot complete task from a Linear notification without an access token")
            })?;

        let ThirdPartyItemData::LinearIssue(linear_issue) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot complete a task without a Linear issue data"
            )));
        };

        let Some(state_id) = linear_issue.get_state_id_for_task_status(TaskStatus::Done) else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot complete a task without a Linear state ID for 'Done' status"
            )));
        };

        self.update_issue_state(
            &access_token,
            third_party_item.source_id.to_string(),
            state_id.to_string(),
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "debug",
        skip(self, executor, third_party_item),
        fields(
            third_party_item_id = third_party_item.id.to_string(),
            third_party_item_source_id = third_party_item.source_id
        ),
        err
    )]
    async fn uncomplete_task<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        third_party_item: &ThirdPartyItem,
        user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        let (access_token, _) = self
            .integration_connection_service
            .read()
            .await
            .find_access_token(executor, IntegrationProviderKind::Linear, user_id)
            .await?
            .ok_or_else(|| {
                anyhow!("Cannot uncomplete task from a Linear notification without an access token")
            })?;

        let ThirdPartyItemData::LinearIssue(linear_issue) = &third_party_item.data else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot uncomplete a task without a Linear issue data"
            )));
        };

        let Some(state_id) = linear_issue.get_state_id_for_task_status(TaskStatus::Active) else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot uncomplete a task without a Linear state ID for 'Active' status"
            )));
        };

        self.update_issue_state(
            &access_token,
            third_party_item.source_id.to_string(),
            state_id.to_string(),
        )
        .await
    }

    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(level = "debug", skip(self, _executor, _id, _patch, _user_id), err)]
    async fn update_task<'a>(
        &self,
        _executor: &mut Transaction<'a, Postgres>,
        _id: &str,
        _patch: &TaskPatch,
        _user_id: UserId,
    ) -> Result<(), UniversalInboxError> {
        // Nothing to do here for now
        Ok(())
    }
}
