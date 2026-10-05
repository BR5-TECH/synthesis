//! The fixtures the tests build a service from.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-DTXV, SAS-FR-VBQJ.
//!
//! The module holds no credential of the running service: the token here is a
//! literal a test presents to a service it started itself.

use std::sync::Arc;

use crate::adapters::http::state::HttpState;
use crate::adapters::relay::state::RelayRegistry;
use crate::application::logging::CaptureSink;
use crate::application::ports::Ports;
use crate::application::startup::reconcile_administrator;
use crate::application::Application;
use crate::domain::ids::UserId;
use crate::identity_config::{IdentityConfig, StaticToken};
use crate::persistence::memory::memory_ports;
use crate::router::ServiceState;

/// The credential a test presents.
pub const TEST_TOKEN: &str = "test-token-0123456789";
/// The administrator identifier a test configures.
pub const TEST_ADMIN_ID: &str = "1a4c5f6b-2d3e-4f50-9a8b-7c6d5e4f3a2b";
/// The build version a test service reports.
pub const TEST_VERSION: &str = "v9.9.9-test";

/// A service state over a store that holds the reconciled administrator alone.
pub fn test_state() -> (ServiceState, Ports) {
    let (state, ports, _logs) = test_state_with_logs();
    (state, ports)
}

/// The same service, with the sink the records are read back from.
///
/// A test that asserts what the service logged holds the sink; every other test
/// takes `test_state` and drops it.
pub fn test_state_with_logs() -> (ServiceState, Ports, Arc<CaptureSink>) {
    let ports = memory_ports();
    let identity = IdentityConfig {
        administrator_id: UserId::parse(TEST_ADMIN_ID).expect("the fixture identifier is a UUID"),
        token: StaticToken::new(TEST_TOKEN.to_string()),
    };
    reconcile_administrator(&ports, identity.administrator_id)
        .expect("the administrator is written");

    let logs = Arc::new(CaptureSink::new());

    let relay = Arc::new(RelayRegistry::new(
        Arc::clone(&ports.clock),
        Arc::clone(&ports.ids),
        Arc::clone(&logs) as Arc<dyn crate::application::logging::LogSink>,
    ));

    let state = ServiceState {
        version: TEST_VERSION,
        http: HttpState {
            application: Application::new(ports.clone()),
            relay,
            hub: Arc::new(crate::adapters::relay::ws::hub::Hub::new()),
            ids: Arc::clone(&ports.ids),
            identity: Arc::new(identity),
            logs: Arc::clone(&logs) as Arc<dyn crate::application::logging::LogSink>,
        },
    };
    (state, ports, logs)
}
