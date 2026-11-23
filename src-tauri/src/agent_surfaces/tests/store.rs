//! The agent surface records (AUI-FR-IAAP to AUI-FR-BPDT).

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::Listener;
use tempfile::TempDir;

use super::validate::{form, model};
use crate::agent_conversations::ConversationOrigin;
use crate::agent_surfaces::store::*;
use crate::agent_surfaces::*;
use crate::comments::{self, Attachment, Participant};
use crate::fs;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::mounted;

const MESSAGE: &str = "Pick a storage. <spoken>Pick a storage.</spoken>";

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
            &comments::DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
            None,
            "@arch which storage?".into(),
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

    fn record(&self, buffer: &'static LogBuffer) -> AgentSurface {
        let origin = self.origin();
        let components = form();
        record_with(
            &self.open.handle(),
            buffer,
            &self.root(),
            SurfaceInput {
                components: &components,
                data_model: &model(),
                message: MESSAGE,
                agent: &agent(),
                origin: &origin,
            },
        )
        .expect("recorded")
    }

    fn submit(
        &self,
        buffer: &'static LogBuffer,
        id: &str,
        action: SurfaceAction,
    ) -> Result<SurfaceSubmitResult, String> {
        submit_impl(&self.open.handle(), buffer, &self.root(), id, action, &human())
    }

    fn thread(&self) -> comments::Discussion {
        comments::fold_discussion(&self.root(), &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("the thread")
    }

    fn resolve_thread(&self) {
        comments::set_resolution_to(
            &self.root(),
            comments::ThreadRef::discussion(&self.draft_id),
            &self.thread_id,
            true,
            &human(),
            "2026-01-02T00:00:00Z",
        )
        .unwrap();
    }

    fn listen(&self) -> Arc<Mutex<Vec<Value>>> {
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        let sink = events.clone();
        self.open.app.listen(AGENT_SURFACE_CHANGED, move |event| {
            sink.lock().unwrap().push(serde_json::from_str(event.payload()).unwrap());
        });
        events
    }

    fn records_left(&self) -> usize {
        std::fs::read_dir(self.path.join(SURFACES_DIR)).map(|d| d.count()).unwrap_or(0)
    }
}

/// The action that the Send button of `form()` sends.
fn send() -> SurfaceAction {
    SurfaceAction {
        name: "choose_storage".into(),
        source_component_id: "send".into(),
        context: json!({"choice": ["sqlite"], "note": "fast", "kind": "storage"}),
        data_model: json!({"choice": ["sqlite"], "note": "fast"}),
    }
}

fn records_of(buffer: &LogBuffer) -> Vec<crate::logging::LogRecord> {
    let filter = LogFilter {
        min_level: LogLevel::Debug,
        domains: vec![Domain::Backend],
        ..LogFilter::default()
    };
    buffer.query(&filter, None, 1000).unwrap().records
}

fn failing_append(_: &fs::RootFs, _: CommentWrite<'_>) -> Result<comments::Discussion, String> {
    Err("io error: disk full at /secret/path".to_string())
}

// AUI-FR-IAAP / AUI-FR-DGUP / AUI-FR-QRCX / AUI-FR-QBXZ / CMS-FR-RWQQ: a
// recorded surface is an `open` JSON record under `agent-surfaces/`, served by
// its id, and one agent comment carries its one `agent_surface` attachment.
#[test]
fn a_recorded_surface_is_open_and_carried_by_one_agent_comment() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let events = f.listen();
    let before = f.thread().comments.len();
    let surface = f.record(&BUFFER);
    assert_eq!(surface.state, SurfaceState::Open);
    assert_eq!(surface.catalog_id, CATALOG_ID);
    assert_eq!(surface.components, form());
    assert_eq!(surface.data_model, model());
    assert_eq!(surface.authored_by, agent());
    assert!(f.path.join(SURFACES_DIR).join(format!("{}.json", surface.id)).is_file());
    assert_eq!(store::get_impl(&f.root(), &surface.id).unwrap(), surface);

    let thread = f.thread();
    assert_eq!(thread.comments.len(), before + 1);
    let comment = thread.comments.last().unwrap();
    assert_eq!(comment.id, surface.comment_id);
    assert_eq!(comment.author, agent());
    assert_eq!(comment.body, MESSAGE);
    assert_eq!(
        comment.attachments,
        vec![Attachment::AgentSurface { surface_id: surface.id.clone() }]
    );
    // AUI contract surface and CMS-FR-RWQQ: the wire shapes.
    let wire = serde_json::to_value(&surface).unwrap();
    assert_eq!(wire["state"], "open");
    assert_eq!(wire["discussionId"], f.thread_id.as_str());
    assert_eq!(wire["catalogId"], "synthesis:discussion/v1");
    assert!(wire.get("submission").is_none());
    let attachment = serde_json::to_value(&comment.attachments[0]).unwrap();
    assert_eq!(attachment, json!({"kind": "agentSurface", "surfaceId": surface.id}));
    // AUI-FR-WDZG.
    assert_eq!(events.lock().unwrap().len(), 1);
    // AUI-FR-DGUP: a second record has an id of its own.
    assert_ne!(f.record(&BUFFER).id, surface.id);
}

// AUI-FR-ZHXO: an invalid surface writes nothing and appends nothing.
#[test]
fn an_invalid_surface_writes_nothing() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let before = f.thread().comments.len();
    let origin = f.origin();
    let components = vec![json!({"id":"root","component":"Modal"})];
    let refused = record_with(
        &f.open.handle(),
        &BUFFER,
        &f.root(),
        SurfaceInput {
            components: &components,
            data_model: &json!({}),
            message: MESSAGE,
            agent: &agent(),
            origin: &origin,
        },
    )
    .unwrap_err();
    assert!(matches!(refused, RecordError::Invalid(ref v) if v.rule == "unknown_component"));
    assert_eq!(f.records_left(), 0);
    assert_eq!(f.thread().comments.len(), before);
}

// AUI-FR-ZHXO: a failed append removes the record, so neither remains, and
// the log carries the typed code alone.
#[test]
fn a_failed_append_removes_the_record() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let events = f.listen();
    let before = f.thread().comments.len();
    let origin = f.origin();
    let components = form();
    let refused = record_using(
        &f.open.handle(),
        &BUFFER,
        &f.root(),
        SurfaceInput {
            components: &components,
            data_model: &model(),
            message: MESSAGE,
            agent: &agent(),
            origin: &origin,
        },
        &failing_append,
    )
    .unwrap_err();
    assert_eq!(refused, RecordError::WriteFailed);
    assert_eq!(f.records_left(), 0, "no record remains");
    assert_eq!(f.thread().comments.len(), before, "no comment remains");
    assert!(events.lock().unwrap().is_empty(), "nothing is announced");
    let text = serde_json::to_string(&records_of(&BUFFER)).unwrap();
    assert!(text.contains(ERR_WRITE_FAILED));
    assert!(!text.contains("/secret"), "no file system message: {text}");
}

// AUI-FR-QRCX: a resolved discussion takes no surface.
#[test]
fn a_resolved_discussion_takes_no_surface() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    f.resolve_thread();
    let origin = f.origin();
    let components = form();
    let refused = record_with(
        &f.open.handle(),
        &BUFFER,
        &f.root(),
        SurfaceInput {
            components: &components,
            data_model: &model(),
            message: MESSAGE,
            agent: &agent(),
            origin: &origin,
        },
    )
    .unwrap_err();
    assert_eq!(refused, RecordError::DiscussionResolved);
    assert_eq!(f.records_left(), 0);
}

// AUI-FR-QBXZ: an unknown or a malformed id is `surface_not_found`.
#[test]
fn an_unknown_surface_is_not_found() {
    let f = Fixture::new();
    assert_eq!(store::get_impl(&f.root(), "nope").unwrap_err(), ERR_SURFACE_NOT_FOUND);
    assert_eq!(store::get_impl(&f.root(), "../escape").unwrap_err(), ERR_SURFACE_NOT_FOUND);
    assert_eq!(store::get_impl(&f.root(), "").unwrap_err(), ERR_SURFACE_NOT_FOUND);
}

// AUI-FR-KBVY / AUI-FR-AIRH / AUI-FR-MAXD / AUI-FR-XMUS / AUI-FR-WDZG: an
// accepted action submits the record, appends one author comment that names
// the button and each context value, returns its id, and emits the change.
#[test]
fn an_accepted_action_submits_and_appends_one_author_comment() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    let events = f.listen();
    let before = f.thread().comments.len();
    let result = f.submit(&BUFFER, &surface.id, send()).expect("accepted");
    assert_eq!(result.surface.state, SurfaceState::Submitted);
    let submission = result.surface.submission.as_ref().expect("submission");
    assert_eq!(submission.name, "choose_storage");
    assert_eq!(submission.source_component_id, "send");
    assert_eq!(submission.context, send().context);
    assert_eq!(submission.data_model, send().data_model);
    assert_eq!(submission.by, human());
    assert!(submission.at.ends_with('Z') && submission.at.contains('T'), "{}", submission.at);
    assert_eq!(submission.comment_id, result.comment_id);
    assert_eq!(store::get_impl(&f.root(), &surface.id).unwrap(), result.surface);

    let thread = f.thread();
    assert_eq!(thread.comments.len(), before + 1);
    let comment = thread.comments.last().unwrap();
    assert_eq!(comment.id, result.comment_id);
    assert_eq!(comment.author, human());
    assert!(comment.attachments.is_empty());
    assert_eq!(comment.body, "Sent “Send”.\n- choice: [\"sqlite\"]\n- note: fast\n- kind: storage");
    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(wire["commentId"], result.comment_id.as_str());
    assert_eq!(wire["surface"]["submission"]["sourceComponentId"], "send");
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["state"], "submitted");
}

// AUI-FR-XMUS / AUI-FR-CGDQ: the body names the button and each value on its
// own line, white space collapsed and each part bounded.
#[test]
fn the_body_is_one_line_per_value_and_bounded() {
    use crate::agent_surfaces::actions::action_body;
    assert_eq!(action_body("go", &[]), "Sent “go”.");
    let lines = vec![
        ("ok".to_string(), "true".to_string()),
        ("s".to_string(), "two\n\nlines  and\ttabs".to_string()),
    ];
    assert_eq!(
        action_body("Send\nnow", &lines),
        "Sent “Send now”.\n- ok: true\n- s: two lines and tabs"
    );
    let long = action_body(&"a".repeat(200), &[("v".to_string(), "b".repeat(900))]);
    let first = long.lines().next().unwrap();
    assert_eq!(first.chars().count(), "Sent “”.".chars().count() + 80);
    assert!(first.contains('…'));
    let value = long.lines().nth(1).unwrap().trim_start_matches("- v: ");
    assert_eq!(value.chars().count(), 500);
}

// AUI-FR-XMUS: a button whose Text child is not a literal is named by its
// event name.
#[test]
fn a_button_whose_label_is_not_literal_is_named_by_its_event() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let origin = f.origin();
    let mut components = form();
    let label = components.iter_mut().find(|c| c["id"] == "send_label").unwrap();
    label["text"] = json!({"@path": "/label"});
    let surface = record_with(
        &f.open.handle(),
        &BUFFER,
        &f.root(),
        SurfaceInput {
            components: &components,
            data_model: &model(),
            message: MESSAGE,
            agent: &agent(),
            origin: &origin,
        },
    )
    .unwrap();
    f.submit(&BUFFER, &surface.id, send()).expect("accepted");
    let body = f.thread().comments.last().unwrap().body.clone();
    assert!(body.starts_with("Sent “choose_storage”."), "{body}");
}

// AUI-FR-AFKN: a bound context value equals the data model at its pointer,
// or null where the model has none.
#[test]
fn a_bound_value_must_match_the_data_model() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    let mut forged = send();
    forged.context["note"] = json!("not what was typed");
    assert_eq!(f.submit(&BUFFER, &surface.id, forged).unwrap_err(), ERR_INVALID_CONTEXT);
    let mut absent = send();
    absent.data_model.as_object_mut().unwrap().remove("note");
    absent.context["note"] = json!(null);
    f.submit(&BUFFER, &surface.id, absent).expect("null where the model has no value");
}

// AUI-FR-BPDT: with no project open, both operations fail with
// `no_project_open`.
#[test]
fn no_open_project_is_refused() {
    use crate::agent_surfaces::commands::{get_with, open_store};
    assert_eq!(get_with(None, "s-1").unwrap_err(), ERR_NO_PROJECT_OPEN);
    assert_eq!(open_store(None).unwrap_err(), ERR_NO_PROJECT_OPEN);
}

// AUI-FR-GTXG: an action on a surface whose discussion is gone is refused.
#[test]
fn an_action_whose_discussion_is_gone_is_refused() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let mut surface = f.record(&BUFFER);
    surface.discussion_id = "gone".into();
    let path = f.path.join(SURFACES_DIR).join(format!("{}.json", surface.id));
    std::fs::write(&path, serde_json::to_string(&surface).unwrap()).unwrap();
    assert_eq!(f.submit(&BUFFER, &surface.id, send()).unwrap_err(), ERR_DISCUSSION_RESOLVED);
    assert_eq!(store::get_impl(&f.root(), &surface.id).unwrap().state, SurfaceState::Open);
}

// AUI-FR-KBVY / AUI-FR-EGAU: a second action is `already_submitted`, and
// nothing changes.
#[test]
fn a_surface_is_submitted_once() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    let first = f.submit(&BUFFER, &surface.id, send()).expect("accepted");
    let before = f.thread().comments.len();
    let mut again = send();
    again.context["note"] = json!("other");
    assert_eq!(f.submit(&BUFFER, &surface.id, again).unwrap_err(), ERR_ALREADY_SUBMITTED);
    assert_eq!(f.thread().comments.len(), before);
    assert_eq!(store::get_impl(&f.root(), &surface.id).unwrap(), first.surface);
    assert_eq!(f.submit(&BUFFER, "nope", send()).unwrap_err(), ERR_SURFACE_NOT_FOUND);
}

// AUI-FR-EGAU: of two actions sent at once on one open surface, one is
// accepted and the other is `already_submitted`.
#[test]
fn two_actions_at_once_submit_one() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    let before = f.thread().comments.len();
    let barrier = std::sync::Barrier::new(2);
    let outcomes: Vec<Result<SurfaceSubmitResult, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let (app, root, id, barrier) =
                    (f.open.handle(), f.root(), surface.id.clone(), &barrier);
                scope.spawn(move || {
                    barrier.wait();
                    submit_impl(&app, &BUFFER, &root, &id, send(), &human())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(f.thread().comments.len(), before + 1, "one author comment");
    assert_eq!(outcomes.iter().filter(|o| o.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| o.as_ref().err().map(String::as_str) == Some(ERR_ALREADY_SUBMITTED))
            .count(),
        1
    );
}

// AUI-FR-KBVY: an action that names no Button of the surface, or a Button
// with another event name, is `unknown_action`.
#[test]
fn an_action_that_names_no_button_is_unknown() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    for (name, source) in [("choose_storage", "q"), ("other", "send"), ("choose_storage", "ghost")]
    {
        let mut action = send();
        action.name = name.into();
        action.source_component_id = source.into();
        assert_eq!(f.submit(&BUFFER, &surface.id, action).unwrap_err(), ERR_UNKNOWN_ACTION);
    }
    assert_eq!(store::get_impl(&f.root(), &surface.id).unwrap().state, SurfaceState::Open);
}

// AUI-FR-AFKN: the context carries exactly the declared keys and the declared
// literals, within the byte bound, beside an object data model.
#[test]
fn a_context_that_does_not_match_the_button_is_invalid() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    let mut cases = Vec::new();
    let mut missing = send();
    missing.context.as_object_mut().unwrap().remove("note");
    cases.push(missing);
    let mut extra = send();
    extra.context["sneaky"] = json!(1);
    cases.push(extra);
    let mut literal = send();
    literal.context["kind"] = json!("forged");
    cases.push(literal);
    let mut not_object = send();
    not_object.context = json!(["choice"]);
    cases.push(not_object);
    let mut model_not_object = send();
    model_not_object.data_model = json!("x");
    cases.push(model_not_object);
    let mut too_big = send();
    too_big.data_model = json!({"big": "a".repeat(32 * 1024)});
    cases.push(too_big);
    for action in cases {
        assert_eq!(f.submit(&BUFFER, &surface.id, action).unwrap_err(), ERR_INVALID_CONTEXT);
    }
    assert_eq!(store::get_impl(&f.root(), &surface.id).unwrap().state, SurfaceState::Open);
}

// AUI-FR-GTXG: an action on a surface whose discussion is resolved is
// `discussion_resolved`, and nothing changes.
#[test]
fn an_action_in_a_resolved_discussion_is_refused() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    f.resolve_thread();
    let before = f.thread().comments.len();
    assert_eq!(f.submit(&BUFFER, &surface.id, send()).unwrap_err(), ERR_DISCUSSION_RESOLVED);
    assert_eq!(f.thread().comments.len(), before);
    assert_eq!(store::get_impl(&f.root(), &surface.id).unwrap().state, SurfaceState::Open);
}

// AUI-FR-FWQX: a failed append of the action comment puts the record back to
// `open`, and the error is `comment_failed`.
#[test]
fn a_failed_action_comment_keeps_the_surface_open() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    let events = f.listen();
    let refused = submit_using(
        &f.open.handle(),
        &BUFFER,
        &f.root(),
        &surface.id,
        send(),
        &human(),
        &failing_append,
    )
    .unwrap_err();
    assert_eq!(refused, ERR_COMMENT_FAILED);
    let stored = store::get_impl(&f.root(), &surface.id).unwrap();
    assert_eq!(stored, surface, "the record is back to open with no submission");
    assert!(events.lock().unwrap().is_empty());
    // The surface still takes the action afterwards.
    f.submit(&BUFFER, &surface.id, send()).expect("accepted");
}

// AUI-FR-XSAO / AUI-FR-DNZK: the log names ids, counts, and typed errors, and
// no component, text, context value, data model, or message.
#[test]
fn the_log_carries_ids_and_counts_only() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    let surface = f.record(&BUFFER);
    let mut bad = send();
    bad.name = "other".into();
    let _ = f.submit(&BUFFER, &surface.id, bad);
    f.submit(&BUFFER, &surface.id, send()).expect("accepted");
    let records = records_of(&BUFFER);
    let recorded =
        records.iter().find(|r| r.message == "agent surface recorded").expect("recorded");
    assert_eq!(recorded.level, LogLevel::Info);
    assert_eq!(recorded.fields["surfaceId"], surface.id.as_str());
    assert_eq!(recorded.fields["components"], 6);
    assert_eq!(recorded.fields["discussionId"], f.thread_id.as_str());
    let accepted =
        records.iter().find(|r| r.message == "agent surface action accepted").expect("accepted");
    assert_eq!(accepted.fields["surfaceId"], surface.id.as_str());
    let refused =
        records.iter().find(|r| r.message == "agent surface action refused").expect("refused");
    assert_eq!(refused.level, LogLevel::Warn);
    assert_eq!(refused.fields["error"], ERR_UNKNOWN_ACTION);
    let text = serde_json::to_string(&records).unwrap();
    for secret in ["Which storage", "sqlite", "fast", "Pick a storage", "choose_storage"] {
        assert!(!text.contains(secret), "the log carries `{secret}`: {text}");
    }
}
