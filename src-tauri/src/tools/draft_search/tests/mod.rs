//! Tests for `DST-draft-search-tool.md`.
//!
//! The group-wide claims of `TLC-tool-conventions.md` — the definition being
//! fixed, the refusal shapes, the log record's fields — are swept over every
//! tool at once in `../tests.rs`. What is here is this tool's own behaviour:
//! what it searches, what it refuses to search, and what it drops.

use rig::tool::PortableTool;
use tauri::Manager;

use super::*;
use crate::drafts::{DraftStatus, ERR_NOT_SINGLE_FILE};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{agent_session, block_on};

mod contract;
mod dropped_hits;
mod limits;
mod posture;
mod refusals;
mod resolving;
mod searched;


// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// A mounted project with an agent session over it, plus the drafts a test
/// seeds into it.
///
/// Drafts live outside what the project scan surfaces (ASC-FR-09), so the
/// drafts index is fed from `crate::drafts`' own channel rather than from the
/// watcher (BMI-FR-19). A test that writes a prompt therefore re-runs a pass
/// through [`DraftFixture::reindex`] to stand in for it.
struct DraftFixture {
    fixture: crate::tools::tests::Fixture,
    session: String,
}

impl DraftFixture {
    fn new() -> Self {
        let fixture = crate::tools::tests::empty_project();
        let root = fixture.app.state::<crate::bm25_index::Bm25Indexer>().root().expect("mounted");
        let session = agent_session(&fixture.app, &root, "draft-search");
        DraftFixture { fixture, session }
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

    /// Create a draft holding `body`, and return its id.
    fn draft(&self, name: &str, body: &str) -> String {
        let root = self.root();
        let created = crate::drafts::create_draft_at_root(&root, Some(name))
            .expect("the scaffold completes");
        let id = created.draft.id.clone();
        let prompt = created.draft.prompt_path.clone().expect("a created draft holds its prompt");
        crate::drafts::save_draft_file_impl(&root, &id, &prompt, body).expect("the prompt writes");
        id
    }

    fn archive(&self, id: &str) {
        crate::drafts::set_draft_status_impl(&self.root(), id, DraftStatus::Archived)
            .expect("the status writes");
    }

    fn reindex(&self) {
        self.fixture.reindex();
    }

    fn tool(&self) -> DraftSearchTool<tauri::test::MockRuntime> {
        DraftSearchTool::new(self.fixture.handle(), self.session.clone())
    }

    fn logged(&self, buffer: &'static LogBuffer) -> DraftSearchTool<tauri::test::MockRuntime> {
        DraftSearchTool::with_buffer(self.fixture.handle(), self.session.clone(), buffer)
    }

    fn search(&self, query: &str, limit: Option<i64>) -> Vec<DraftSearchMatch> {
        block_on(self.tool().call(DraftSearchArgs {
            query: query.into(),
            limit,
        }))
        .expect("the call succeeds")
        .drafts
    }
}

/// Seed a draft with a second file in `files/`, which is what DRS-FR-11 forbids
/// and DRS-FR-15 reports (DST-FR-13).
fn make_inconsistent(fixture: &DraftFixture, id: &str) {
    let dir = crate::drafts::draft_dir(&fixture.root(), id).expect("the draft resolves");
    std::fs::write(dir.join("files").join("intruder.md"), "# second\n").expect("the write lands");
}
