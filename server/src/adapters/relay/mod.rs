//! The relay boundary: the routing state, its port, the session rules, and the
//! routes that reach them.
//!
//! Specification: `specifications/server/SRB-server-relay-boundary.md`.
//! Requirements: SRB-FR-DYAE, SRB-FR-TOQF.
//!
//! The WebSocket routes and their upgrade handlers belong to
//! `specifications/server/WSK-websocket.md` and are driven against the port of
//! SRB-FR-DYAE. The session rules and the project-discovery route belong to
//! `specifications/server/RSN-remote-session.md`.

pub mod port;
pub mod projects;
pub mod routes;
pub mod session;
pub mod state;
pub mod ws;

#[cfg(test)]
mod tests;
