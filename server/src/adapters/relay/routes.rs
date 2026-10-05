//! The relay introspection routes.
//!
//! Specification: `specifications/server/SRB-server-relay-boundary.md`.
//! Requirements: SRB-FR-SEKN, SRB-FR-QMDT, SRB-FR-TOQF.
//!
//! Both routes are read-only. They report the routes and their client counts,
//! and report no handle content and no payload.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use crate::adapters::http::dto::ListBody;
use crate::adapters::http::error::ApiError;
use crate::adapters::http::state::HttpState;
use crate::adapters::relay::port::WorkerRoute;
use crate::application::ports::Principal;

/// The relay routes of V1.
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route("/v1/relay/workers", get(list_workers))
        .route("/v1/relay/workers/{instance_id}", get(read_worker))
}

async fn list_workers(
    State(state): State<HttpState>,
    _principal: Principal,
) -> Json<ListBody<WorkerRoute>> {
    Json(ListBody::new(state.relay.workers()))
}

async fn read_worker(
    State(state): State<HttpState>,
    _principal: Principal,
    Path(instance_id): Path<String>,
) -> Result<Json<WorkerRoute>, ApiError> {
    Ok(Json(state.relay.worker(&instance_id)?))
}
