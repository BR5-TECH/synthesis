//! Tests for `NST-note-search-tool.md`.
//!
//! The group-wide claims of `TLC-tool-conventions.md` — the definition being
//! fixed, the refusal shapes, the log record's fields — are swept over every
//! tool at once in `../tests.rs`. What is here is this tool's own behaviour:
//! what it searches, what it refuses to search, and what it drops.

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::Manager;

use super::*;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::notes::NoteScope;
use crate::tools::tests::{agent_session, block_on};

mod contract;
mod decoding;
mod freshness;
mod limits;
mod match_shape;
mod records;
mod refusals;
mod searchable;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// A mounted project with an agent session over it, plus the notes a test seeds
/// into it.
///
/// Notes live outside what the project scan surfaces (ASC-FR-09), so the notes
/// index is fed from `crate::notes`' own channel rather than from the watcher
/// (BMI-FR-29). A test that writes a note therefore re-runs a pass through
/// [`NoteFixture::reindex`] to stand in for it.
struct NoteFixture {
    fixture: crate::tools::tests::Fixture,
    session: String,
}

impl NoteFixture {
    fn new() -> Self {
        let fixture = crate::tools::tests::empty_project();
        let root = fixture
            .app
            .state::<crate::bm25_index::Bm25Indexer>()
            .root()
            .expect("mounted");
        let session = agent_session(&fixture.app, &root, "note-search");
        NoteFixture { fixture, session }
    }

    fn root(&self) -> crate::fs::RootFs {
        let root = self
            .fixture
            .app
            .state::<crate::bm25_index::Bm25Indexer>()
            .root()
            .expect("mounted");
        crate::fs::RootFs::for_root(&root)
    }

    /// Write a project file, creating parents.
    fn write(&self, rel: &str, body: &str) {
        let path = self.root().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// Create a project-scoped note holding `body`, and return its id.
    fn note(&self, body: &str) -> String {
        self.scoped_note(NoteScope::Project, body, None, None)
    }

    fn scoped_note(
        &self,
        scope: NoteScope,
        body: &str,
        reminder: Option<&str>,
        revision: Option<&str>,
    ) -> String {
        crate::notes::create_note_in(
            &self.root(),
            scope,
            body.to_string(),
            reminder.map(str::to_string),
            revision.map(str::to_string),
            "2026-01-01T00:00:00Z",
        )
        .expect("the note writes")
        .id
    }

    fn reindex(&self) {
        self.fixture.reindex();
    }

    fn tool(&self) -> NoteSearchTool<tauri::test::MockRuntime> {
        NoteSearchTool::new(self.fixture.handle(), self.session.clone())
    }

    fn logged(&self, buffer: &'static LogBuffer) -> NoteSearchTool<tauri::test::MockRuntime> {
        NoteSearchTool::with_buffer(self.fixture.handle(), self.session.clone(), buffer)
    }

    fn search(&self, query: &str, limit: Option<i64>) -> Vec<NoteMatch> {
        block_on(self.tool().call(NoteSearchArgs {
            query: query.into(),
            limit,
        }))
        .expect("the call succeeds")
        .notes
    }

    fn refusal(&self, query: &str, limit: Option<i64>) -> rig::tool::ToolExecutionError {
        let tool = self.tool();
        let error = block_on(tool.call(NoteSearchArgs {
            query: query.into(),
            limit,
        }))
        .expect_err("the call refuses");
        tool.map_error(error)
    }
}

fn entity(entity_id: &str) -> NoteScope {
    NoteScope::Entity {
        entity_id: entity_id.to_string(),
        entity_path: entity_id.to_string(),
    }
}

fn ids(matches: &[NoteMatch]) -> Vec<String> {
    let mut out: Vec<String> = matches.iter().map(|m| m.id.clone()).collect();
    out.sort();
    out
}

/// Every record `buffer` holds under `domain`.
fn records_of(buffer: &LogBuffer, domain: Domain) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![domain],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records
}

/// A hit as the notes index produces one.
fn hit(note_id: &str, score: f32) -> crate::bm25_index::ChunkHit {
    crate::bm25_index::ChunkHit {
        index: IndexId::Notes,
        path: note_id.to_string(),
        node_id: None,
        draft_id: None,
        note_id: Some(note_id.to_string()),
        document_id: None,
        chunk_ordinal: 0,
        score,
        text: "whatever the pass captured".to_string(),
    }
}

