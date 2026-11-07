//! Tests for the export requests of `DXP-document-export.md`.

use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::logging::{LogFilter, LogLevel};
use crate::tools::tests::mounted;

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

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = fs::RootFs::for_root(&path);
        let created = crate::drafts::create_draft_at_root(&root, Some("relay")).expect("draft");
        let thread = comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
            None,
            "@arch export the guide".into(),
            Vec::new(),
            comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture { open: mounted(dir), path, draft_id: created.draft.id, thread_id: thread.id }
    }

    fn root(&self) -> fs::RootFs {
        fs::RootFs::for_root(&self.path)
    }

    fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false)
    }

    fn record(&self, buffer: &'static LogBuffer) -> ExportRequest {
        let origin = self.origin();
        record_with(
            &self.open.handle(),
            buffer,
            &self.root(),
            ExportRequestInput {
                path: "docs/guide.md",
                format: Some("pdf"),
                message: MESSAGE,
                agent: &agent(),
                origin: &origin,
            },
        )
        .expect("recorded")
    }

    fn resolve(
        &self,
        buffer: &'static LogBuffer,
        id: &str,
        outcome: ExportOutcome,
    ) -> Result<ExportResolution, String> {
        resolve_export_request_impl(
            &self.open.handle(),
            buffer,
            &self.root(),
            id,
            outcome,
            &human(),
        )
    }

    fn thread(&self) -> comments::Discussion {
        comments::fold_discussion(&self.root(), &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("the thread")
    }

    fn listen(&self) -> Arc<Mutex<Vec<String>>> {
        let events: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = events.clone();
        self.open.app.listen(EXPORT_REQUEST_CHANGED, move |event| {
            sink.lock().unwrap().push(event.payload().to_string());
        });
        events
    }
}

// DXP-FR-YEAQ / DXP-FR-XZVZ / DXP-FR-NXGQ: a recorded request is a pending
// TOML record under `export-requests/`, served by its id.
#[test]
fn a_recorded_request_is_pending_and_served_by_its_id() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let request = f.record(&BUFFER);
    assert_eq!(request.state, ExportRequestState::Pending);
    assert!(f.path.join(REQUESTS_DIR).join(format!("{}.toml", request.id)).is_file());
    assert_eq!(get_export_request_impl(&f.root(), &request.id).unwrap(), request);
    // DXP contract surface: the wire shape.
    let wire = serde_json::to_value(&request).unwrap();
    assert_eq!(wire["state"], "pending");
    assert_eq!(wire["discussionId"], f.thread_id.as_str());
    assert_eq!(wire["target"]["kind"], "draft");
    assert_eq!(wire["format"], "pdf");
    assert!(wire.get("destination").is_none());
}

// DXP-FR-NXGQ: an unknown or malformed id is `request_not_found`.
#[test]
fn an_unknown_request_is_not_found() {
    let f = Fixture::new();
    assert_eq!(get_export_request_impl(&f.root(), "nope").unwrap_err(), ERR_REQUEST_NOT_FOUND);
    assert_eq!(get_export_request_impl(&f.root(), "../escape").unwrap_err(), ERR_REQUEST_NOT_FOUND);
}

// DXP-FR-LHAG / DXP-FR-VFFP / DXP-FR-FVTK: each outcome sets the state, appends
// one author comment that states it, returns its id, and emits the change.
#[test]
fn each_outcome_sets_the_state_and_appends_one_author_comment() {
    static BUFFER: LogBuffer = LogBuffer::new();
    for (outcome, state, body) in [
        (
            ExportOutcome::Exported { destination: "/tmp/out/guide.pdf".into() },
            ExportRequestState::Exported,
            "Exported “guide.md” to `/tmp/out/guide.pdf`.",
        ),
        (
            ExportOutcome::Cancelled,
            ExportRequestState::Cancelled,
            "Cancelled the export of “guide.md”.",
        ),
        (
            ExportOutcome::Failed { error: "write_failed".into() },
            ExportRequestState::Failed,
            "The export of “guide.md” failed: write_failed.",
        ),
    ] {
        let f = Fixture::new();
        let events = f.listen();
        let request = f.record(&BUFFER);
        let before = f.thread().comments.len();
        let resolution = f.resolve(&BUFFER, &request.id, outcome.clone()).expect("resolved");
        assert_eq!(resolution.request.state, state);
        assert!(resolution.request.resolved_at.is_some());
        let thread = f.thread();
        assert_eq!(thread.comments.len(), before + 1);
        let comment = thread.comments.last().unwrap();
        assert_eq!(Some(&comment.id), resolution.comment_id.as_ref());
        assert_eq!(comment.body, body);
        assert_eq!(comment.author, human());
        assert_eq!(get_export_request_impl(&f.root(), &request.id).unwrap(), resolution.request);
        match outcome {
            ExportOutcome::Exported { destination } => {
                assert_eq!(resolution.request.destination, Some(destination))
            }
            ExportOutcome::Failed { error } => assert_eq!(resolution.request.error, Some(error)),
            ExportOutcome::Cancelled => {}
        }
        let wire = serde_json::to_value(&resolution).unwrap();
        assert!(wire["commentId"].is_string());
        assert_eq!(wire["request"]["state"], state.as_str());
        // One event for the recording and one for the resolution.
        let seen = events.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert!(seen[1].contains(state.as_str()));
    }
}

// DXP-FR-LHAG: a request is resolved once.
#[test]
fn a_resolved_request_is_not_resolved_again() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let request = f.record(&BUFFER);
    f.resolve(&BUFFER, &request.id, ExportOutcome::Cancelled).unwrap();
    let before = f.thread().comments.len();
    let refused = f
        .resolve(&BUFFER, &request.id, ExportOutcome::Exported { destination: "/x.pdf".into() })
        .unwrap_err();
    assert_eq!(refused, ERR_ALREADY_RESOLVED);
    assert_eq!(f.thread().comments.len(), before);
    let stored = get_export_request_impl(&f.root(), &request.id).unwrap();
    assert_eq!(stored.state, ExportRequestState::Cancelled);
    assert_eq!(
        f.resolve(&BUFFER, "nope", ExportOutcome::Cancelled).unwrap_err(),
        ERR_REQUEST_NOT_FOUND
    );
}

// DXP-FR-FVTK: a resolved discussion takes no comment, and the state lands.
#[test]
fn a_resolved_discussion_takes_no_comment_and_the_state_lands() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let request = f.record(&BUFFER);
    comments::set_resolution_to(
        &f.root(),
        comments::ThreadRef::discussion(&f.draft_id),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .unwrap();
    let before = f.thread().comments.len();
    let resolution = f.resolve(&BUFFER, &request.id, ExportOutcome::Cancelled).unwrap();
    assert_eq!(resolution.comment_id, None);
    assert_eq!(resolution.request.state, ExportRequestState::Cancelled);
    assert_eq!(f.thread().comments.len(), before);
    assert!(serde_json::to_value(&resolution).unwrap().get("commentId").is_none());
}

// DXP-FR-LHAG: the wire outcome parses to the three outcomes, and an export
// with no destination or an unknown outcome is `invalid_outcome`.
#[test]
fn the_wire_outcome_parses() {
    assert_eq!(
        ExportOutcome::parse("exported", Some(" /a.pdf ".into()), None).unwrap(),
        ExportOutcome::Exported { destination: "/a.pdf".into() }
    );
    assert_eq!(ExportOutcome::parse("cancelled", None, None).unwrap(), ExportOutcome::Cancelled);
    assert_eq!(
        ExportOutcome::parse("failed", None, Some("boom".into())).unwrap(),
        ExportOutcome::Failed { error: "boom".into() }
    );
    assert_eq!(ExportOutcome::parse("exported", None, None).unwrap_err(), ERR_INVALID_OUTCOME);
    assert_eq!(ExportOutcome::parse("done", None, None).unwrap_err(), ERR_INVALID_OUTCOME);
}

// DXP-FR-IMSX: recording and resolving each log one `INFO` record with the id,
// the format, and the state, and none carries the path, the destination, the
// error text, or the message.
#[test]
fn the_log_carries_ids_and_kinds_only() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let request = f.record(&BUFFER);
    f.resolve(
        &BUFFER,
        &request.id,
        ExportOutcome::Exported { destination: "/secret/out.pdf".into() },
    )
    .unwrap();
    let records = BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Backend],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records;
    let infos: Vec<_> =
        records.iter().filter(|r| r.message.starts_with("export request")).collect();
    assert_eq!(infos.len(), 2);
    let text = serde_json::to_string(&records).unwrap();
    assert!(text.contains(&request.id));
    assert!(!text.contains("guide"));
    assert!(!text.contains("/secret"));
    assert!(!text.contains("Export the guide"));
}
