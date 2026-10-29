//! Anchored hunks over plain text (`PCP-prompt-change-proposals.md` PCP-FR-29).
//!
//! The one module both proposal stores use to name, place, and apply a change.
//! It holds no proposal, draft, or artifact state, touches no disk, and names
//! neither store, so sharing it shares no operation between them.

pub mod anchors;
pub mod hunks;

#[cfg(test)]
mod tests;
