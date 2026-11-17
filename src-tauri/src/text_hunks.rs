//! Anchored hunks over plain text (`PCP-prompt-change-proposals.md` PCP-FR-29).
//!
//! The one module both proposal stores use to name, place, and apply a change.
//! It holds no proposal, draft, or artifact state, touches no disk, and names
//! neither store, so sharing it shares no operation between them.

pub mod anchors;
mod diagrams;
pub mod fences;
pub mod hunks;
pub mod line_diff;
pub mod updates;

#[cfg(test)]
mod diagram_tests;
#[cfg(test)]
mod tests;
