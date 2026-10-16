//! Task-local buffer of the realtime events recorded by an entry point.

use std::{
    collections::BTreeSet,
    future::Future,
    mem,
    sync::{Arc, Mutex},
};

use tracing::{debug, trace, warn};
use uuid::Uuid;

use universal_inbox::{
    realtime::{RealtimeEvent, RealtimeEventKind},
    user::UserId,
};

use super::EventPublisher;

/// Above this many changed items, an event carries no ids: the client refetches everything.
pub const MAX_EVENT_IDS: usize = 50;

tokio::task_local! {
    static EVENT_SCOPE: Arc<EventScope>;
}

struct EventScope {
    publisher: Arc<dyn EventPublisher>,
    pending: Mutex<PendingEvents>,
}

/// Recorded events, one per user and kind.
#[derive(Default)]
struct PendingEvents(Vec<PendingEvent>);

struct PendingEvent {
    user_id: UserId,
    kind: RealtimeEventKind,
    /// `None` once a change without id was recorded or there are too many ids
    ids: Option<BTreeSet<Uuid>>,
}

impl PendingEvents {
    fn record(&mut self, user_id: UserId, kind: RealtimeEventKind, id: Option<Uuid>) {
        let pending = match self
            .0
            .iter_mut()
            .find(|pending| pending.user_id == user_id && pending.kind == kind)
        {
            Some(pending) => pending,
            None => {
                self.0.push(PendingEvent {
                    user_id,
                    kind,
                    ids: Some(BTreeSet::new()),
                });
                self.0.last_mut().unwrap()
            }
        };

        match (id, &mut pending.ids) {
            (Some(id), Some(ids)) => {
                ids.insert(id);
                if ids.len() > MAX_EVENT_IDS {
                    pending.ids = None;
                }
            }
            (None, _) => pending.ids = None,
            (Some(_), None) => {}
        }
    }

    fn take(&mut self) -> Vec<RealtimeEvent> {
        mem::take(&mut self.0)
            .into_iter()
            .map(|pending| RealtimeEvent {
                user_id: pending.user_id,
                kind: pending.kind,
                ids: pending
                    .ids
                    .filter(|ids| !ids.is_empty())
                    .map(|ids| ids.into_iter().collect()),
            })
            .collect()
    }
}

impl EventScope {
    async fn flush(&self) {
        let events = self.pending.lock().unwrap().take();
        self.publisher.publish(events).await;
    }

    fn discard(&self) {
        let discarded = self.pending.lock().unwrap().take();
        if !discarded.is_empty() {
            debug!(
                "Entry point failed, discarding {} realtime events",
                discarded.len()
            );
        }
    }
}

/// Publish the pending events of a scope whose future was dropped before completing (client
/// disconnect, job timeout, panic): the changes it committed must still reach the web app. A
/// change that was rolled back with it only costs the client a refetch.
struct FlushOnDrop {
    scope: Arc<EventScope>,
    completed: bool,
}

impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        let events = self.scope.pending.lock().unwrap().take();
        if events.is_empty() {
            return;
        }
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let publisher = self.scope.publisher.clone();
                handle.spawn(async move { publisher.publish(events).await });
            }
            Err(_) => warn!(
                "No Tokio runtime to publish {} realtime events of a dropped scope",
                events.len()
            ),
        }
    }
}

/// Run `future` in a realtime event scope: the events it records are published with
/// `publisher` once it completes with `Ok`, and discarded when it fails, as its uncommitted
/// changes were rolled back. A nested call joins the outer scope, which publishes them.
pub async fn with_event_scope<F, T, E>(
    publisher: Arc<dyn EventPublisher>,
    future: F,
) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    with_event_scope_if(publisher, future, Result::is_ok).await
}

/// Like [`with_event_scope`], with `succeeded` telling whether the output of `future` is a
/// success, eg. an HTTP response with a successful status.
pub async fn with_event_scope_if<F>(
    publisher: Arc<dyn EventPublisher>,
    future: F,
    succeeded: impl FnOnce(&F::Output) -> bool,
) -> F::Output
where
    F: Future,
{
    if EVENT_SCOPE.try_with(|_| ()).is_ok() {
        return future.await;
    }

    let scope = Arc::new(EventScope {
        publisher,
        pending: Mutex::new(PendingEvents::default()),
    });
    let mut guard = FlushOnDrop {
        scope: scope.clone(),
        completed: false,
    };
    let output = EVENT_SCOPE.scope(scope.clone(), future).await;
    guard.completed = true;
    if succeeded(&output) {
        scope.flush().await;
    } else {
        scope.discard();
    }
    output
}

/// Record that items of `kind` changed for `user_id`, to be published when the current scope
/// completes or calls [`flush_events`]. Call it before committing: the event is only published
/// once the entry point is done. `id` is the changed item, `None` when unknown.
pub fn record_event(user_id: UserId, kind: RealtimeEventKind, id: Option<Uuid>) {
    if EVENT_SCOPE
        .try_with(|scope| scope.pending.lock().unwrap().record(user_id, kind, id))
        .is_err()
    {
        trace!("No realtime event scope, dropping {kind} event for user {user_id}");
    }
}

/// Publish the events recorded so far in the current scope. Call it right after a commit in a
/// long-running entry point, so clients see the change without waiting for it to complete.
///
/// Precondition: no other transaction of the entry point is still open, as its changes would
/// be published before they are committed.
pub async fn flush_events() {
    if let Ok(scope) = EVENT_SCOPE.try_with(Arc::clone) {
        scope.flush().await;
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{super::InMemoryEventPublisher, *};

    /// Run `future` in a scope that succeeds.
    async fn in_scope(publisher: Arc<dyn EventPublisher>, future: impl Future<Output = ()>) {
        with_event_scope(publisher, async {
            future.await;
            Ok::<(), ()>(())
        })
        .await
        .unwrap();
    }

    fn user(n: u128) -> UserId {
        UserId(Uuid::from_u128(n))
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn event(user_id: UserId, kind: RealtimeEventKind, ids: Option<Vec<Uuid>>) -> RealtimeEvent {
        RealtimeEvent { user_id, kind, ids }
    }

    #[tokio::test]
    async fn test_events_are_batched_per_user_and_kind() {
        let publisher = Arc::new(InMemoryEventPublisher::default());

        in_scope(publisher.clone(), async {
            record_event(
                user(1),
                RealtimeEventKind::NotificationsChanged,
                Some(id(2)),
            );
            record_event(
                user(1),
                RealtimeEventKind::NotificationsChanged,
                Some(id(1)),
            );
            record_event(
                user(1),
                RealtimeEventKind::NotificationsChanged,
                Some(id(2)),
            );
            record_event(
                user(2),
                RealtimeEventKind::NotificationsChanged,
                Some(id(3)),
            );
            record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(4)));
            record_event(user(1), RealtimeEventKind::UserChanged, None);
        })
        .await;

        assert_eq!(
            publisher.published(),
            vec![vec![
                event(
                    user(1),
                    RealtimeEventKind::NotificationsChanged,
                    Some(vec![id(1), id(2)])
                ),
                event(
                    user(2),
                    RealtimeEventKind::NotificationsChanged,
                    Some(vec![id(3)])
                ),
                event(user(1), RealtimeEventKind::TasksChanged, Some(vec![id(4)])),
                event(user(1), RealtimeEventKind::UserChanged, None),
            ]]
        );
    }

    #[tokio::test]
    async fn test_event_ids_are_dropped_when_unknown_or_too_many() {
        let publisher = Arc::new(InMemoryEventPublisher::default());

        in_scope(publisher.clone(), async {
            record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(1)));
            record_event(user(1), RealtimeEventKind::TasksChanged, None);
            record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(2)));
            for n in 0..=MAX_EVENT_IDS as u128 {
                record_event(user(2), RealtimeEventKind::TasksChanged, Some(id(n)));
            }
        })
        .await;

        assert_eq!(
            publisher.published(),
            vec![vec![
                event(user(1), RealtimeEventKind::TasksChanged, None),
                event(user(2), RealtimeEventKind::TasksChanged, None),
            ]]
        );
    }

    #[tokio::test]
    async fn test_events_are_published_when_the_scope_completes() {
        let publisher = Arc::new(InMemoryEventPublisher::default());

        in_scope(publisher.clone(), async {
            record_event(user(1), RealtimeEventKind::NotificationsChanged, None);
            tokio::task::yield_now().await;
            assert_eq!(publisher.published(), Vec::<Vec<RealtimeEvent>>::new());
        })
        .await;

        assert_eq!(
            publisher.published(),
            vec![vec![event(
                user(1),
                RealtimeEventKind::NotificationsChanged,
                None
            )]]
        );
    }

    #[tokio::test]
    async fn test_nested_scope_publishes_once_with_the_outer_scope() {
        let outer = Arc::new(InMemoryEventPublisher::default());
        let inner = Arc::new(InMemoryEventPublisher::default());

        in_scope(outer.clone(), async {
            record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(1)));
            in_scope(inner.clone(), async {
                record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(2)));
            })
            .await;
            assert_eq!(outer.published(), Vec::<Vec<RealtimeEvent>>::new());
        })
        .await;

        assert_eq!(inner.published(), Vec::<Vec<RealtimeEvent>>::new());
        assert_eq!(
            outer.published(),
            vec![vec![event(
                user(1),
                RealtimeEventKind::TasksChanged,
                Some(vec![id(1), id(2)])
            )]]
        );
    }

    #[tokio::test]
    async fn test_flush_events_publishes_the_events_recorded_so_far() {
        let publisher = Arc::new(InMemoryEventPublisher::default());

        in_scope(publisher.clone(), async {
            record_event(
                user(1),
                RealtimeEventKind::IntegrationConnectionChanged,
                Some(id(1)),
            );
            flush_events().await;
            record_event(
                user(1),
                RealtimeEventKind::IntegrationConnectionChanged,
                Some(id(1)),
            );
        })
        .await;

        let expected = vec![event(
            user(1),
            RealtimeEventKind::IntegrationConnectionChanged,
            Some(vec![id(1)]),
        )];
        assert_eq!(publisher.published(), vec![expected.clone(), expected]);
    }

    #[tokio::test]
    async fn test_events_recorded_without_scope_are_dropped() {
        record_event(user(1), RealtimeEventKind::TasksChanged, None);
        flush_events().await;

        let publisher = Arc::new(InMemoryEventPublisher::default());
        in_scope(publisher.clone(), async {}).await;

        assert_eq!(publisher.published(), Vec::<Vec<RealtimeEvent>>::new());
    }

    #[tokio::test]
    async fn test_events_of_a_failed_scope_are_discarded() {
        let publisher = Arc::new(InMemoryEventPublisher::default());

        let result = with_event_scope(publisher.clone(), async {
            record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(1)));
            flush_events().await;
            record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(2)));
            Err::<(), _>("rolled back")
        })
        .await;

        assert_eq!(result, Err("rolled back"));
        // Only the events flushed after a commit are published
        assert_eq!(
            publisher.published(),
            vec![vec![event(
                user(1),
                RealtimeEventKind::TasksChanged,
                Some(vec![id(1)])
            )]]
        );
    }

    #[tokio::test]
    async fn test_events_of_a_dropped_scope_are_published() {
        let publisher = Arc::new(InMemoryEventPublisher::default());
        let (recorded_sender, recorded_receiver) = tokio::sync::oneshot::channel();

        let scope = tokio::spawn(with_event_scope(publisher.clone(), async move {
            record_event(user(1), RealtimeEventKind::TasksChanged, Some(id(1)));
            recorded_sender.send(()).unwrap();
            std::future::pending::<Result<(), ()>>().await
        }));
        recorded_receiver.await.unwrap();
        scope.abort();
        assert!(scope.await.unwrap_err().is_cancelled());
        // The drop guard publishes on a spawned task
        for _ in 0..10 {
            if !publisher.published().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }

        assert_eq!(
            publisher.published(),
            vec![vec![event(
                user(1),
                RealtimeEventKind::TasksChanged,
                Some(vec![id(1)])
            )]]
        );
    }
}
