//! Tests for `RDX-request-document-export-tool.md`.

use rig::tool::{PortableTool, ToolErrorKind};
use tempfile::TempDir;

use super::*;
use crate::comments::{Attachment, DiscussionTarget};
use crate::document_export::requests::{get_export_request_impl, ExportRequestState};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted};

const MESSAGE: &str = "Please export the guide. <spoken>Export the guide.</spoken>";

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: Some("Developer".into()),
    }
}

type Mock = tauri::test::MockRuntime;

/// A resolver that stands in for the project's artifact types: `docs/guide.md`
/// and `doc-1` are DOC artifacts, and each other name gives one typed error.
fn stub_resolve(_: &tauri::AppHandle<Mock>, identifier: &str) -> Result<String, &'static str> {
    match identifier {
        "docs/guide.md" | "doc-1" => Ok("docs/guide.md".to_string()),
        "notes.md" => Err(dxp::ERR_NOT_DOC_ARTIFACT),
        "docs/guide.pdf" => Err(dxp::ERR_NOT_MARKDOWN),
        "doc-outside" => Err(dxp::ERR_OUTSIDE_PROJECT),
        _ => Err(dxp::ERR_NOT_FOUND),
    }
}

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
    requested: Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let created = crate::drafts::create_draft_at_root(&root, Some("relay")).expect("draft");
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
            None,
            "@arch export the guide".into(),
            Vec::new(),
            crate::comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture {
            open: mounted(dir),
            path,
            draft_id: created.draft.id,
            thread_id: thread.id,
            requested: Arc::new(AtomicBool::new(false)),
        }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    fn tool(&self) -> RequestDocumentExportTool<Mock> {
        RequestDocumentExportTool::new(
            self.open.handle(),
            crate::agent_conversations::OwnedRoots { worktree: self.root(), store: self.root() },
            ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false),
            agent(),
            Arc::clone(&self.requested),
        )
        .with_resolver(stub_resolve)
    }

    fn call(&self, args: serde_json::Value) -> Result<RequestDocumentExportOutput, ToolRefusal> {
        block_on(self.tool().call(serde_json::from_value(args).expect("args")))
    }

    fn thread(&self) -> crate::comments::Discussion {
        crate::comments::fold_discussion(&self.root(), &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("the thread")
    }
}

fn request(document: &str, format: Option<&str>) -> serde_json::Value {
    let mut args = serde_json::json!({ "document": document, "message": MESSAGE });
    if let Some(format) = format {
        args["format"] = serde_json::json!(format);
    }
    args
}

fn records(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .expect("the buffer answers")
        .records
}

// RDX-FR-UBTA / RDX-FR-ORZH / RDX-FR-EDQY: a portable tool with the fixed
// description and a schema that takes no destination.
#[test]
fn it_is_a_portable_tool_with_the_fixed_description_and_no_destination() {
    let f = Fixture::new();
    let tool = f.tool();
    assert_eq!(<RequestDocumentExportTool<Mock> as PortableTool>::NAME, "request_document_export");
    assert_eq!(tool.description(), DESCRIPTION);
    let schema = tool.parameters();
    let properties = schema["properties"].as_object().expect("properties");
    let mut names: Vec<&str> = properties.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, ["document", "format", "message"]);
    assert_eq!(schema["required"], serde_json::json!(["document", "message"]));
    assert_eq!(schema["properties"]["format"]["enum"], serde_json::json!(["pdf", "docx"]));
}

// RDX-FR-TMYF / RDX-FR-PZGN / CMS-FR-OKDW / DXP-FR-XZVZ: a call records one
// pending request and one agent comment that carries the `export_request`
// attachment, returns `{ requested: true }`, and sets the turn-ending flag.
#[test]
fn a_request_records_one_request_and_one_comment_and_ends_the_turn() {
    let f = Fixture::new();
    let before = f.thread().comments.len();
    let output = f.call(request("doc-1", Some("PDF"))).expect("requested");
    assert_eq!(output, RequestDocumentExportOutput { requested: true });
    assert!(f.requested.load(Ordering::SeqCst));
    assert_eq!(serde_json::to_value(&output).unwrap(), serde_json::json!({ "requested": true }));

    let thread = f.thread();
    assert_eq!(thread.comments.len(), before + 1);
    let comment = thread.comments.last().unwrap();
    assert_eq!(comment.body, MESSAGE);
    assert_eq!(comment.author, agent());
    let [Attachment::ExportRequest { request_id, path, format }] = comment.attachments.as_slice()
    else {
        panic!("one export request attachment: {:?}", comment.attachments);
    };
    assert_eq!(path, "docs/guide.md");
    assert_eq!(format.as_deref(), Some("pdf"));

    let stored = get_export_request_impl(&f.root(), request_id).expect("the record");
    assert_eq!(stored.state, ExportRequestState::Pending);
    assert_eq!(stored.path, "docs/guide.md");
    assert_eq!(stored.discussion_id, f.thread_id);
    assert_eq!(stored.target, DiscussionTarget::Draft { draft_id: f.draft_id.clone() });
}

// RDX-FR-POVJ / CMS-FR-OKDW: a call with no format records none.
#[test]
fn a_request_without_a_format_records_none() {
    let f = Fixture::new();
    f.call(request("docs/guide.md", None)).expect("requested");
    let thread = f.thread();
    let Some(Attachment::ExportRequest { format, .. }) =
        thread.comments.last().and_then(|c| c.attachments.first())
    else {
        panic!("an export request attachment");
    };
    assert_eq!(format, &None);
    // CMS-FR-OKDW: the wire shape the frontend reads.
    let wire =
        serde_json::to_value(thread.comments.last().unwrap().attachments[0].clone()).unwrap();
    assert_eq!(wire["kind"], "exportRequest");
    assert_eq!(wire["path"], "docs/guide.md");
    assert!(wire.get("format").is_none());
    assert!(wire["requestId"].is_string());
}

// RDX-FR-DVQL / RDX-FR-POVJ / RDX-FR-AROQ: each malformed argument is its own
// retryable refusal, and nothing is recorded.
#[test]
fn malformed_arguments_are_refused_and_record_nothing() {
    let f = Fixture::new();
    let before = f.thread().comments.len();
    let cases = [
        (serde_json::json!({ "document": "  ", "message": MESSAGE }), BLANK_DOCUMENT),
        (request("docs/guide.md", Some("html")), BAD_FORMAT),
        (serde_json::json!({ "document": "docs/guide.md", "message": " " }), BLANK_MESSAGE),
        (
            serde_json::json!({ "document": "docs/guide.md", "message": "Export it." }),
            NO_SPOKEN_PART,
        ),
    ];
    for (args, message) in cases {
        let refused = f.call(args).unwrap_err();
        assert_eq!(refused.to_string(), message);
        assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
        assert!(refused.retryable());
    }
    assert_eq!(f.thread().comments.len(), before);
    assert!(!f.requested.load(Ordering::SeqCst));
}

// RDX-FR-DVQL / RDX-FR-RRJQ: the typed errors of the resolution map to the
// refusals of the contract surface.
#[test]
fn the_resolution_errors_map_to_their_refusals() {
    let f = Fixture::new();
    let not_found = f.call(request("missing.md", None)).unwrap_err();
    assert_eq!(not_found.to_string(), DOCUMENT_NOT_FOUND);
    assert_eq!(not_found.kind(), ToolErrorKind::NotFound);
    assert!(not_found.retryable());
    for (document, message) in [
        ("notes.md", NOT_A_DOC_ARTIFACT),
        ("docs/guide.pdf", NOT_MARKDOWN),
        ("doc-outside", OUTSIDE_PROJECT),
    ] {
        let refused = f.call(request(document, None)).unwrap_err();
        assert_eq!(refused.to_string(), message);
        assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
        assert!(refused.retryable());
    }
    assert!(!f.requested.load(Ordering::SeqCst));
}

// RDX-FR-NOBI: a resolved conversation is not retryable, and nothing is
// recorded.
#[test]
fn a_resolved_conversation_is_refused_and_not_retryable() {
    let f = Fixture::new();
    crate::comments::set_resolution_to(
        &f.root(),
        crate::comments::ThreadRef::discussion(&f.draft_id),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    let refused = f.call(request("docs/guide.md", None)).unwrap_err();
    assert_eq!(refused.to_string(), CONVERSATION_RESOLVED);
    assert_eq!(refused.kind(), ToolErrorKind::PermissionDenied);
    assert!(!refused.retryable());
    assert!(f.root().list_dir(f.path.join("export-requests")).unwrap_or_default().is_empty());
}

// RDX-FR-IDWS: a conversation that can not be found is the could-not-record
// refusal, and no record remains.
#[test]
fn a_missing_conversation_is_not_recorded() {
    let f = Fixture::new();
    let tool = RequestDocumentExportTool::new(
        f.open.handle(),
        crate::agent_conversations::OwnedRoots { worktree: f.root(), store: f.root() },
        ConversationOrigin::stub_draft("no-such-thread", f.draft_id.clone(), false),
        agent(),
        Arc::clone(&f.requested),
    )
    .with_resolver(stub_resolve);
    let refused =
        block_on(tool.call(serde_json::from_value(request("doc-1", None)).unwrap())).unwrap_err();
    assert_eq!(refused.to_string(), NOT_RECORDED);
    assert_eq!(refused.kind(), ToolErrorKind::Other);
    assert!(refused.retryable());
    assert!(f.root().list_dir(f.path.join("export-requests")).unwrap_or_default().is_empty());
}

// RDX-FR-DFXB: with no project open the call is the shared refusal.
#[test]
fn no_open_project_is_the_shared_refusal() {
    let app = closed_project();
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    let tool = RequestDocumentExportTool::new(
        app.handle().clone(),
        crate::agent_conversations::OwnedRoots { worktree: root.clone(), store: root },
        ConversationOrigin::stub_draft("t", "d", false),
        agent(),
        Arc::new(AtomicBool::new(false)),
    )
    .with_resolver(stub_resolve);
    let refused =
        block_on(tool.call(serde_json::from_value(request("doc-1", None)).unwrap())).unwrap_err();
    assert!(matches!(refused, ToolRefusal::NoProjectOpen));
}

// RDX-FR-IEQA: a success and a refusal each log once under `ai`, and no
// record carries the document or the message.
#[test]
fn the_log_carries_no_document_and_no_message() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let tool = f.tool().with_buffer(&BUFFER);
    block_on(tool.call(serde_json::from_value(request("doc-1", Some("docx"))).unwrap()))
        .expect("requested");
    let tool = f.tool().with_buffer(&BUFFER);
    block_on(tool.call(serde_json::from_value(request("doc-1", Some("odt"))).unwrap()))
        .unwrap_err();
    let all = records(&BUFFER);
    let text = serde_json::to_string(&all).unwrap();
    assert!(all.iter().any(|r| r.level == LogLevel::Info));
    assert!(all.iter().any(|r| r.level == LogLevel::Warn));
    assert!(text.contains("\"format\":\"docx\""));
    assert!(text.contains("bad_format"));
    assert!(!text.contains("doc-1"));
    assert!(!text.contains("guide"));
    assert!(!text.contains("Export the guide"));
}

/// Every file below `dir`, relative to it, with its bytes.
fn files_of(dir: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(
        base: &std::path::Path,
        dir: &std::path::Path,
        out: &mut std::collections::BTreeMap<String, Vec<u8>>,
    ) {
        for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap_or_default());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

// RDX-FR-LNCQ: a request writes the request record and appends to the
// conversation's log, and changes no other file.
#[test]
fn a_request_writes_only_the_record_and_the_comment() {
    let f = Fixture::new();
    let before = files_of(&f.path);
    f.call(request("doc-1", Some("pdf"))).expect("requested");
    let after = files_of(&f.path);
    let changed: Vec<&String> = after
        .iter()
        .filter(|(name, bytes)| before.get(*name) != Some(*bytes))
        .map(|(name, _)| name)
        .collect();
    assert!(before.keys().all(|name| after.contains_key(name)), "nothing is removed");
    assert_eq!(changed.len(), 2, "the record and the log: {changed:?}");
    assert!(changed.iter().any(|name| name.contains("export-requests/")), "{changed:?}");
    assert!(changed.iter().any(|name| name.ends_with(".jsonl")), "{changed:?}");
}
