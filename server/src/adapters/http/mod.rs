//! The HTTP adapter of the application service.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-VBQJ, SAS-FR-LFCA.

pub mod content_routes;
pub mod dto;
pub mod error;
pub mod extract;
pub mod identity_routes;
pub mod logging;
pub mod membership_routes;
pub mod project_routes;
pub mod state;

use axum::Router;

use crate::adapters::http::state::HttpState;

/// Every authenticated route of the service.
///
/// The health route of `BMS-backend-microservice.md` BMS-FR-11 is not here: it
/// is the one route that needs no credential.
///
/// The record of the route, the status, and the identifiers the request touched
/// is written by the layer the caller adds over this router
/// (`logging::log_requests`).
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(identity_routes::routes())
        .merge(membership_routes::routes())
        .merge(project_routes::routes())
        .merge(content_routes::routes())
        .merge(crate::adapters::relay::routes::routes())
        .merge(crate::adapters::relay::projects::routes())
        .merge(crate::adapters::relay::ws::routes())
}
