//! The relay routing state and its inbound port.
//!
//! Specification: `specifications/server/SRB-server-relay-boundary.md`.
//! Requirements: SRB-FR-DYAE, SRB-FR-YCFA, SRB-FR-DKPM, SRB-FR-XSAG,
//! SRB-FR-MTHQ, SRB-FR-CBWU, SRB-FR-TOQF.
//!
//! The handle lifecycle the client operations apply is
//! `specifications/server/RSN-remote-session.md` RSN-FR-FKRE.
//!
//! The WebSocket adapter of `WSK-websocket.md` holds no routing state of its
//! own: it drives this port, and the port owns the routes.

use serde::Serialize;

use crate::adapters::relay::session::{HandleState, InvalidReason, ReasonCode};
use crate::domain::clock::Timestamp;
use crate::domain::error::DomainError;

/// One project a worker exposes (SRB-FR-EASV).
///
/// The relay routes by `project_id` alone. The display name is presentation
/// only and is never matched or compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExposedProject {
    pub project_id: String,
    pub display_name: String,
}

/// One active worker connection (SRB-FR-DKPM).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkerRoute {
    /// The identifier the IDE generated in memory when it started.
    pub instance_id: String,
    /// The connection that holds the route now.
    pub connection_id: String,
    pub connected_at: Timestamp,
    pub projects: Vec<ExposedProject>,
    /// The selected project of this worker, which no other worker observes
    /// (SRB-FR-OPBZ).
    pub selected_project_id: Option<String>,
    /// How many client connections are attached to the route.
    pub attached_clients: usize,
}

/// What a worker registration did (SRB-FR-LGQB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerRegistration {
    /// The identifier held no route.
    Fresh(WorkerRoute),
    /// The identifier held a route, and this connection replaced it.
    Replaced {
        route: WorkerRoute,
        previous_connection_id: String,
        /// The handles the replacement detached, which the transport notifies
        /// (`RSN-remote-session.md` RSN-FR-SVMD).
        detached_handles: Vec<String>,
    },
}

impl WorkerRegistration {
    /// The route the registration holds, whichever outcome it is.
    pub fn route(&self) -> &WorkerRoute {
        match self {
            WorkerRegistration::Fresh(route) => route,
            WorkerRegistration::Replaced { route, .. } => route,
        }
    }
}

/// The notice an older worker connection reads after it was replaced
/// (SRB-FR-LGQB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplacementNotice {
    pub instance_id: String,
    pub replaced_connection_id: String,
    pub replaced_at: Timestamp,
}

/// One client connection (SRB-FR-XSAG, SRB-FR-MTHQ).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientConnection {
    /// The opaque handle the relay assigned. It names the connection and
    /// nothing else, and it is never assigned again after a reset.
    pub handle: String,
    /// The identifier of the connection in a log record (SRB-FR-IZAB).
    ///
    /// It is not the handle: a record names no handle, so the relay gives each
    /// client connection a second identifier that is safe to write down. It
    /// carries no routing meaning and unlocks nothing.
    pub connection_id: String,
    /// The registration the handle was assigned for, as the QR payload named
    /// it. The relay routes by it and reads nothing else in it (RSN-FR-NGKW).
    pub registration_id: String,
    /// The one worker the handle is bound to (RSN-FR-UDCM).
    pub bound_instance_id: String,
    /// The lifecycle state of the handle (RSN-FR-FKRE).
    pub state: HandleState,
    /// The worker route the connection is attached to, when it holds one.
    pub instance_id: Option<String>,
    /// Whether the worker has authenticated the connection (SRB-FR-HZLE).
    pub authenticated: bool,
    pub registered_at: Timestamp,
}

/// The operations the transport adapter drives (SRB-FR-DYAE).
///
/// Every operation applies whole or changes nothing, and the state is safe to
/// drive from many connections at once (SRB-FR-VUCM).
pub trait RelayApi: Send + Sync {
    /// Records a worker connection, replacing one that holds the same
    /// `instance_id` (SRB-FR-LGQB).
    fn register_worker(
        &self,
        instance_id: &str,
        connection_id: &str,
        projects: Vec<ExposedProject>,
    ) -> Result<WorkerRegistration, DomainError>;

    /// Replaces the whole exposed set of a worker and its selection, in one
    /// operation (SRB-FR-NIUT, RSN-FR-CQEF, RSN-FR-VTKA).
    ///
    /// The set and the selection are applied together, so no reader ever
    /// observes one announcement's projects beside another's selection.
    ///
    /// `connection_id` names the connection that announced. A connection that
    /// no longer holds the route changes nothing, so a replacement that lands
    /// between a transport's own check and this call still wins (SRB-FR-LGQB,
    /// RSN-FR-EHUT).
    fn announce_projects(
        &self,
        instance_id: &str,
        connection_id: &str,
        projects: Vec<ExposedProject>,
        selected_project_id: Option<&str>,
    ) -> Result<WorkerRoute, DomainError>;

    /// Selects a project of the worker's own exposed set (SRB-FR-FVJD).
    fn select_project(
        &self,
        instance_id: &str,
        project_id: &str,
    ) -> Result<WorkerRoute, DomainError>;

    /// The connection that holds a route now, without writing a record.
    ///
    /// The transport reads it on every worker frame, so a connection that was
    /// replaced stops being authoritative at once (SRB-FR-LGQB, RSN-FR-EHUT).
    fn route_connection(&self, instance_id: &str) -> Option<String>;

    /// Drops a worker route and detaches its clients (SRB-FR-GJSP).
    ///
    /// The handles of the detached connections are returned.
    fn drop_worker(
        &self,
        instance_id: &str,
        connection_id: &str,
    ) -> Result<Vec<String>, DomainError>;

    /// The active worker routes (SRB-FR-SEKN).
    fn workers(&self) -> Vec<WorkerRoute>;

    /// One worker route.
    fn worker(&self, instance_id: &str) -> Result<WorkerRoute, DomainError>;

    /// Assigns a fresh opaque handle for one registration route
    /// (SRB-FR-XSAG, RSN-FR-EIVS).
    ///
    /// The handle starts `pending`: it is eligible for the client endpoint only
    /// after the IDE accepts the registration (RSN-FR-HSNA). A registration
    /// whose target holds no worker route is refused `worker_not_found`
    /// (RSN-FR-BUXE).
    fn register_client(
        &self,
        registration_id: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode>;

    /// Applies the one terminal registration result (RSN-FR-HSNA).
    ///
    /// An accepted result makes the handle `active`; a rejected result
    /// invalidates it at once. A handle that is no longer `pending` is refused,
    /// so a second result for one registration changes nothing (RSN-FR-GPLZ).
    fn resolve_registration(
        &self,
        handle: &str,
        instance_id: &str,
        accepted: bool,
    ) -> Result<ClientConnection, ReasonCode>;

    /// Drops a handle whose registration connection closed before the IDE
    /// resolved it (RSN-FR-YAOC).
    ///
    /// A handle that was already accepted or invalidated is left alone: only a
    /// `pending` one belongs to the registration connection that opened it.
    fn drop_pending_registration(&self, handle: &str);

    /// Attaches a client connection to one worker route (SRB-FR-MTHQ).
    ///
    /// The handle must be `active` and must be bound to that worker
    /// (RSN-FR-UDCM, RSN-FR-NUAB).
    fn attach_client(
        &self,
        handle: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode>;

    /// Records that the worker authenticated the client (SRB-FR-HZLE).
    ///
    /// The worker must be the one the handle is bound to (RSN-FR-UDCM,
    /// RSN-FR-FKRE), so one worker never authenticates another worker's client.
    fn authenticate_client(
        &self,
        handle: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode>;

    /// The selected project the connection observes (SRB-FR-CBWU).
    fn observe_selected_project(&self, handle: &str) -> Result<Option<String>, ReasonCode>;

    /// The authenticated handles a worker broadcast reaches (SRB-FR-PWNK).
    ///
    /// A connection that no longer holds the route reaches none of them.
    fn broadcast_targets(
        &self,
        instance_id: &str,
        connection_id: &str,
    ) -> Result<Vec<String>, ReasonCode>;

    /// Invalidates a handle for the reason the caller names
    /// (RSN-FR-DWLS, RSN-FR-KVBO).
    ///
    /// The record is kept, so every later frame that presents the handle is
    /// refused with the same reason rather than as an unknown handle
    /// (RSN-FR-ADXL).
    /// `instance_id` names the worker that asked, and the handle must be bound
    /// to it. `None` is the relay acting on its own behalf.
    fn invalidate_handle(
        &self,
        handle: &str,
        instance_id: Option<&str>,
        reason: InvalidReason,
    ) -> Result<ClientConnection, ReasonCode>;

    /// Invalidates a handle after a protocol failure (SRB-FR-AVTK,
    /// RSN-FR-SGQX).
    fn reset_client(&self, handle: &str) -> Result<ClientConnection, ReasonCode>;

    /// One client connection, whatever its handle state.
    fn client(&self, handle: &str) -> Result<ClientConnection, ReasonCode>;

    /// The same read, without writing a record.
    ///
    /// The transport reads the handle state on every inbound frame, and a
    /// record for each of them would evict the records a reader needs
    /// (SRB-FR-IZAB).
    fn peek_client(&self, handle: &str) -> Result<ClientConnection, ReasonCode>;

    /// Takes the replacement notice a connection has not read yet.
    fn take_replacement_notice(&self, connection_id: &str) -> Option<ReplacementNotice>;
}
