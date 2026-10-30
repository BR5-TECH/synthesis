//! Tests for `MDT-measure-document-tool.md`.
//!
//! The group-wide claims of `TLC-tool-conventions.md` are swept over every tool
//! at once in `../tests/`. What is here is this tool's own behaviour.

use rig::tool::PortableTool;
use tauri::Manager;

use super::*;
use crate::logging::{LogFilter, LogLevel};
use crate::tools::tests::{agent_session, block_on};

struct MeasureFixture {
    fixture: crate::tools::tests::Fixture,
    session: String,
}

impl MeasureFixture {
    fn new() -> Self {
        let fixture = crate::tools::tests::empty_project();
        let root = fixture.app.state::<crate::bm25_index::Bm25Indexer>().root().expect("mounted");
        let session = agent_session(&fixture.app, &root, "measure");
        MeasureFixture { fixture, session }
    }

    fn root(&self) -> crate::fs::RootFs {
        let root =
            self.fixture.app.state::<crate::bm25_index::Bm25Indexer>().root().expect("mounted");
        crate::fs::RootFs::for_root(&root)
    }

    fn file(&self, rel: &str, body: &[u8]) {
        let path = self.root().path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn draft(&self, body: &str) -> String {
        let root = self.root();
        let created =
            crate::drafts::create_draft_at_root(&root, Some("measured")).expect("scaffold");
        let id = created.draft.id.clone();
        let prompt = created.draft.prompt_path.clone().expect("a prompt");
        crate::drafts::save_draft_file_impl(&root, &id, &prompt, body).expect("writes");
        id
    }

    /// A tool whose window is `window`, or the default context size where it
    /// is `None` (AAP-FR-RANS).
    fn tool(&self, window: Option<u64>) -> MeasureDocumentTool<tauri::test::MockRuntime> {
        self.tool_at(window.unwrap_or(crate::ai_api::DEFAULT_CONTEXT_SIZE), 30)
    }

    fn tool_at(&self, window: u64, percent: u8) -> MeasureDocumentTool<tauri::test::MockRuntime> {
        MeasureDocumentTool::new(
            self.fixture.handle(),
            self.session.clone(),
            DocumentBudget { window, percent },
        )
    }

    fn call(
        &self,
        window: Option<u64>,
        args: serde_json::Value,
    ) -> Result<MeasureDocumentOutput, ToolRefusal> {
        let args: MeasureDocumentArgs = serde_json::from_value(args).expect("decodes");
        block_on(self.tool(window).call(args))
    }
}

// MDT-FR-DRFB, MDT-FR-YFZQ: a portable tool under the documented name, with
// the fixed description and two optional parameters.
#[test]
fn is_a_portable_tool_with_two_optional_parameters() {
    let fixture = MeasureFixture::new();
    let definition = rig::tool::tool_definition(&fixture.tool(None));
    assert_eq!(definition.name, "measure_document");
    assert_eq!(definition.description, DESCRIPTION);
    let properties = definition.parameters["properties"].as_object().expect("an object");
    assert_eq!(properties.len(), 2);
    assert_eq!(definition.parameters["properties"]["path"]["description"], PATH_DESCRIPTION);
    assert_eq!(
        definition.parameters["properties"]["draft_id"]["description"],
        DRAFT_ID_DESCRIPTION
    );
    assert_eq!(definition.parameters["required"], serde_json::json!([]), "neither alone");
    // MDT-FR-QYLP: the description calls the count an estimate.
    assert!(DESCRIPTION.contains("estimate"));
}

// MDT-FR-EEFA, MDT-FR-YFZQ, TLC-FR-15: the definition does not vary by window, and no
// argument names a window, a percent, or a model.
#[test]
fn the_window_is_bound_at_construction_and_never_an_argument() {
    let fixture = MeasureFixture::new();
    let a = rig::tool::tool_definition(&fixture.tool(Some(1_000)));
    let b = rig::tool::tool_definition(&fixture.tool(None));
    assert_eq!(a.description, b.description);
    assert_eq!(a.parameters, b.parameters);
    for absent in ["window", "percent", "model", "limit_tokens"] {
        assert!(a.parameters["properties"].get(absent).is_none(), "{absent} is not a parameter");
    }
    // An argument that tries anyway is ignored (TLC-FR-07).
    fixture.file("doc.md", "x".repeat(1_000).as_bytes());
    let output = fixture
        .call(Some(1_000), serde_json::json!({ "path": "doc.md", "window": 1_000_000 }))
        .expect("measures");
    assert_eq!(output.limit_tokens, 300);
}

// MDT-FR-OTUZ: exactly one target; both, neither, or only blanks are refused.
#[test]
fn a_call_names_exactly_one_target() {
    let fixture = MeasureFixture::new();
    fixture.file("a.md", b"a");
    let id = fixture.draft("b");
    for args in [
        serde_json::json!({}),
        serde_json::json!({ "path": "a.md", "draft_id": id }),
        serde_json::json!({ "path": "  ", "draft_id": "" }),
    ] {
        let refusal = fixture.call(None, args).expect_err("refused");
        assert_eq!(refusal, ToolRefusal::InvalidArguments(crate::tools::MEASURE_NOT_ONE_TARGET));
        let error = refusal.to_execution_error();
        assert_eq!(error.retryable(), Some(true), "retryable");
    }
}

// MDT-FR-LWAK, MDT-FR-MYNF, MDT-FR-QYLP, MDT-FR-PPCH: the size of a file, and
// the five fields alone.
#[test]
fn a_file_is_measured_in_lines_characters_and_estimated_tokens() {
    let fixture = MeasureFixture::new();
    fixture.file("docs/hld.md", "один\r\ntwo\nthree".as_bytes());
    let output = fixture.call(None, serde_json::json!({ "path": "docs/hld.md" })).expect("ok");
    assert_eq!(output.lines, 3, "a last line without a terminator still counts");
    assert_eq!(output.characters, 15, "scalar values, with no line-ending normalisation");
    assert_eq!(output.estimated_tokens, 4, "15 / 4, rounded up");
    let json = serde_json::to_value(&output).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["above_limit", "characters", "estimated_tokens", "limit_tokens", "lines"]);

    fixture.file("empty.md", b"");
    let empty = fixture.call(None, serde_json::json!({ "path": "empty.md" })).expect("ok");
    assert_eq!((empty.lines, empty.characters, empty.estimated_tokens), (0, 0, 0));
}

// MDT-FR-CTKN, MDT-FR-FHWP: the limit is the window times the percent, rounded
// down; above it only when the estimate is greater.
#[test]
fn the_limit_and_whether_the_document_is_above_it() {
    let fixture = MeasureFixture::new();
    // 120 characters = 30 estimated tokens.
    fixture.file("doc.md", "x".repeat(120).as_bytes());
    let at = fixture.call(Some(100), serde_json::json!({ "path": "doc.md" })).expect("ok");
    assert_eq!(at.limit_tokens, 30);
    assert!(!at.above_limit, "equal is not above");
    let over = fixture.call(Some(99), serde_json::json!({ "path": "doc.md" })).expect("ok");
    assert_eq!(over.limit_tokens, 29, "99 × 30 / 100 rounded down");
    assert!(over.above_limit);
    // The percent is the turn's own, not a fixed 30.
    for (percent, limit) in [(1, 1), (7, 7), (100, 100)] {
        let args: MeasureDocumentArgs =
            serde_json::from_value(serde_json::json!({ "path": "doc.md" })).unwrap();
        let output = block_on(fixture.tool_at(100, percent).call(args)).expect("ok");
        assert_eq!(output.limit_tokens, limit, "{percent}%");
        assert_eq!(output.above_limit, 30 > limit, "{percent}%");
    }
    // MDT-FR-CTKN: the default window of AAP-FR-RANS gives a limit too.
    let default = fixture.call(None, serde_json::json!({ "path": "doc.md" })).expect("ok");
    assert_eq!(default.limit_tokens, 300_000);
    assert!(!default.above_limit);
    assert_eq!(default.estimated_tokens, 30);
}

// MDT-FR-TVWI: a draft is measured as its live prompt.
#[test]
fn a_draft_is_measured_as_its_prompt() {
    let fixture = MeasureFixture::new();
    let id = fixture.draft("one\ntwo\n");
    let output = fixture.call(None, serde_json::json!({ "draftId": id })).expect("ok");
    assert_eq!((output.lines, output.characters), (2, 8));
    let refusal =
        fixture.call(None, serde_json::json!({ "draft_id": "no-such-draft" })).expect_err("no");
    assert_eq!(refusal, ToolRefusal::DraftNotFound);
}

// MDT-FR-TVWI: a draft without its single live prompt is the retryable
// inconsistent-draft refusal, kind Other.
#[test]
fn a_draft_without_its_single_prompt_is_inconsistent() {
    let fixture = MeasureFixture::new();
    let id = fixture.draft("# Plan\n");
    let dir = crate::drafts::draft_dir(&fixture.root(), &id).expect("resolves");
    std::fs::write(dir.join("files").join("intruder.md"), "# second\n").expect("write");
    let refusal = fixture.call(None, serde_json::json!({ "draft_id": id })).expect_err("no");
    assert_eq!(refusal, ToolRefusal::MeasureDraftInconsistent);
    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), rig::tool::ToolErrorKind::Other);
    assert_eq!(error.retryable(), Some(true));
}

// MDT-FR-VHHI: a draft prompt that does not decode is the retryable unreadable
// refusal, kind Other.
#[test]
fn a_draft_prompt_that_is_not_text_is_unreadable() {
    let fixture = MeasureFixture::new();
    let id = fixture.draft("readable for now\n");
    let dir = crate::drafts::draft_dir(&fixture.root(), &id).expect("resolves");
    let prompt = std::fs::read_dir(dir.join("files")).unwrap().next().unwrap().unwrap().path();
    std::fs::write(prompt, [0xff, 0xfe, 0x00, 0x9f]).expect("write");
    let refusal = fixture.call(None, serde_json::json!({ "draft_id": id })).expect_err("no");
    assert_eq!(refusal, ToolRefusal::MeasureUnreadable);
    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), rig::tool::ToolErrorKind::Other);
    assert_eq!(error.retryable(), Some(true));
}

// MDT-FR-LWAK: the line count agrees with the ranges of `read_draft` — the
// last line is at offset `lines - 1`, and offset `lines` holds nothing.
#[test]
fn the_line_count_agrees_with_read_draft_ranges() {
    let fixture = MeasureFixture::new();
    let id = fixture.draft("one\ntwo\nthree");
    let measured = fixture.call(None, serde_json::json!({ "draft_id": id })).expect("ok");
    assert_eq!(measured.lines, 3);
    let reader = crate::tools::draft_read::DraftReadTool::new(
        fixture.fixture.handle(),
        fixture.session.clone(),
    );
    let read = |offset: u64| {
        let args: crate::tools::draft_read::ReadDraftArgs = serde_json::from_value(
            serde_json::json!({ "draft_id": id, "offset": offset, "limit": 1 }),
        )
        .unwrap();
        block_on(reader.call(args)).expect("reads").content
    };
    assert_eq!(read(measured.lines as u64 - 1), "three");
    assert_eq!(read(measured.lines as u64), "");
}

// MDT-FR-TWVZ: a path naming nothing, a directory, an absolute path, and an
// escape are each the retryable NotFound refusal.
#[test]
fn a_path_that_names_no_file_is_not_found() {
    let fixture = MeasureFixture::new();
    fixture.file("dir/inner.md", b"x");
    for path in ["missing.md", "dir", "/etc/passwd", "../outside.md"] {
        let refusal = fixture.call(None, serde_json::json!({ "path": path })).expect_err("refused");
        assert_eq!(refusal, ToolRefusal::MeasureFileNotFound, "{path}");
        let error = refusal.to_execution_error();
        assert_eq!(error.kind(), rig::tool::ToolErrorKind::NotFound);
        assert_eq!(error.retryable(), Some(true));
    }
}

// MDT-FR-VHHI: a file that does not decode is the retryable unreadable
// refusal, kind Other, and no byte of it reaches the message.
#[test]
fn a_file_that_is_not_text_is_unreadable() {
    let fixture = MeasureFixture::new();
    fixture.file("bin.dat", &[0xff, 0xfe, 0xfd]);
    let refusal = fixture.call(None, serde_json::json!({ "path": "bin.dat" })).expect_err("no");
    assert_eq!(refusal, ToolRefusal::MeasureUnreadable);
    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), rig::tool::ToolErrorKind::Other);
    assert_eq!(error.retryable(), Some(true));
}

// MDT-FR-OPKM: with no project open, the shared refusal.
#[test]
fn no_open_project_is_the_shared_refusal() {
    let app = crate::tools::tests::closed_project();
    let tool =
        MeasureDocumentTool::new(app.handle().clone(), "x", DocumentBudget::default_window());
    let args = MeasureDocumentArgs { path: Some("a.md".into()), draft_id: None };
    assert_eq!(block_on(tool.call(args)), Err(ToolRefusal::NoProjectOpen));
}

// MDT-FR-CGXO, CVL-FR-09: measuring changes nothing on disk.
#[test]
fn measuring_changes_nothing() {
    let fixture = MeasureFixture::new();
    fixture.file("doc.md", b"hello\n");
    let id = fixture.draft("prompt\n");
    let before = crate::tools::tests::tree_snapshot(fixture.root().path());
    fixture.call(None, serde_json::json!({ "path": "doc.md" })).expect("ok");
    fixture.call(None, serde_json::json!({ "draft_id": id })).expect("ok");
    assert_eq!(crate::tools::tests::tree_snapshot(fixture.root().path()), before);
}

static MDT_BUFFER: LogBuffer = LogBuffer::new();

// MDT-FR-DAVA, TLC-FR-14: an INFO record with the target and the estimate, a WARN record
// with the target and the reason, under `ai` and `backend`, and no text.
#[test]
fn each_call_is_logged_with_its_target_and_never_its_text() {
    let fixture = MeasureFixture::new();
    fixture.file("secret.md", b"the secret text of the document");
    let tool = MeasureDocumentTool::with_buffer(
        fixture.fixture.handle(),
        fixture.session.clone(),
        DocumentBudget::default_window(),
        &MDT_BUFFER,
    );
    let ok = MeasureDocumentArgs { path: Some("secret.md".into()), draft_id: None };
    block_on(tool.call(ok)).expect("measures");
    let bad = MeasureDocumentArgs { path: None, draft_id: Some("x".repeat(2_000)) };
    block_on(tool.call(bad)).expect_err("refused");

    let records = MDT_BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![crate::logging::Domain::Ai],
                ..LogFilter::default()
            },
            None,
            100,
        )
        .expect("answers")
        .records;
    let info = records.iter().find(|r| r.level == LogLevel::Info).expect("INFO");
    assert_eq!(info.fields["tool"], serde_json::json!(NAME));
    assert_eq!(info.fields["path"], serde_json::json!("secret.md"));
    assert_eq!(info.fields["estimated_tokens"], serde_json::json!(8));
    let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("WARN");
    assert_eq!(warn.fields["reason"], serde_json::json!("draft_not_found"));
    let logged = warn.fields["draft_id"].as_str().unwrap();
    assert!(logged.chars().count() <= 513, "bounded to 512 characters and an ellipsis");
    for record in &records {
        assert!(!serde_json::to_string(record).unwrap().contains("secret text"));
    }
    // The same records stand under `backend` too.
    let backend = MDT_BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![crate::logging::Domain::Backend],
                ..LogFilter::default()
            },
            None,
            100,
        )
        .expect("answers")
        .records;
    assert!(backend.iter().any(|r| r.level == LogLevel::Info && r.fields["tool"] == NAME));
    assert!(backend.iter().any(|r| r.level == LogLevel::Warn && r.fields["tool"] == NAME));
}
