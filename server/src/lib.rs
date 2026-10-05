//! The Synthesis backend microservice, as a library.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirements: BMS-FR-03, BMS-FR-04.
//!
//! The binary holds the process entry point alone. Every layer lives here: the
//! domain and the application services of
//! `specifications/server/SAS-server-application-service.md`, the relay
//! boundary of `specifications/server/SRB-server-relay-boundary.md`, the
//! persistence adapter, and the HTTP surface.

pub mod adapters;
pub mod application;
pub mod config;
pub mod domain;
pub mod identity_config;
pub mod persistence;
pub mod router;
pub mod shutdown;
pub mod testing;
pub mod version;

// The build script includes these two files as well, so the crate compiles them
// only to give the build script's selection rule and its command probe a home
// the tests can reach (BMS-FR-16, BMS-FR-26). Nothing in the running service
// calls either.
#[allow(dead_code)]
pub mod build_probe;
#[allow(dead_code)]
pub mod version_resolve;

#[cfg(test)]
mod guards;

use std::sync::Arc;

use crate::adapters::http::state::HttpState;
use crate::adapters::relay::state::RelayRegistry;
use crate::application::logging::stdout_sink;
use crate::application::startup::reconcile_administrator;
use crate::application::Application;
use crate::domain::error::DomainError;
use crate::identity_config::IdentityConfig;
use crate::persistence::memory::memory_ports;
use crate::router::ServiceState;

/// Builds the service state over the in-memory persistence adapter.
///
/// SAS-FR-DTXV: the administrator user is created or reconciled here, before
/// the first request is served.
pub fn build_state(
    identity: IdentityConfig,
    build_version: &'static str,
) -> Result<ServiceState, DomainError> {
    let ports = memory_ports();
    reconcile_administrator(&ports, identity.administrator_id)?;

    // One sink for the whole process: the relay records and the request
    // records reach the same standard output BMS-FR-09 already writes to.
    let logs = stdout_sink();

    let relay = Arc::new(RelayRegistry::new(
        Arc::clone(&ports.clock),
        Arc::clone(&ports.ids),
        Arc::clone(&logs),
    ));

    Ok(ServiceState {
        version: build_version,
        http: HttpState {
            hub: Arc::new(crate::adapters::relay::ws::hub::Hub::new()),
            ids: Arc::clone(&ports.ids),
            application: Application::new(ports),
            relay,
            identity: Arc::new(identity),
            logs,
        },
    })
}
