//! The plan proposals of the Map discussions (`MPR-map-plan-proposals.md`).
//!
//! An agent in a Map discussion records a proposal of rows. The author decides
//! each row on its own. Nothing reaches GitHub, and no draft is created, before
//! the author accepts the row that does it.

mod applying;
mod commands;
mod deciding;
mod errors;
mod model;
mod recording;
mod rules;
mod store;

#[cfg(test)]
mod tests;

pub use commands::*;
pub use deciding::{accept_row, reject_row};
pub use errors::MAP_PLAN_PROPOSALS_CHANGED;
pub use model::*;
pub use recording::*;
