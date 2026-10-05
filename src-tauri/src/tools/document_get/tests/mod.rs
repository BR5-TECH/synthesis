//! Tests for `GDT-get-document-tool.md`.
//!
//! The group-wide claims of `TLC-tool-conventions.md` are swept over every tool
//! in `../tests`. What is here is this tool's own behaviour: what it reads, how
//! it cuts a range, and what it refuses.

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::Manager;

use super::*;
use crate::documents::SourceKind;
use crate::logging::{Domain, LogFilter, LogLevel};
use crate::tools::tests::document_fixture::DocFixture;
use crate::tools::tests::{block_on, closed_project};

mod access;
mod contract;
mod ranges;
mod records;

fn args(id: &str) -> GetDocumentArgs {
    GetDocumentArgs {
        id: id.to_string(),
        byte_offset: None,
        byte_length: None,
        line_offset: None,
        line_limit: None,
    }
}

fn get(fixture: &DocFixture, args: GetDocumentArgs) -> Result<String, ToolRefusal> {
    block_on(GetDocumentTool::new(fixture.fixture.handle()).call(args))
}

/// A selected folder holding one Markdown document, and its id.
fn with_document(content: &str) -> (DocFixture, String) {
    let fixture = DocFixture::new();
    fixture.write("refs/doc.md", content);
    fixture.select(SourceKind::Folder, "refs");
    let id = fixture.id_of("refs/doc.md");
    (fixture, id)
}
