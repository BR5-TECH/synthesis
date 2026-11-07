//! Document export (`DXP-document-export.md`).
//!
//! The backend converts a saved DOC artifact to a `.pdf` or a `.docx` file
//! (DXP-FR-IEQA), and owns the export requests that the agent tool
//! `crate::tools::request_document_export` records (DXP-FR-YEAQ).
//!
//! - `source` resolves and checks the file to export (DXP-FR-LNCQ, DXP-FR-PKDB).
//! - `convert` turns the Markdown into a PDF or a DOCX (DXP-FR-RKJC to DXP-FR-CPPT).
//! - `commands` holds the Tauri command of the export (DXP-FR-IEQA).
//! - `requests` holds the export requests and their commands (DXP-FR-YEAQ).

pub mod commands;
pub mod convert;
pub mod requests;
pub mod source;

// DXP contract surface, "Typed errors".
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_NOT_FOUND: &str = "not_found";
pub const ERR_NOT_DOC_ARTIFACT: &str = "not_doc_artifact";
pub const ERR_NOT_MARKDOWN: &str = "not_markdown";
pub const ERR_OUTSIDE_PROJECT: &str = "outside_project";
pub const ERR_INVALID_DESTINATION: &str = "invalid_destination";
pub const ERR_CONVERSION_FAILED: &str = "conversion_failed";
pub const ERR_WRITE_FAILED: &str = "write_failed";
pub const ERR_REQUEST_NOT_FOUND: &str = "request_not_found";
pub const ERR_ALREADY_RESOLVED: &str = "already_resolved";
pub const ERR_INVALID_OUTCOME: &str = "invalid_outcome";

#[cfg(test)]
mod tests;
