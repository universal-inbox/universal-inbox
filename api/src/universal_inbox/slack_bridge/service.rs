use std::sync::Arc;

use chrono::{TimeDelta, Utc};
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        integrations::slack::{SlackContext, SlackExtensionCredential},
        provider::{IntegrationConnectionContext, IntegrationProviderKind},
    },
    notification::NotificationId,
    slack_bridge::{
        SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction,
        SlackBridgePendingActionId, SlackBridgeStatus,
    },
    user::UserId,
};

use universal_inbox::integration_connection::IntegrationConnectionStatus;

use crate::{
    repository::{
        Repository, integration_connection::IntegrationConnectionRepository,
        slack_bridge::SlackBridgeRepository,
    },
    universal_inbox::UniversalInboxError,
};

const EXTENSION_HEARTBEAT_TIMEOUT_SECONDS: i64 = 120;

pub struct SlackBridgeService {
    repository: Arc<Repository>,
}

impl SlackBridgeService {
    pub fn new(repository: Arc<Repository>) -> Self {
        Self { repository }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            user_id = %user_id,
            action_type = %action_type,
        ),
        err
    )]
    pub async fn create_pending_action(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        notification_id: Option<NotificationId>,
        action_type: SlackBridgeActionType,
        slack_team_id: SlackTeamId,
        slack_channel_id: SlackChannelId,
        slack_thread_ts: SlackTs,
        slack_last_message_ts: SlackTs,
    ) -> Result<SlackBridgePendingAction, UniversalInboxError> {
        let action = SlackBridgePendingAction {
            id: Uuid::new_v4().into(),
            user_id,
            notification_id,
            action_type,
            slack_team_id,
            slack_channel_id,
            slack_thread_ts,
            slack_last_message_ts,
            status: SlackBridgeActionStatus::Pending,
            failure_message: None,
            retry_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };

        self.repository
            .create_pending_action(executor, &action)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user_id = %user_id),
        err
    )]
    pub async fn get_actionable_actions_for_extension(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        credentials: Vec<SlackExtensionCredential>,
    ) -> Result<Vec<SlackBridgePendingAction>, UniversalInboxError> {
        // Update heartbeat and credentials on the Slack integration connection
        let integration_connection = self
            .repository
            .get_integration_connection_per_provider(
                executor,
                user_id,
                IntegrationProviderKind::Slack,
                None,
                Some(IntegrationConnectionStatus::Validated),
            )
            .await?;

        if let Some(integration_connection) = integration_connection
            && let universal_inbox::integration_connection::provider::IntegrationProvider::Slack {
                context: Some(context),
                ..
            } = &integration_connection.provider
        {
            let updated_context = IntegrationConnectionContext::Slack(SlackContext {
                team_id: context.team_id.clone(),
                extension_credentials: credentials,
                last_extension_heartbeat_at: Some(Utc::now()),
            });

            self.repository
                .update_integration_connection_context(
                    executor,
                    integration_connection.id,
                    Some(updated_context),
                )
                .await?;
        }

        self.repository
            .get_actionable_actions(executor, user_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(action_id = %action_id),
        err
    )]
    pub async fn complete_action(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action_id: SlackBridgePendingActionId,
        user_id: UserId,
    ) -> Result<Option<SlackBridgePendingAction>, UniversalInboxError> {
        let result = self
            .repository
            .mark_action_completed(executor, action_id, user_id)
            .await?;
        if result.is_none() {
            tracing::warn!(
                %action_id, %user_id,
                "Slack bridge action not marked as completed: action missing or in terminal state"
            );
        }
        Ok(result)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(action_id = %action_id),
        err
    )]
    pub async fn fail_action(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        action_id: SlackBridgePendingActionId,
        user_id: UserId,
        error: &str,
    ) -> Result<Option<SlackBridgePendingAction>, UniversalInboxError> {
        let result = self
            .repository
            .mark_action_failed(executor, action_id, user_id, error)
            .await?;
        if result.is_none() {
            tracing::warn!(
                %action_id, %user_id,
                "Slack bridge action not marked as failed: action missing or in terminal state"
            );
        }
        Ok(result)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user_id = %user_id),
        err
    )]
    pub async fn get_bridge_status(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<SlackBridgeStatus, UniversalInboxError> {
        let integration_connection = self
            .repository
            .get_integration_connection_per_provider(
                executor,
                user_id,
                IntegrationProviderKind::Slack,
                None,
                Some(IntegrationConnectionStatus::Validated),
            )
            .await?;

        let (extension_connected, team_id_match, user_id_match) =
            if let Some(ref integration_connection) = integration_connection {
                match &integration_connection.provider {
                    universal_inbox::integration_connection::provider::IntegrationProvider::Slack {
                        context: Some(context),
                        ..
                    } => {
                        let connected = context
                            .last_extension_heartbeat_at
                            .map(|heartbeat| {
                                Utc::now() - heartbeat
                                    < TimeDelta::seconds(EXTENSION_HEARTBEAT_TIMEOUT_SECONDS)
                            })
                            .unwrap_or(false);

                        let matching_credential = context
                            .extension_credentials
                            .iter()
                            .find(|c| c.team_id == context.team_id);

                        let team_match = matching_credential.is_some();

                        let user_match = matching_credential
                            .zip(integration_connection.provider_user_id.as_ref())
                            .is_some_and(|(cred, provider_uid)| cred.user_id == *provider_uid);

                        (connected, team_match, user_match)
                    }
                    _ => (false, false, false),
                }
            } else {
                (false, false, false)
            };

        let (pending_actions_count, failed_actions_count, last_completed_at) =
            self.repository.get_bridge_status(executor, user_id).await?;

        Ok(SlackBridgeStatus {
            extension_connected,
            team_id_match,
            user_id_match,
            pending_actions_count,
            failed_actions_count,
            last_completed_at,
        })
    }
}
