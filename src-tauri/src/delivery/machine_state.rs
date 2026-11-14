//! `DMS-delivery-machine-state.md`: how the author arranges a delivery canvas
//! on this machine, and the plan data that a GitHub plan has no field for.
//!
//! Three kinds of record are kept under `delivery/` in the repository store of
//! the open project (DMS-FR-KITP, RMS-FR-OCPB), outside every worktree:
//!
//! - the **layout** of a scope (DMS-FR-DMWD): `delivery/layout/<digest>.json`,
//!   read and written by the two commands of [`commands`];
//! - the **overlay** of a GitHub plan (DMS-FR-WAUS):
//!   `delivery/overlay/<project-node-id>.json`, in-process only (DMS-FR-VEHN);
//! - the **publish journal** of a Flow file (DMS-FR-RVJQ):
//!   `delivery/publish/<digest>.json`, in-process only.
//!
//! Every write is one atomic whole-file write (RMS-FR-PFOB), and every read and
//! write goes through the FSA primitives of `crate::fs` with the store as the
//! root. No log record of this module carries a footprint path, an artifact id,
//! a draft id, or an item name (DMS-FR-ROMF): a record names the kind of the
//! record and counts.

pub mod commands;
pub mod journal;
pub mod layout;
pub mod overlay;
pub(in crate::delivery) mod records;

pub use journal::{
    read_publish_journal, remove_publish_journal, write_publish_journal, CreatedItem,
    CreatedItemKind, CreatedRelation, PublishJournal,
};
pub use layout::{
    load_layout, parse_scope, save_layout, DeliveryLayout, Extent, LayoutEntry, LayoutScope, Point,
};
pub use overlay::{
    read_delivery_overlay, validate_overlay, write_delivery_overlay, DeliveryOverlay, OverlayEntry,
    RelatesEdge, DUPLICATE_MILESTONE_ORDER,
};
pub use records::scope_digest;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_overlay;
