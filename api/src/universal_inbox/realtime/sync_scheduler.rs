//! Sync scheduling of the users with an event stream open on an API pod.
//!
//! The web application used to schedule the syncs of its user by polling the notifications
//! and tasks lists (`trigger_sync`). Once it relies on the event stream instead, each pod
//! schedules the due syncs of its connected users itself. Two pods holding streams of the same
//! user are safe: `schedule_due_syncs` claims with `FOR UPDATE SKIP LOCKED`, so the second
//! claim finds nothing due.

use std::{sync::Arc, time::Duration};

use tokio::{
    sync::RwLock,
    time::{MissedTickBehavior, interval_at},
};
use tracing::{Instrument, info_span, warn};

use universal_inbox::user::UserId;

use crate::{
    jobs::JobStorage,
    observability::attr,
    universal_inbox::{
        audit::actor_context::with_system_actor,
        integration_connection::service::IntegrationConnectionService,
        realtime::{EventPublisher, hub::EventHub, with_event_scope},
    },
};

/// Schedule the due syncs of the users connected to `hub` every `interval` (non-zero),
/// forever. A failure for one user is logged and does not stop the others.
pub async fn run_connected_users_sync_scheduler(
    hub: Arc<EventHub>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    job_storage: JobStorage,
    event_publisher: Arc<dyn EventPublisher>,
    interval: Duration,
) {
    let mut ticks = interval_at(tokio::time::Instant::now() + interval, interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let connected_users = hub.connected_users();
        if connected_users.is_empty() {
            continue;
        }
        let span = info_span!(
            "schedule_connected_users_syncs",
            { attr::REALTIME_CONNECTED_USERS_COUNT } = connected_users.len()
        );
        schedule_users_syncs(
            connected_users,
            &integration_connection_service,
            &job_storage,
            &event_publisher,
        )
        .instrument(span)
        .await;
    }
}

async fn schedule_users_syncs(
    connected_users: Vec<UserId>,
    integration_connection_service: &RwLock<IntegrationConnectionService>,
    job_storage: &JobStorage,
    event_publisher: &Arc<dyn EventPublisher>,
) {
    // One after the other: a pod holds few users, and scheduling is two short statements
    for user_id in connected_users {
        let mut job_storage = job_storage.clone();
        let result = with_event_scope(
            event_publisher.clone(),
            with_system_actor("ScheduleConnectedUsersSyncs", None, async {
                integration_connection_service
                    .read()
                    .await
                    .schedule_due_syncs(user_id, &mut job_storage)
                    .await
            }),
        )
        .await;
        if let Err(err) = result {
            warn!(
                { attr::USER_ID } = %user_id,
                { attr::ERROR_MESSAGE } = %err,
                "Failed to schedule the syncs of a connected user"
            );
        }
    }
}
