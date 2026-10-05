//! Tests for `RFT-read-file-tool.md`.
//!
//! The group-wide conventions this tool shares with the other four are checked
//! once in `tools/tests.rs`; what follows is what is true of this tool alone.

use rig::tool::{PortableTool, ToolErrorKind};

use tempfile::TempDir;
use tauri::Manager;

use super::*;
use crate::bm25_index::Bm25Indexer;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted, Fixture};
use crate::tools::ToolRefusal;

mod binary_links;
mod identity;
mod log_records;
mod paging;
mod posture;
mod refusals;
mod resolution;
mod slicing;


// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn write(root: &std::path::Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
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

/// A record's field names, sorted.
fn field_keys(record: &crate::logging::LogRecord) -> Vec<&str> {
    let mut keys: Vec<&str> = record.fields.keys().map(String::as_str).collect();
    keys.sort();
    keys
}

/// Everything `buffer` holds, rendered, for the leak scans.
fn rendered_records(buffer: &LogBuffer) -> String {
    serde_json::to_string(
        &buffer
            .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
            .unwrap()
            .records,
    )
    .unwrap()
}

/// A run of printable ASCII inside the binary fixture. Distinctive enough that
/// finding it anywhere means it came from the file and nowhere else, and — being
/// plain ASCII — it survives `String::from_utf8_lossy` intact, so a leak really
/// would show up.
const BINARY_MARKER: &str = "CONFIDENTIALPIXELDATA";

/// A hundred lines, each naming its own 0-based index.
fn hundred_lines() -> String {
    (0..100)
        .map(|i| format!("line {i}\n"))
        .collect::<Vec<_>>()
        .concat()
}

struct Project {
    fixture: Fixture,
    root: std::path::PathBuf,
    /// The agent session's instance: the worktree and its own temp directory,
    /// and no `app_data_dir()` (FSA-FR-29).
    access: std::sync::Arc<crate::fs::FsAccess>,
    session: String,
    temp: std::path::PathBuf,
}

impl Project {
    fn tool(&self) -> FileReadTool<tauri::test::MockRuntime> {
        FileReadTool::new(self.fixture.handle(), self.session.clone())
    }

    /// The same tool, reporting into `buffer`. Every test that inspects a record
    /// has to drive `call` rather than [`Project::read`], the free function
    /// logging nothing.
    fn logged_tool(&self, buffer: &'static LogBuffer) -> FileReadTool<tauri::test::MockRuntime> {
        FileReadTool::with_buffer(self.fixture.handle(), self.session.clone(), buffer)
    }

    fn read(&self, args: ReadFileArgs) -> Result<String, ToolRefusal> {
        read(&self.access, &self.root, &args)
    }

    fn at(&self, path: &str) -> Result<String, ToolRefusal> {
        self.read(ReadFileArgs {
            path: path.to_string(),
            offset: None,
            limit: None,
        })
    }
}

/// A mounted project whose reads run on an agent-profile instance.
fn project() -> Project {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "src/a.ts", "export const a = 1;\n");
    write(root, "src/nested/deep.ts", "// deep\n");
    write(root, "long.txt", &hundred_lines());
    write(root, "empty.txt", "");
    write(root, "crlf.txt", "one\r\ntwo\r\nthree\r\nfour\r\n");
    write(root, "no-final-newline.txt", "alpha\nbeta\ngamma");
    write(root, "multibyte.txt", "α∫∂\nβ√ç\nγ†ø\n");
    // Not UTF-8: 0xFF can never begin a valid sequence. The printable run in
    // front of it is what makes the leak scan below able to fail — a scan for
    // bytes that cannot survive a lossy rendering finds nothing whatever leaks.
    let mut blob = BINARY_MARKER.as_bytes().to_vec();
    blob.extend_from_slice(&[0xFF, 0xFE, 0x00]);
    std::fs::write(root.join("blob.png"), &blob).unwrap();

    let fixture = mounted(dir);
    let root = fixture.app.state::<Bm25Indexer>().root().expect("mounted");
    // The agent profile, built by the state exactly as FSA-FR-29 describes it.
    let session = crate::tools::tests::agent_session(&fixture.app, &root, "reader");
    let access = fixture
        .app
        .state::<crate::fs::FsAccessState>()
        .agent_session(&session)
        .expect("just opened");
    let temp = access.session_temp_dir().expect("the profile has one").to_path_buf();
    Project {
        fixture,
        root,
        access,
        session,
        temp,
    }
}
