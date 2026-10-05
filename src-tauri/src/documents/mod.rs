//! The Documents collection (`DCL-documents-collection.md`): the reference
//! documents a user selects for a project — PDF, Markdown, and text files,
//! wherever they live — so that the user and conversational agents can read
//! them.
//!
//! The collection stores references to the selected files and folders and
//! nothing else. It reads them only through the documents instance of
//! `FSA-filesystem-access.md`, which reaches the selected paths read-only.
//!
//! - `model.rs`: the payload types, the typed errors, and the path rules.
//! - `discovery.rs`: the walk over the sources.
//! - `manager.rs`: the state of the collection and its refresh.
//! - `reading.rs`: reads of one document and the session PDF text cache.
//! - `watcher.rs`: the watch over the selected paths.
//! - `lifecycle.rs`: the collection on the application.
//! - `commands.rs`: the Tauri commands.

pub mod commands;
mod discovery;
pub mod lifecycle;
pub mod log;
pub mod manager;
pub mod model;
pub mod reading;
pub mod watcher;

#[cfg(test)]
mod tests;

pub use manager::DocumentsCollection;
pub use model::{
    document_id, format_of, normalise_path, Availability, DocumentEntry, DocumentFormat,
    DocumentPdf, DocumentSource, DocumentText, DocumentsError, DocumentsSnapshot,
    PickDocumentSourcesResult, PickMode, SourceKind, SourceReason, StoredSource,
};
pub use reading::IndexDocument;
pub use watcher::DocumentsWatcher;
