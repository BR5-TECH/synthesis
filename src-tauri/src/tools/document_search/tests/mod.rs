//! Tests for `SDT-search-documents-tool.md`.
//!
//! The group-wide claims of `TLC-tool-conventions.md` are swept over every tool
//! in `../tests`. What is here is this tool's own behaviour: what it ranks, what
//! it refuses, and what it drops.

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::Manager;

use super::*;
use crate::documents::SourceKind;
use crate::logging::{Domain, LogFilter, LogLevel};
use crate::tools::tests::document_fixture::DocFixture;
use crate::tools::tests::{block_on, closed_project};

mod behaviour;
mod contract;
mod records;

fn search(fixture: &DocFixture, query: &str, limit: Option<i64>) -> Vec<DocumentMatch> {
    let tool = SearchDocumentsTool::new(fixture.fixture.handle());
    block_on(tool.call(SearchDocumentsArgs {
        query: query.to_string(),
        limit,
    }))
    .expect("the call succeeds")
    .documents
}

fn refusal(fixture: &DocFixture, query: &str) -> ToolRefusal {
    let tool = SearchDocumentsTool::new(fixture.fixture.handle());
    block_on(tool.call(SearchDocumentsArgs {
        query: query.to_string(),
        limit: None,
    }))
    .expect_err("the call refuses")
}
