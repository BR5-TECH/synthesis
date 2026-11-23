//! Tests for `SSF-show-surface-tool.md`.

use rig::tool::{PortableTool, ToolErrorKind};
use serde_json::{json, Value};
use tempfile::TempDir;

use super::*;
use crate::agent_surfaces::{SurfaceState, SURFACES_DIR};
use crate::comments::{Attachment, DiscussionTarget};
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted};

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

type Mock = tauri::test::MockRuntime;

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
    asked: Arc<AtomicBool>,
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
            "@arch which storage?".into(),
            Vec::new(),
            crate::comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture {
            open: mounted(dir),
            path,
            draft_id: created.draft.id,
            thread_id: thread.id,
            asked: Arc::new(AtomicBool::new(false)),
        }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(&self.path)
    }

    fn tool_in(&self, thread_id: &str) -> ShowSurfaceTool<Mock> {
        ShowSurfaceTool::new(
            self.open.handle(),
            crate::agent_conversations::OwnedRoots { worktree: self.root(), store: self.root() },
            ConversationOrigin::stub_draft(thread_id.to_string(), self.draft_id.clone(), false),
            agent(),
            Arc::clone(&self.asked),
        )
    }

    fn tool(&self) -> ShowSurfaceTool<Mock> {
        self.tool_in(&self.thread_id)
    }

    fn call(&self, args: Value) -> Result<ShowSurfaceOutput, ToolRefusal> {
        block_on(self.tool().call(serde_json::from_value(args).expect("args")))
    }

    fn thread(&self) -> crate::comments::Discussion {
        crate::comments::fold_discussion(&self.root(), &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("the thread")
    }

    fn records_left(&self) -> usize {
        std::fs::read_dir(self.path.join(SURFACES_DIR)).map(|d| d.count()).unwrap_or(0)
    }
}

fn panel() -> Value {
    json!([
        {"id":"root","component":"Column","children":["q","pick","send"]},
        {"id":"q","component":"Text","text":"Which storage do you want?"},
        {"id":"pick","component":"ChoicePicker","options":[{"label":"SQLite","value":"sqlite"}],"value":{"@path":"/choice"}},
        {"id":"send","component":"Button","child":"l","action":{"event":{"name":"choose","context":{"choice":{"@path":"/choice"}}}}},
        {"id":"l","component":"Text","text":"Send"}
    ])
}

fn args() -> Value {
    json!({ "message": MESSAGE, "components": panel(), "data_model": {"choice": []} })
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

// SSF-FR-CETL / SSF-FR-FKWS: a portable tool whose description is the fixed
// text, the compact catalog, and the catalog's instructions.
#[test]
fn it_is_a_portable_tool_with_the_description_and_the_catalog() {
    let f = Fixture::new();
    let tool = f.tool();
    assert_eq!(<ShowSurfaceTool<Mock> as PortableTool>::NAME, "show_surface");
    let description = tool.description();
    assert!(description.starts_with(DESCRIPTION));
    assert!(description.contains(&compact_catalog()));
    assert!(description.ends_with(crate::agent_surfaces::catalog().instructions()));
}

// SSF-FR-XKQA: one line for each component and each function, with the
// required properties marked.
#[test]
fn the_compact_catalog_has_a_line_for_each_component_and_function() {
    let compact = compact_catalog();
    let lines: Vec<&str> = compact.lines().collect();
    assert_eq!(lines.len(), 19 + 11, "{compact}");
    let text = lines.iter().find(|l| l.starts_with("Text: ")).expect("Text");
    for part in ["id!", "component!", "text!", "variant", "weight", "@accessibility"] {
        assert!(
            text.split(", ").any(|p| p.trim_start_matches("Text: ") == part),
            "{text} has {part}"
        );
    }
    let button = lines.iter().find(|l| l.starts_with("Button: ")).expect("Button");
    assert!(button.contains("child!") && button.contains("action!") && button.contains("@checks"));
    assert!(lines.contains(&"formatString(value!)"));
    assert!(lines.contains(&"length(value!, min, max)"));
    assert!(lines.contains(&"@index(offset)"));
}

// SSF-FR-FKWS: the parameter schema is small: a component names its id and
// one of the catalog's types, and nothing else of the catalog is in it.
#[test]
fn the_parameter_schema_is_small() {
    let schema = parameters();
    assert_eq!(schema["required"], json!(["message", "components"]));
    let components = &schema["properties"]["components"];
    assert_eq!(components["minItems"], 1);
    assert_eq!(components["maxItems"], 100);
    assert_eq!(components["items"]["required"], json!(["id", "component"]));
    assert_eq!(
        components["items"]["properties"]["component"]["enum"].as_array().unwrap().len(),
        19
    );
    assert!(components["items"].get("additionalProperties").is_none());
    assert_eq!(schema["properties"]["data_model"]["type"], "object");
    assert!(schema.get("$defs").is_none());
    assert!(schema.to_string().len() < 2_000);
}

// SSF-FR-FCDS / SSF-FR-BWTD / CMS-FR-RWQQ: a call records one open surface and
// one agent comment that carries it, returns `{ shown: true }`, and sets the
// turn-ending flag.
#[test]
fn a_call_records_one_surface_and_one_comment_and_ends_the_turn() {
    let f = Fixture::new();
    let before = f.thread().comments.len();
    let output = f.call(args()).expect("shown");
    assert_eq!(output, ShowSurfaceOutput { shown: true });
    assert_eq!(serde_json::to_value(&output).unwrap(), json!({ "shown": true }));
    assert!(f.asked.load(Ordering::SeqCst));
    let thread = f.thread();
    assert_eq!(thread.comments.len(), before + 1);
    let comment = thread.comments.last().unwrap();
    assert_eq!(comment.body, MESSAGE);
    assert_eq!(comment.author, agent());
    let [Attachment::AgentSurface { surface_id }] = comment.attachments.as_slice() else {
        panic!("one agent surface attachment: {:?}", comment.attachments);
    };
    let stored = crate::agent_surfaces::store::get_impl(&f.root(), surface_id).expect("record");
    assert_eq!(stored.state, SurfaceState::Open);
    assert_eq!(stored.discussion_id, f.thread_id);
    assert_eq!(serde_json::to_value(&stored.components).unwrap(), panel());
    assert_eq!(f.records_left(), 1);
}

// SSF-FR-CVBB: an absent or a null data model is `{}`.
#[test]
fn an_absent_data_model_is_an_empty_object() {
    let f = Fixture::new();
    let display = json!([{"id":"root","component":"Text","text":"Hello"}]);
    f.call(json!({ "message": MESSAGE, "components": display })).expect("shown");
    f.call(json!({ "message": MESSAGE, "components": display, "data_model": null }))
        .expect("shown");
    for comment in f.thread().comments.iter().skip(1) {
        let Some(Attachment::AgentSurface { surface_id }) = comment.attachments.first() else {
            panic!("a surface");
        };
        let stored = crate::agent_surfaces::store::get_impl(&f.root(), surface_id).unwrap();
        assert_eq!(stored.data_model, json!({}));
    }
}

// SSF-FR-EFSU: a blank message and a message with no spoken part are
// retryable refusals, and nothing is recorded.
#[test]
fn a_message_must_have_text_and_a_spoken_part() {
    let f = Fixture::new();
    let before = f.thread().comments.len();
    for (message, expected) in [
        ("  ", BLANK_MESSAGE),
        ("Pick one.", NO_SPOKEN_PART),
        ("Pick one. <spoken> </spoken>", NO_SPOKEN_PART),
    ] {
        let refused = f.call(json!({ "message": message, "components": panel() })).unwrap_err();
        assert_eq!(refused.to_string(), expected, "{message:?}");
        assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
        assert!(refused.retryable());
    }
    assert_eq!(f.thread().comments.len(), before);
    assert_eq!(f.records_left(), 0);
    assert!(!f.asked.load(Ordering::SeqCst));
}

// SSF-FR-CVBB: an invalid surface is a retryable refusal that names the rule,
// the component, and the property, and nothing is recorded.
#[test]
fn an_invalid_surface_names_where_it_is() {
    let f = Fixture::new();
    let mut components = panel();
    components[1]["component"] = json!("Modal");
    let refused = f.call(json!({ "message": MESSAGE, "components": components })).unwrap_err();
    assert_eq!(refused.kind(), ToolErrorKind::InvalidArgs);
    assert!(refused.retryable());
    let text = refused.to_string();
    assert!(
        text.starts_with(
            "The panel is not valid: unknown_component at component `q`, property `component`."
        ),
        "{text}"
    );
    assert!(text.ends_with("Fix it and call again."));
    let refused =
        f.call(json!({ "message": MESSAGE, "components": panel(), "data_model": [] })).unwrap_err();
    assert!(matches!(
        &refused,
        ToolRefusal::InvalidSurface { rule: "data_model_not_object", component_id: None, .. }
    ));
    assert!(!refused.to_string().contains("at component"));
    // A long property name the model sent is cut.
    let mut long = panel();
    long[1]["x".repeat(300)] = json!(1);
    let text = f.call(json!({ "message": MESSAGE, "components": long })).unwrap_err().to_string();
    assert!(!text.contains(&"x".repeat(101)));
    assert_eq!(f.records_left(), 0);
    assert!(!f.asked.load(Ordering::SeqCst));
}

// SSF-FR-NHDH: a resolved conversation is not retryable, and nothing is
// recorded.
#[test]
fn a_resolved_conversation_is_refused() {
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
    let refused = f.call(args()).unwrap_err();
    assert_eq!(refused.to_string(), CONVERSATION_RESOLVED);
    assert_eq!(refused.kind(), ToolErrorKind::PermissionDenied);
    assert!(!refused.retryable());
    assert_eq!(f.records_left(), 0);
}

// SSF-FR-WJHF: a conversation that can not be found is the could-not-record
// refusal, and no record remains.
#[test]
fn a_missing_conversation_is_not_recorded() {
    let f = Fixture::new();
    let tool = f.tool_in("no-such-thread");
    let refused = block_on(tool.call(serde_json::from_value(args()).unwrap())).unwrap_err();
    assert_eq!(refused.to_string(), NOT_RECORDED);
    assert_eq!(refused.kind(), ToolErrorKind::Other);
    assert!(refused.retryable());
    assert_eq!(refused.reason(), "surface_not_recorded");
    assert_eq!(f.records_left(), 0);
}

// SSF-FR-DNZK: with no project open the call is the shared refusal.
#[test]
fn no_open_project_is_the_shared_refusal() {
    let app = closed_project();
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    let tool = ShowSurfaceTool::new(
        app.handle().clone(),
        crate::agent_conversations::OwnedRoots { worktree: root.clone(), store: root },
        ConversationOrigin::stub_draft("t", "d", false),
        agent(),
        Arc::new(AtomicBool::new(false)),
    );
    let refused = block_on(tool.call(serde_json::from_value(args()).unwrap())).unwrap_err();
    assert!(matches!(refused, ToolRefusal::NoProjectOpen));
}

// SSF-FR-BVSL / SSF-FR-JPNA: a success and a refusal each log under `ai`, with
// the counts and the rule, and no component text or message.
#[test]
fn the_log_carries_counts_and_rules_only() {
    static BUFFER: LogBuffer = LogBuffer::new();
    let f = Fixture::new();
    block_on(f.tool().with_buffer(&BUFFER).call(serde_json::from_value(args()).unwrap()))
        .expect("shown");
    let mut bad = panel();
    bad[1]["component"] = json!("Video");
    block_on(
        f.tool().with_buffer(&BUFFER).call(
            serde_json::from_value(json!({ "message": MESSAGE, "components": bad })).unwrap(),
        ),
    )
    .unwrap_err();
    let all = records(&BUFFER);
    let info = all.iter().find(|r| r.level == LogLevel::Info).expect("a success");
    assert!(info.domains.contains(&Domain::Ai) && info.domains.contains(&Domain::Backend));
    assert_eq!(info.fields["components"], 5);
    assert_eq!(info.fields["discussionId"], f.thread_id.as_str());
    let warn = all.iter().find(|r| r.level == LogLevel::Warn).expect("a refusal");
    assert_eq!(warn.fields["refusal"], "invalid_surface");
    assert_eq!(warn.fields["rule"], "unknown_component");
    let text = serde_json::to_string(&all).unwrap();
    for sample in ["Which storage", "SQLite", "Pick a storage", "choose"] {
        assert!(!text.contains(sample), "the log carries `{sample}`: {text}");
    }
}

/// Every file below `dir`, relative to it.
fn files_of(dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    fn walk(
        base: &std::path::Path,
        dir: &std::path::Path,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.insert(path.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(dir, dir, &mut out);
    out
}

// SSF-FR-PTFV: a call writes the surface record and the comment and no other
// file.
#[test]
fn a_call_writes_only_the_record_and_the_comment() {
    let f = Fixture::new();
    let before = files_of(&f.path);
    f.call(args()).expect("shown");
    let after = files_of(&f.path);
    let added: Vec<&String> = after.difference(&before).collect();
    assert_eq!(added.len(), 1, "one new file: {added:?}");
    assert!(added[0].starts_with(SURFACES_DIR));
}
