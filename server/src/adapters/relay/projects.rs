//! The project-discovery route of the remote session.
//!
//! Specification: `specifications/server/RSN-remote-session.md`.
//! Requirements: RSN-FR-MHDV, RSN-FR-QLEC, RSN-FR-ANWK, RSN-FR-TFJU,
//! RSN-FR-GXWB, RSN-FR-ZDPA, RSN-FR-XBRC.
//!
//! The route reports the projects one worker exposes to one registered client.
//! It serves no global list, and it reports no key, no handle, no application
//! payload, no user, and no ownership data.

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::adapters::http::state::HttpState;
use crate::adapters::relay::port::ExposedProject;
use crate::adapters::relay::session::ReasonCode;
use crate::application::ports::Principal;

/// The path of the route.
pub const PROJECTS_PATH: &str = "/v1/relay/projects";

/// The header the client presents its opaque handle in (RSN-FR-MHDV).
pub const HANDLE_HEADER: &str = "x-synthesis-client-handle";
/// The header the client names its target worker in (RSN-FR-MHDV).
pub const INSTANCE_HEADER: &str = "x-synthesis-instance-id";

/// The route of RSN-FR-MHDV.
pub fn routes() -> Router<HttpState> {
    Router::new().route("/v1/relay/projects", get(read_projects))
}

/// What the route reports (RSN-FR-QLEC).
#[derive(Debug, Serialize)]
struct ProjectsBody {
    instance_id: String,
    selected_project_id: Option<String>,
    projects: Vec<ExposedProject>,
}

/// One refusal of the route, with the reason code the client reads
/// (RSN-FR-TFJU).
#[derive(Debug, Clone, Copy)]
struct SessionRefusal(ReasonCode);

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: ErrorMember,
}

#[derive(Debug, Serialize)]
struct ErrorMember {
    code: &'static str,
    message: &'static str,
}

impl IntoResponse for SessionRefusal {
    fn into_response(self) -> Response {
        let status = match self.0 {
            // A handle the relay does not hold, and a worker with no route, are
            // each reported as absent rather than as refused, so the answer
            // discloses no session that exists.
            ReasonCode::HandleNotFound | ReasonCode::WorkerNotFound => StatusCode::NOT_FOUND,
            ReasonCode::HandleRevoked | ReasonCode::HandleExpired | ReasonCode::HandleReset => {
                StatusCode::FORBIDDEN
            }
            _ => StatusCode::BAD_REQUEST,
        };
        let body = Json(ErrorBody {
            error: ErrorMember {
                code: self.0.as_str(),
                // The message names the reason alone: no handle value, no key,
                // and no payload reaches an error answer.
                message: "the request does not name a routed session",
            },
        });
        (status, body).into_response()
    }
}

/// Reads one header, or refuses the request as naming no session.
fn header(headers: &HeaderMap, name: &str) -> Result<String, SessionRefusal> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(SessionRefusal(ReasonCode::HandleNotFound))
}

/// Answers `GET /v1/relay/projects`.
///
/// RSN-FR-XBRC: `Principal` resolves the bearer token before the handler runs,
/// so a request with no token never reaches a project list. The handle and the
/// worker are checked after it, because the token authenticates transport
/// access alone (RSN-FR-TWQJ).
async fn read_projects(
    State(state): State<HttpState>,
    _principal: Principal,
    headers: HeaderMap,
) -> Result<Response, SessionRefusal> {
    let handle = header(&headers, HANDLE_HEADER)?;
    let instance_id = header(&headers, INSTANCE_HEADER)?;

    // RSN-FR-TFJU, RSN-FR-ZDPA: a handle no route holds — including every
    // handle after a restart — is answered as absent.
    let client = state
        .relay
        .client(&handle)
        .map_err(|_| SessionRefusal(ReasonCode::HandleNotFound))?;
    if let Some(refusal) = client.state.refusal() {
        return Err(SessionRefusal(refusal));
    }
    // RSN-FR-ANWK: the route reports the projects of the worker the handle is
    // bound to, and of no other worker.
    if client.bound_instance_id != instance_id {
        return Err(SessionRefusal(ReasonCode::HandleNotFound));
    }

    let route = state
        .relay
        .worker(&instance_id)
        .map_err(|_| SessionRefusal(ReasonCode::WorkerNotFound))?;

    // RSN-FR-GXWB: the answer is the announcement the route holds now. The
    // relay caches nothing beyond it, so it is never served from a store.
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(ProjectsBody {
            instance_id: route.instance_id,
            selected_project_id: route.selected_project_id,
            projects: route.projects,
        }),
    )
        .into_response())
}
