//! The message sinks of the open WebSocket connections.
//!
//! Specification: `specifications/server/WSK-websocket.md`.
//! Requirements: WSK-FR-SYNB, WSK-FR-WKAI, WSK-FR-GZDU.
//!
//! The hub holds one sink per open socket and nothing else. Every routing
//! decision — which worker a frame reaches, which clients a broadcast reaches,
//! whether a handle may be used — is read from the relay port of
//! `SRB-server-relay-boundary.md` SRB-FR-DYAE, so the transport holds no
//! routing state of its own.
//!
//! A sink is dropped with the socket it names, so a frame for a connection that
//! has closed is not buffered and not queued (WSK-FR-GZDU).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::mpsc::Sender;

/// One outbound frame, as the writer task takes it.
#[derive(Debug, Clone)]
pub enum Outbound {
    /// One JSON text frame.
    Text(String),
    /// The connection closes with this reason after the frames already queued.
    Close(&'static str),
    /// The connection closes normally, because the exchange it carried is over
    /// and no reason of WSK-FR-BWZT applies.
    Finish,
}

/// The sink of one socket.
///
/// Bounded: a connection that is open but is not reading holds at most the
/// channel's capacity, so one slow client cannot make the relay grow a queue
/// for it (WSK-FR-GZDU).
pub type Sink = Sender<Outbound>;

/// Puts one frame on a sink, and reports whether the sink took it.
///
/// WSK-FR-RCUY, RSN-FR-HZTV: a sink that is closed, or whose queue is full
/// because the peer has stopped reading, did **not** take the frame. The
/// caller answers its sender rather than reporting a delivery that did not
/// happen, and the stalled connection is closed by the write bound of the
/// transport.
#[must_use]
pub fn deliver(sink: &Sink, text: String) -> bool {
    sink.try_send(Outbound::Text(text)).is_ok()
}

/// The sink of one worker connection.
#[derive(Debug, Clone)]
struct WorkerSink {
    connection_id: String,
    sink: Sink,
}

/// The sink of one client socket.
#[derive(Debug, Clone)]
struct ClientSink {
    handle: String,
    sink: Sink,
}

/// The sink of one registration socket.
#[derive(Debug, Clone)]
struct RegistrationSink {
    registration_id: String,
    handle: Option<String>,
    sink: Sink,
}

#[derive(Debug, Default)]
struct HubState {
    workers: HashMap<String, WorkerSink>,
    clients: HashMap<u64, ClientSink>,
    registrations: HashMap<u64, RegistrationSink>,
}

/// Every open socket of the three endpoints.
#[derive(Debug, Default)]
pub struct Hub {
    state: Mutex<HubState>,
    next_socket: AtomicU64,
}

impl Hub {
    /// A hub that holds no socket.
    pub fn new() -> Self {
        Hub::default()
    }

    /// The identifier of one socket, which names the sink and nothing else.
    pub fn next_socket_id(&self) -> u64 {
        self.next_socket.fetch_add(1, Ordering::Relaxed)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HubState> {
        self.state.lock().expect("the hub is not poisoned")
    }

    /// Records the sink of a worker connection, and reports the sink of the
    /// connection it replaced (WSK-FR-WKAI).
    pub fn put_worker(&self, instance_id: &str, connection_id: &str, sink: Sink) -> Option<Sink> {
        let replaced = self.lock().workers.insert(
            instance_id.to_string(),
            WorkerSink {
                connection_id: connection_id.to_string(),
                sink,
            },
        );
        replaced.map(|previous| previous.sink)
    }

    /// The sink of the worker that holds the route now.
    pub fn worker(&self, instance_id: &str) -> Option<Sink> {
        self.lock()
            .workers
            .get(instance_id)
            .map(|worker| worker.sink.clone())
    }

    /// Drops the sink of a worker connection, when that connection still holds
    /// the route.
    pub fn drop_worker(&self, instance_id: &str, connection_id: &str) {
        let mut state = self.lock();
        let holds_route = state
            .workers
            .get(instance_id)
            .is_some_and(|worker| worker.connection_id == connection_id);
        if holds_route {
            state.workers.remove(instance_id);
        }
    }

    /// Records the sink of a client socket.
    pub fn put_client(&self, socket_id: u64, handle: &str, sink: Sink) {
        self.lock().clients.insert(
            socket_id,
            ClientSink {
                handle: handle.to_string(),
                sink,
            },
        );
    }

    /// Drops the sink of a client socket.
    pub fn drop_client(&self, socket_id: u64) {
        self.lock().clients.remove(&socket_id);
    }

    /// The sinks of every socket that holds one handle (RSN-FR-JAVE).
    pub fn clients_of(&self, handle: &str) -> Vec<Sink> {
        self.lock()
            .clients
            .values()
            .filter(|client| client.handle == handle)
            .map(|client| client.sink.clone())
            .collect()
    }

    /// The sinks of every socket that holds one of these handles
    /// (WSK-FR-IKVE).
    pub fn clients_of_all(&self, handles: &[String]) -> Vec<Sink> {
        let state = self.lock();
        state
            .clients
            .values()
            .filter(|client| handles.iter().any(|handle| handle == &client.handle))
            .map(|client| client.sink.clone())
            .collect()
    }

    /// Records the sink of a registration socket, before a handle is assigned.
    ///
    /// Reports `false` when another open registration already holds that
    /// identifier. A registration identifier is routing state, so two live
    /// exchanges that share one would each be reachable by the other's answers
    /// (RSN-FR-NGKW).
    #[must_use]
    pub fn put_registration(&self, socket_id: u64, registration_id: &str, sink: Sink) -> bool {
        let mut state = self.lock();
        let taken = state
            .registrations
            .values()
            .any(|registration| registration.registration_id == registration_id);
        if taken {
            return false;
        }
        state.registrations.insert(
            socket_id,
            RegistrationSink {
                registration_id: registration_id.to_string(),
                handle: None,
                sink,
            },
        );
        true
    }

    /// Records the handle the relay assigned to a registration socket.
    pub fn bind_registration(&self, socket_id: u64, handle: &str) {
        if let Some(registration) = self.lock().registrations.get_mut(&socket_id) {
            registration.handle = Some(handle.to_string());
        }
    }

    /// Drops the sink of a registration socket.
    pub fn drop_registration(&self, socket_id: u64) {
        self.lock().registrations.remove(&socket_id);
    }

    /// The sink of the registration socket one registration belongs to.
    pub fn registration_of(&self, registration_id: &str) -> Option<Sink> {
        self.lock()
            .registrations
            .values()
            .find(|registration| registration.registration_id == registration_id)
            .map(|registration| registration.sink.clone())
    }

    /// The sink of the registration socket one handle belongs to
    /// (WSK-FR-FXTA).
    pub fn registration_of_handle(&self, handle: &str) -> Option<Sink> {
        self.lock()
            .registrations
            .values()
            .find(|registration| registration.handle.as_deref() == Some(handle))
            .map(|registration| registration.sink.clone())
    }

    /// The handle one registration was bound to.
    pub fn registration_handle(&self, registration_id: &str) -> Option<String> {
        self.lock()
            .registrations
            .values()
            .find(|registration| registration.registration_id == registration_id)
            .and_then(|registration| registration.handle.clone())
    }

    /// Closes every open socket with one reason (RSN-FR-CLGY).
    ///
    /// Reports how many sockets were told to close, so a caller — and a test —
    /// sees that a shutdown reached the connections it was meant to.
    pub fn close_all(&self, reason: &'static str) -> usize {
        let state = self.lock();
        let mut closed = 0;
        for worker in state.workers.values() {
            let _ = worker.sink.try_send(Outbound::Close(reason));
            closed += 1;
        }
        for client in state.clients.values() {
            let _ = client.sink.try_send(Outbound::Close(reason));
            closed += 1;
        }
        for registration in state.registrations.values() {
            let _ = registration.sink.try_send(Outbound::Close(reason));
            closed += 1;
        }
        closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::channel;

    fn sink() -> (Sink, tokio::sync::mpsc::Receiver<Outbound>) {
        channel(16)
    }

    // WSK-FR-WKAI: a second worker connection takes the route, and the sink of
    // the connection it replaced is reported so the transport can close it.
    #[test]
    fn a_replacement_reports_the_sink_it_replaced() {
        let hub = Hub::new();
        let (first, _first_rx) = sink();
        let (second, _second_rx) = sink();

        assert!(hub.put_worker("w-1", "c-1", first).is_none());
        assert!(hub.put_worker("w-1", "c-2", second).is_some());
        assert!(hub.worker("w-1").is_some());
    }

    // WSK-FR-WKAI: a connection that no longer holds the route drops nothing,
    // so a replaced worker closing later does not remove the newer route.
    #[test]
    fn a_replaced_connection_drops_no_route() {
        let hub = Hub::new();
        let (first, _first_rx) = sink();
        let (second, _second_rx) = sink();
        hub.put_worker("w-1", "c-1", first);
        hub.put_worker("w-1", "c-2", second);

        hub.drop_worker("w-1", "c-1");
        assert!(hub.worker("w-1").is_some());

        hub.drop_worker("w-1", "c-2");
        assert!(hub.worker("w-1").is_none());
    }

    // RSN-FR-JAVE: several sockets may hold one handle, and a broadcast reaches
    // every one of them.
    #[test]
    fn every_socket_of_one_handle_is_reached() {
        let hub = Hub::new();
        let (one, _one_rx) = sink();
        let (two, _two_rx) = sink();
        let (other, _other_rx) = sink();
        hub.put_client(1, "h-1", one);
        hub.put_client(2, "h-1", two);
        hub.put_client(3, "h-2", other);

        assert_eq!(hub.clients_of("h-1").len(), 2);
        assert_eq!(hub.clients_of_all(&["h-1".to_string()]).len(), 2);
        assert_eq!(
            hub.clients_of_all(&["h-1".to_string(), "h-2".to_string()])
                .len(),
            3
        );

        hub.drop_client(1);
        assert_eq!(hub.clients_of("h-1").len(), 1);
    }

    // WSK-FR-GZDU: a registration socket is reachable by its registration
    // identifier while it is open, and by nothing after it closes.
    #[test]
    fn a_registration_socket_is_reached_by_its_registration_identifier() {
        let hub = Hub::new();
        let (one, _one_rx) = sink();
        assert!(hub.put_registration(7, "reg-1", one.clone()));
        // RSN-FR-NGKW: a second open registration may not take an identifier a
        // live one already holds.
        assert!(!hub.put_registration(8, "reg-1", one));
        assert!(hub.registration_of("reg-1").is_some());
        assert_eq!(hub.registration_handle("reg-1"), None);

        hub.bind_registration(7, "h-9");
        assert_eq!(hub.registration_handle("reg-1"), Some("h-9".to_string()));

        hub.drop_registration(7);
        assert!(hub.registration_of("reg-1").is_none());
    }
}
