use std::sync::Arc;

use anyhow::anyhow;
use slack_morphism::prelude::*;
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::warn;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection,
        integrations::slack::{SlackConfig, SlackContext, SlackMessageConfig},
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    notification::NotificationStatus,
    third_party::{
        integrations::slack::SlackThread,
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemKind},
    },
    user::UserId,
};

use crate::observability::RecordSpanError;
use crate::observability::attr;
use crate::{
    integrations::slack::{SlackService, find_slack_references_in_message},
    jobs::slack::event_trace::SlackEventTrace,
    universal_inbox::{
        UniversalInboxError, UpsertStatus,
        integration_connection::service::IntegrationConnectionService,
        notification::{
            NotificationEventOutcome, NotificationEventService, service::NotificationService,
        },
        third_party::service::ThirdPartyItemService,
    },
};

/// What a Slack message event did to one user's notification, recorded as
/// `slack.message.outcome` on the per-user span and logged once per user.
///
/// `slack.message.outcome` values:
/// - `created`, `updated`: the notification is new, or changed, and visible (Unread)
/// - `unchanged`: neither the thread nor the notification changed (eg. a replayed event)
/// - `unsubscribed`: saved as Unsubscribed (the user unsubscribed from the thread)
/// - `deleted_last_from_user`: saved as Deleted, the user sent the last message
/// - `deleted_already_read`: saved as Deleted, the user already read the last message in Slack
/// - `deleted`: saved as Deleted for another reason (eg. a removed reaction)
/// - a `slack.fetch.skip_reason` (`thread_not_accessible`, `no_channel_id`, ...): there was
///   nothing to fetch for this user
/// - `sync_disabled`: the user disabled Slack message sync
///
/// Logged for the event rather than a user: `no_recipient` (no user is concerned),
/// `not_a_message_event`, and `error` (see `SlackEventTrace::log_worker_failure`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlackMessageDecision {
    Created,
    Updated,
    Unchanged,
    Unsubscribed,
    DeletedLastFromUser,
    DeletedAlreadyRead,
    Deleted,
    Skipped(&'static str),
    SyncDisabled,
    NoRecipient,
    NotAMessageEvent,
}

/// How the notification row changed, from its `UpsertStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationChange {
    Created,
    Updated,
    Untouched,
}

/// The state of a Slack thread that explains why its notification is Deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlackThreadState {
    last_message_from_user: bool,
    last_message_read: bool,
}

impl SlackThreadState {
    fn of(slack_thread: &SlackThread) -> Self {
        SlackThreadState {
            last_message_from_user: slack_thread.is_last_message_from_user(),
            last_message_read: slack_thread.last_read.as_ref()
                == Some(&slack_thread.messages.last().origin.ts),
        }
    }
}

impl SlackMessageDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            SlackMessageDecision::Created => "created",
            SlackMessageDecision::Updated => "updated",
            SlackMessageDecision::Unchanged => "unchanged",
            SlackMessageDecision::Unsubscribed => "unsubscribed",
            SlackMessageDecision::DeletedLastFromUser => "deleted_last_from_user",
            SlackMessageDecision::DeletedAlreadyRead => "deleted_already_read",
            SlackMessageDecision::Deleted => "deleted",
            SlackMessageDecision::Skipped(reason) => reason,
            SlackMessageDecision::SyncDisabled => "sync_disabled",
            SlackMessageDecision::NoRecipient => "no_recipient",
            SlackMessageDecision::NotAMessageEvent => "not_a_message_event",
        }
    }

    fn from_outcome(outcome: &NotificationEventOutcome) -> Self {
        let upsert_notification = match outcome {
            NotificationEventOutcome::Skipped(reason) => {
                return SlackMessageDecision::Skipped(reason);
            }
            NotificationEventOutcome::ItemUnchanged => return SlackMessageDecision::Unchanged,
            NotificationEventOutcome::Saved(upsert_notification) => upsert_notification,
        };
        let change = match upsert_notification {
            UpsertStatus::Created(_) => NotificationChange::Created,
            UpsertStatus::Updated { .. } => NotificationChange::Updated,
            UpsertStatus::Untouched(_) => NotificationChange::Untouched,
        };
        let notification = upsert_notification.value_ref();
        let slack_thread_state = match &notification.source_item.data {
            ThirdPartyItemData::SlackThread(slack_thread) => {
                Some(SlackThreadState::of(slack_thread))
            }
            _ => None,
        };
        Self::from_saved(change, notification.status, slack_thread_state)
    }

    fn from_saved(
        change: NotificationChange,
        status: NotificationStatus,
        slack_thread_state: Option<SlackThreadState>,
    ) -> Self {
        // An untouched notification was decided by an earlier event: whatever its status,
        // this event changed nothing.
        if change == NotificationChange::Untouched {
            return SlackMessageDecision::Unchanged;
        }
        match status {
            NotificationStatus::Unsubscribed => SlackMessageDecision::Unsubscribed,
            NotificationStatus::Deleted => match slack_thread_state {
                Some(SlackThreadState {
                    last_message_from_user: true,
                    ..
                }) => SlackMessageDecision::DeletedLastFromUser,
                Some(SlackThreadState {
                    last_message_read: true,
                    ..
                }) => SlackMessageDecision::DeletedAlreadyRead,
                _ => SlackMessageDecision::Deleted,
            },
            _ if change == NotificationChange::Created => SlackMessageDecision::Created,
            _ => SlackMessageDecision::Updated,
        }
    }
}

#[tracing::instrument(level = "info", skip_all, fields({ attr::ERROR_TYPE } = tracing::field::Empty))]
pub async fn handle_slack_message_push_event(
    executor: &mut Transaction<'_, Postgres>,
    event: &SlackPushEventCallback,
    notification_service: Arc<RwLock<NotificationService>>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    slack_service: Arc<SlackService>,
) -> Result<Vec<(UserId, SlackMessageDecision)>, UniversalInboxError> {
    let trace = SlackEventTrace::from_event(event);
    let result: Result<Vec<(UserId, SlackMessageDecision)>, UniversalInboxError> = async {
        let current_span = tracing::Span::current();
        trace.record_on(&current_span);
        let mut decisions = Vec::new();

        let (provider_user_ids, thread_ts) = match event {
            SlackPushEventCallback {
                team_id,
                event:
                    SlackEventCallbackBody::Message(SlackMessageEvent {
                        origin: SlackMessageOrigin { thread_ts, .. },
                        content: Some(content),
                        ..
                    }),
                ..
            } => {
                let references = find_slack_references_in_message(content);
                let mut user_ids: Vec<String> =
                    references.users.keys().map(|id| id.to_string()).collect();

                if let Some((access_token, _)) = integration_connection_service
                    .read()
                    .await
                    .find_slack_access_token(
                        executor,
                        IntegrationConnectionContext::Slack(SlackContext {
                            team_id: team_id.clone(),
                            extension_credentials: vec![],
                            last_extension_heartbeat_at: None,
                        }),
                    )
                    .await?
                {
                    let slack_api_token =
                        SlackApiToken::new(SlackApiTokenValue(access_token.as_str().to_string()))
                            .with_team_id(team_id.clone());
                    for usergroup_id in references.usergroups.keys() {
                        let usergroup_users = slack_service
                            .list_users_in_usergroup(usergroup_id, &slack_api_token)
                            .await?;
                        user_ids.extend(
                            usergroup_users
                                .iter()
                                .map(|user_id| user_id.to_string())
                                .collect::<Vec<String>>(),
                        );
                    }
                }

                (user_ids, thread_ts.clone())
            }
            _ => {
                current_span.set_attribute(attr::SLACK_MESSAGE_OUTCOME, "discarded");
                current_span
                    .set_attribute(attr::SLACK_MESSAGE_DISCARD_REASON, "not_a_message_event");
                warn!("Slack push event is not a message event");
                trace.log_user_decision(SlackMessageDecision::NotAMessageEvent, None, None);
                return Ok(decisions);
            }
        };

        current_span.set_attribute(
            attr::SLACK_REFERENCED_USERS_COUNT,
            provider_user_ids.len() as i64,
        );

        let integration_connections = integration_connection_service
            .read()
            .await
            .find_integration_connection_per_provider_user_ids(
                executor,
                IntegrationProviderKind::Slack,
                provider_user_ids,
            )
            .await?;
        let handled_integration_connection_ids = integration_connections
            .iter()
            .map(|integration_connection| integration_connection.id)
            .collect::<Vec<_>>();

        current_span.set_attribute(
            attr::SLACK_MATCHED_INTEGRATION_CONNECTIONS_COUNT,
            integration_connections.len() as i64,
        );

        for integration_connection in integration_connections {
            decisions.push(
                handle_slack_message_push_event_if_enabled(
                    executor,
                    event,
                    &trace,
                    integration_connection,
                    None,
                    notification_service.clone(),
                )
                .await?,
            );
        }

        let Some(thread_ts) = thread_ts else {
            log_if_no_recipient(&trace, &decisions);
            return Ok(decisions);
        };
        let third_party_items = third_party_item_service
            .read()
            .await
            .find_third_party_items_for_source_id(
                executor,
                ThirdPartyItemKind::SlackThread,
                thread_ts.as_ref(),
                None,
            )
            .await?;

        current_span.set_attribute(
            attr::SLACK_KNOWN_THREAD_ITEMS_COUNT,
            third_party_items.len() as i64,
        );

        for third_party_item in third_party_items.iter() {
            if !handled_integration_connection_ids
                .contains(&third_party_item.integration_connection_id)
                && let Some(integration_connection) = integration_connection_service
                    .read()
                    .await
                    .get_integration_connection(
                        executor,
                        third_party_item.integration_connection_id,
                    )
                    .await?
            {
                decisions.push(
                    handle_slack_message_push_event_if_enabled(
                        executor,
                        event,
                        &trace,
                        integration_connection,
                        Some(third_party_item),
                        notification_service.clone(),
                    )
                    .await?,
                );
            }
        }

        log_if_no_recipient(&trace, &decisions);
        Ok(decisions)
    }
    .await;
    if let Err(error) = &result {
        trace.log_worker_failure(error);
    }
    result.record_span_error()
}

/// End the log trail of an event no user is notified about (no mentioned user with a Slack
/// connection, no known thread), so it never ends silently.
fn log_if_no_recipient(trace: &SlackEventTrace, decisions: &[(UserId, SlackMessageDecision)]) {
    if decisions.is_empty() {
        trace.log_user_decision(SlackMessageDecision::NoRecipient, None, None);
    }
}

#[tracing::instrument(level = "info", skip_all)]
async fn handle_slack_message_push_event_if_enabled(
    executor: &mut Transaction<'_, Postgres>,
    event: &SlackPushEventCallback,
    trace: &SlackEventTrace,
    integration_connection: IntegrationConnection,
    existing_third_party_item: Option<&ThirdPartyItem>,
    notification_service: Arc<RwLock<NotificationService>>,
) -> Result<(UserId, SlackMessageDecision), UniversalInboxError> {
    let current_span = tracing::Span::current();
    trace.record_on(&current_span);
    current_span.set_attribute(
        attr::INTEGRATION_CONNECTION_ID,
        integration_connection.id.to_string(),
    );
    current_span.set_attribute(attr::USER_ID, integration_connection.user_id.to_string());

    let IntegrationProvider::Slack {
        config: slack_config,
        ..
    } = &integration_connection.provider
    else {
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "Integration connection {} provider is supposed to be Slack",
            integration_connection.id
        )));
    };

    let decision = if let SlackConfig {
        message_config:
            SlackMessageConfig {
                sync_enabled: true,
                is_2way_sync,
                ..
            },
        ..
    } = slack_config
    {
        current_span.set_attribute(attr::SLACK_MESSAGE_SYNC_ENABLED, true);
        let outcome = notification_service
            .read()
            .await
            .save_notification_from_event(
                executor,
                event,
                if *is_2way_sync {
                    None
                } else {
                    existing_third_party_item
                },
                integration_connection.user_id,
            )
            .await?;
        SlackMessageDecision::from_outcome(&outcome)
    } else {
        current_span.set_attribute(attr::SLACK_MESSAGE_SYNC_ENABLED, false);
        SlackMessageDecision::SyncDisabled
    };

    current_span.set_attribute(attr::SLACK_MESSAGE_OUTCOME, decision.as_str());
    trace.log_user_decision(
        decision,
        Some(integration_connection.user_id),
        Some(integration_connection.id),
    );
    Ok((integration_connection.user_id, decision))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    const THREAD_UNREAD: Option<SlackThreadState> = Some(SlackThreadState {
        last_message_from_user: false,
        last_message_read: false,
    });
    const THREAD_LAST_FROM_USER: Option<SlackThreadState> = Some(SlackThreadState {
        last_message_from_user: true,
        last_message_read: false,
    });
    const THREAD_READ: Option<SlackThreadState> = Some(SlackThreadState {
        last_message_from_user: false,
        last_message_read: true,
    });

    #[rstest]
    #[case::created(
        NotificationChange::Created,
        NotificationStatus::Unread,
        THREAD_UNREAD,
        "created"
    )]
    #[case::updated(
        NotificationChange::Updated,
        NotificationStatus::Unread,
        THREAD_UNREAD,
        "updated"
    )]
    #[case::untouched_unread(
        NotificationChange::Untouched,
        NotificationStatus::Unread,
        THREAD_UNREAD,
        "unchanged"
    )]
    #[case::untouched_deleted(
        NotificationChange::Untouched,
        NotificationStatus::Deleted,
        THREAD_LAST_FROM_USER,
        "unchanged"
    )]
    #[case::untouched_unsubscribed(
        NotificationChange::Untouched,
        NotificationStatus::Unsubscribed,
        THREAD_UNREAD,
        "unchanged"
    )]
    #[case::unsubscribed(
        NotificationChange::Updated,
        NotificationStatus::Unsubscribed,
        THREAD_UNREAD,
        "unsubscribed"
    )]
    #[case::deleted_last_from_user(
        NotificationChange::Created,
        NotificationStatus::Deleted,
        THREAD_LAST_FROM_USER,
        "deleted_last_from_user"
    )]
    #[case::deleted_already_read(
        NotificationChange::Updated,
        NotificationStatus::Deleted,
        THREAD_READ,
        "deleted_already_read"
    )]
    #[case::deleted_not_a_thread(
        NotificationChange::Updated,
        NotificationStatus::Deleted,
        None,
        "deleted"
    )]
    fn test_decision_from_saved_notification(
        #[case] change: NotificationChange,
        #[case] status: NotificationStatus,
        #[case] slack_thread_state: Option<SlackThreadState>,
        #[case] expected: &str,
    ) {
        assert_eq!(
            SlackMessageDecision::from_saved(change, status, slack_thread_state).as_str(),
            expected
        );
    }
}
