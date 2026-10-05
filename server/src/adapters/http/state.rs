//! The state every application route carries.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-VBQJ, SAS-FR-XRPD.

use std::sync::Arc;

use crate::adapters::relay::port::RelayApi;
use crate::adapters::relay::ws::hub::Hub;
use crate::application::logging::LogSink;
use crate::application::Application;
use crate::domain::ids::IdSource;
use crate::identity_config::IdentityConfig;

/// The inbound ports and the identity settings the HTTP adapter holds.
#[derive(Clone)]
pub struct HttpState {
    /// The application services (SAS-FR-VBQJ).
    pub application: Application,
    /// The relay routing state (`SRB-server-relay-boundary.md` SRB-FR-DYAE).
    pub relay: Arc<dyn RelayApi>,
    /// The sinks of the open WebSocket connections
    /// (`WSK-websocket.md` WSK-FR-SYNB). The hub holds no routing state: every
    /// routing decision is read from `relay`.
    pub hub: Arc<Hub>,
    /// The source of the connection identifiers a relay record names
    /// (`SRB-server-relay-boundary.md` SRB-FR-IZAB).
    pub ids: Arc<dyn IdSource>,
    /// The administrator identifier and the static token.
    pub identity: Arc<IdentityConfig>,
    /// The diagnostic channel the request record is written to
    /// (the non-functional logging rule of SAS).
    pub logs: Arc<dyn LogSink>,
}
