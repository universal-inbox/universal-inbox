//! Realtime updates: listen to the server-sent event stream of the API and refetch the lists
//! it says have changed. Polling (`layouts::authenticated`) slows down while it is connected
//! and takes over when it is not.

use dioxus::prelude::*;
use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::{
    FutureExt, StreamExt,
    future::{Fuse, FusedFuture},
    select,
};
use gloo_timers::future::TimeoutFuture;
use log::{debug, warn};
use url::Url;
use wasm_bindgen::{JsCast, prelude::Closure};
use web_sys::{Event, EventSource};

use universal_inbox::realtime::{RESYNC_EVENT, RealtimeEventKind};

use crate::{
    model::UniversalInboxUIModel,
    services::{
        integration_connection_service::IntegrationConnectionCommand,
        notification_service::NotificationCommand, task_service::TaskCommand,
    },
};

/// Events of a burst (eg. a sync) collected before refetching each changed list once.
const DEBOUNCE_DELAY_MS: u32 = 300;
/// Retry delays of a stream refused by the server (logged out, disabled, too many streams).
const MIN_RECONNECT_DELAY_MS: u32 = 30_000;
const MAX_RECONNECT_DELAY_MS: u32 = 300_000;
/// A tab hidden that long closes its stream; it reopens, and refetches, once visible again.
const HIDDEN_TAB_DISCONNECT_DELAY_MS: u32 = 180_000;
/// Attribute of the document body telling whether the stream is open (used by the tests).
const CONNECTED_ATTRIBUTE: &str = "data-realtime-connected";

/// The event kinds the web application listens to. `user.changed` has no list to refetch yet.
const LISTENED_EVENT_KINDS: [RealtimeEventKind; 3] = [
    RealtimeEventKind::NotificationsChanged,
    RealtimeEventKind::TasksChanged,
    RealtimeEventKind::IntegrationConnectionChanged,
];

#[derive(Debug, PartialEq)]
pub enum RealtimeCommand {
    /// Listen to the stream of the authenticated user
    Connect,
    /// Stop listening, eg. on logout
    Disconnect,
}

/// A list to refetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refresh {
    Notifications,
    Tasks,
    IntegrationConnections,
}

/// Every list: refetched when events may have been lost (stream opened, `resync`).
const ALL_REFRESHES: [Refresh; 3] = [
    Refresh::Notifications,
    Refresh::Tasks,
    Refresh::IntegrationConnections,
];

/// The lists to refetch on an event of `kind`.
fn refreshes_for(kind: RealtimeEventKind) -> &'static [Refresh] {
    match kind {
        RealtimeEventKind::NotificationsChanged => &[Refresh::Notifications],
        RealtimeEventKind::TasksChanged => &[Refresh::Tasks],
        RealtimeEventKind::IntegrationConnectionChanged => &[Refresh::IntegrationConnections],
        RealtimeEventKind::UserChanged => &[],
        // For the browser extension only
        RealtimeEventKind::SlackBridgePendingActionsChanged => &[],
    }
}

/// What an event source, tagged with the generation of the connection that opened it, and the
/// page tell the service.
enum StreamSignal {
    Opened(u64),
    Refreshes(u64, &'static [Refresh]),
    Errored { generation: u64, closed: bool },
    VisibilityChanged { hidden: bool },
}

/// An open event source, closed when dropped.
struct Connection {
    source: EventSource,
    _listeners: Vec<Closure<dyn FnMut(Event)>>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.source.close();
    }
}

fn open_connection(
    url: &Url,
    generation: u64,
    signals: &UnboundedSender<StreamSignal>,
) -> Option<Connection> {
    let source = match EventSource::new(url.as_str()) {
        Ok(source) => source,
        Err(err) => {
            warn!("Failed to open the realtime event stream: {err:?}");
            return None;
        }
    };
    let mut listeners: Vec<Closure<dyn FnMut(Event)>> = vec![];

    let sender = signals.clone();
    let on_open = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = sender.unbounded_send(StreamSignal::Opened(generation));
    });
    source.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    listeners.push(on_open);

    let sender = signals.clone();
    let errored_source = source.clone();
    let on_error = Closure::<dyn FnMut(Event)>::new(move |_| {
        // `CONNECTING`: the browser retries by itself. `CLOSED`: the server refused the stream.
        let closed = errored_source.ready_state() == EventSource::CLOSED;
        let _ = sender.unbounded_send(StreamSignal::Errored { generation, closed });
    });
    source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    listeners.push(on_error);

    let events = LISTENED_EVENT_KINDS
        .iter()
        .map(|kind| (kind.to_string(), refreshes_for(*kind)))
        .chain([(RESYNC_EVENT.to_string(), &ALL_REFRESHES[..])]);
    for (name, refreshes) in events {
        let sender = signals.clone();
        let on_event = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = sender.unbounded_send(StreamSignal::Refreshes(generation, refreshes));
        });
        if let Err(err) =
            source.add_event_listener_with_callback(&name, on_event.as_ref().unchecked_ref())
        {
            warn!("Failed to listen to {name} realtime events: {err:?}");
        }
        listeners.push(on_event);
    }

    Some(Connection {
        source,
        _listeners: listeners,
    })
}

fn document() -> Option<web_sys::Document> {
    web_sys::window().and_then(|window| window.document())
}

/// Forward the visibility changes of the page, for the service lifetime.
fn listen_to_visibility(signals: &UnboundedSender<StreamSignal>) {
    let Some(document) = document() else {
        return;
    };
    let sender = signals.clone();
    let listened_document = document.clone();
    let on_visibility_change = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = sender.unbounded_send(StreamSignal::VisibilityChanged {
            hidden: listened_document.hidden(),
        });
    });
    if let Err(err) = document.add_event_listener_with_callback(
        "visibilitychange",
        on_visibility_change.as_ref().unchecked_ref(),
    ) {
        warn!("Failed to listen to the page visibility: {err:?}");
    }
    // The service lives as long as the application
    on_visibility_change.forget();
}

/// Whether the tab is hidden, eg. opened in the background.
pub fn is_page_hidden() -> bool {
    document().is_some_and(|document| document.hidden())
}

fn timer(delay_ms: u32) -> Fuse<TimeoutFuture> {
    TimeoutFuture::new(delay_ms).fuse()
}

struct RealtimeState {
    url: Url,
    signals: UnboundedSender<StreamSignal>,
    ui_model: Signal<UniversalInboxUIModel>,
    /// Whether the user is authenticated and the stream should be open
    wanted: bool,
    /// Whether the stream was closed because the tab stayed hidden
    suspended: bool,
    connection: Option<Connection>,
    /// Generation of the current connection: signals of a previous one are stale
    generation: u64,
    pending_refreshes: Vec<Refresh>,
    reconnect_delay_ms: u32,
    debounce_timer: Fuse<TimeoutFuture>,
    reconnect_timer: Fuse<TimeoutFuture>,
    hidden_timer: Fuse<TimeoutFuture>,
}

impl RealtimeState {
    fn connect_if_wanted(&mut self) {
        if self.wanted && !self.suspended && self.connection.is_none() {
            self.generation += 1;
            self.connection = open_connection(&self.url, self.generation, &self.signals);
        }
    }

    fn disconnect(&mut self) {
        self.connection = None;
        self.reconnect_timer = Fuse::terminated();
        self.set_connected(false);
    }

    fn set_connected(&mut self, connected: bool) {
        if self.ui_model.peek().is_realtime_connected != connected {
            self.ui_model.write().is_realtime_connected = connected;
        }
        if let Some(body) = document().and_then(|document| document.body()) {
            let _ = body.set_attribute(CONNECTED_ATTRIBUTE, &connected.to_string());
        }
    }

    fn add_refreshes(&mut self, refreshes: &[Refresh]) {
        for refresh in refreshes {
            if !self.pending_refreshes.contains(refresh) {
                self.pending_refreshes.push(*refresh);
            }
        }
        if !self.pending_refreshes.is_empty() && self.debounce_timer.is_terminated() {
            self.debounce_timer = timer(DEBOUNCE_DELAY_MS);
        }
    }

    fn handle_command(&mut self, command: RealtimeCommand) {
        match command {
            RealtimeCommand::Connect => {
                self.wanted = true;
                self.connect_if_wanted();
            }
            RealtimeCommand::Disconnect => {
                self.wanted = false;
                self.disconnect();
            }
        }
    }

    /// Handle `signal`, returning a list to refetch at once.
    fn handle_signal(&mut self, signal: StreamSignal) -> Option<Refresh> {
        match signal {
            StreamSignal::Opened(generation) if generation == self.generation => {
                debug!("Realtime event stream opened");
                self.set_connected(true);
                self.reconnect_delay_ms = MIN_RECONNECT_DELAY_MS;
                // Catch up with what changed while the stream was not open
                self.add_refreshes(&ALL_REFRESHES);
            }
            StreamSignal::Refreshes(generation, refreshes) if generation == self.generation => {
                self.add_refreshes(refreshes);
            }
            StreamSignal::Errored { generation, closed } if generation == self.generation => {
                self.set_connected(false);
                if closed {
                    debug!(
                        "Realtime event stream refused, retrying in {}ms",
                        self.reconnect_delay_ms
                    );
                    self.connection = None;
                    self.reconnect_timer = timer(self.reconnect_delay_ms);
                    self.reconnect_delay_ms =
                        (self.reconnect_delay_ms * 2).min(MAX_RECONNECT_DELAY_MS);
                    // The event source cannot tell why: a regular API call does, and logs the
                    // user out right away if the session expired (`api::call_api`)
                    return Some(Refresh::IntegrationConnections);
                }
            }
            StreamSignal::VisibilityChanged { hidden: true } => {
                self.hidden_timer = timer(HIDDEN_TAB_DISCONNECT_DELAY_MS);
            }
            StreamSignal::VisibilityChanged { hidden: false } => {
                self.hidden_timer = Fuse::terminated();
                if self.suspended {
                    self.suspended = false;
                    self.reconnect_delay_ms = MIN_RECONNECT_DELAY_MS;
                    // Its opening refetches the lists
                    self.connect_if_wanted();
                } else if self.wanted && !self.ui_model.peek().is_realtime_connected {
                    // Polling skipped the hidden tab and no event refreshed it
                    self.add_refreshes(&ALL_REFRESHES);
                }
            }
            // A signal of a previous connection
            StreamSignal::Opened(_)
            | StreamSignal::Refreshes(..)
            | StreamSignal::Errored { .. } => {}
        }
        None
    }

    fn suspend(&mut self) {
        debug!("Tab hidden, closing the realtime event stream");
        self.suspended = true;
        self.disconnect();
    }
}

pub async fn realtime_service(
    mut rx: UnboundedReceiver<RealtimeCommand>,
    api_base_url: Url,
    ui_model: Signal<UniversalInboxUIModel>,
    notification_service: Coroutine<NotificationCommand>,
    task_service: Coroutine<TaskCommand>,
    integration_connection_service: Coroutine<IntegrationConnectionCommand>,
) {
    let Ok(url) = api_base_url.join("events") else {
        warn!("Invalid realtime event stream URL");
        return;
    };
    let (signals, mut signals_rx) = unbounded::<StreamSignal>();
    listen_to_visibility(&signals);

    let send_refresh = |refresh: Refresh| match refresh {
        Refresh::Notifications => notification_service.send(NotificationCommand::Refresh),
        Refresh::Tasks => task_service.send(TaskCommand::RefreshSyncedTasks),
        Refresh::IntegrationConnections => {
            integration_connection_service.send(IntegrationConnectionCommand::Refresh)
        }
    };

    let mut state = RealtimeState {
        url,
        signals,
        ui_model,
        wanted: false,
        suspended: false,
        connection: None,
        generation: 0,
        pending_refreshes: vec![],
        reconnect_delay_ms: MIN_RECONNECT_DELAY_MS,
        debounce_timer: Fuse::terminated(),
        reconnect_timer: Fuse::terminated(),
        // A tab opened in the background gets no `visibilitychange` until it is shown
        hidden_timer: if is_page_hidden() {
            timer(HIDDEN_TAB_DISCONNECT_DELAY_MS)
        } else {
            Fuse::terminated()
        },
    };

    loop {
        select! {
            command = rx.next() => match command {
                Some(command) => state.handle_command(command),
                None => break,
            },
            signal = signals_rx.next() => match signal {
                Some(signal) => {
                    if let Some(refresh) = state.handle_signal(signal) {
                        send_refresh(refresh);
                    }
                }
                None => break,
            },
            _ = &mut state.debounce_timer => {
                for refresh in std::mem::take(&mut state.pending_refreshes) {
                    send_refresh(refresh);
                }
            },
            _ = &mut state.reconnect_timer => state.connect_if_wanted(),
            _ = &mut state.hidden_timer => state.suspend(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_refreshes_for_event_kinds() {
        assert_eq!(
            refreshes_for(RealtimeEventKind::NotificationsChanged),
            &[Refresh::Notifications]
        );
        assert_eq!(
            refreshes_for(RealtimeEventKind::TasksChanged),
            &[Refresh::Tasks]
        );
        assert_eq!(
            refreshes_for(RealtimeEventKind::IntegrationConnectionChanged),
            &[Refresh::IntegrationConnections]
        );
        assert_eq!(
            refreshes_for(RealtimeEventKind::UserChanged),
            &[] as &[Refresh]
        );
        assert_eq!(
            refreshes_for(RealtimeEventKind::SlackBridgePendingActionsChanged),
            &[] as &[Refresh]
        );
    }
}
