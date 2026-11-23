//! The readers of the message log (AUI-FR-TYBX): an action and a media read
//! use the state that the whole log folds to, not the first message alone.
//! Also the relative context of a Button in a template (AUI-FR-AFKN).

use serde_json::{json, Value};
use tempfile::TempDir;

use crate::agent_conversations::ConversationOrigin;
use crate::agent_surfaces::media::*;
use crate::agent_surfaces::store::*;
use crate::agent_surfaces::*;
use crate::comments::{self, Participant};
use crate::fs;
use crate::logging::LogBuffer;
use crate::tools::tests::mounted;

static BUFFER: LogBuffer = LogBuffer::new();

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent { agent_id: "a".into(), handle: "arch".into(), model: None, title: None }
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
            "@arch show me".into(),
            Vec::new(),
            comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture { open: mounted(dir), path, draft_id: created.draft.id, thread_id: thread.id }
    }

    fn root(&self) -> fs::RootFs {
        fs::RootFs::for_root(&self.path)
    }

    fn record(&self, components: Vec<Value>, data_model: Value) -> AgentSurface {
        let origin =
            ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false);
        record_with(
            &self.open.handle(),
            &BUFFER,
            &self.root(),
            SurfaceInput {
                components: &components,
                data_model: &data_model,
                message: "Look. <spoken>Look.</spoken>",
                agent: &agent(),
                origin: &origin,
            },
        )
        .expect("recorded")
    }

    /// Add one `updateComponents` message to the stored log of `id`.
    fn update_components(&self, id: &str, components: Value) {
        let file = self.path.join(SURFACES_DIR).join(format!("{id}.json"));
        let mut record: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        record["messages"].as_array_mut().expect("a log").push(json!({
            "version": "1.0",
            "updateComponents": { "surfaceId": id, "components": components },
        }));
        std::fs::write(&file, serde_json::to_string(&record).unwrap()).unwrap();
    }

    fn submit(&self, id: &str, action: SurfaceAction) -> Result<SurfaceSubmitResult, String> {
        submit_impl(&self.open.handle(), &BUFFER, &self.root(), id, action, &human())
    }

    fn last_body(&self) -> String {
        comments::fold_discussion(&self.root(), &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .and_then(|t| t.comments.last().map(|c| c.body.clone()))
            .expect("a comment")
    }
}

fn button(id: &str, label: &str, event: &str) -> Vec<Value> {
    vec![
        json!({"id": id, "component": "Button", "child": format!("{id}_label"),
               "action": {"event": {"name": event}}}),
        json!({"id": format!("{id}_label"), "component": "Text", "text": label}),
    ]
}

fn action(source: &str, name: &str, context: Value) -> SurfaceAction {
    SurfaceAction {
        name: name.into(),
        source_component_id: source.into(),
        context,
        data_model: json!({}),
    }
}

// AUI-FR-TYBX / AUI-FR-KBVY / AUI-FR-XMUS: an action is checked against the
// folded state. A Button that a later message adds is a sender, and its label
// in the comment is the label of the latest message. The event that a later
// message replaced is no longer an action of the surface.
#[test]
fn an_action_uses_the_folded_state_of_the_log() {
    let f = Fixture::new();
    let mut first = vec![json!({"id":"root","component":"Column","children":["go"]})];
    first.extend(button("go", "Go", "go"));
    let surface = f.record(first, json!({}));
    let mut later = vec![json!({"id":"root","component":"Column","children":["go","more"]})];
    later.extend(button("go", "Go now", "go_now"));
    later.extend(button("more", "More", "more"));
    f.update_components(&surface.id, Value::Array(later));

    assert_eq!(
        f.submit(&surface.id, action("go", "go", json!({}))).unwrap_err(),
        ERR_UNKNOWN_ACTION
    );
    f.submit(&surface.id, action("more", "more", json!({}))).expect("a sender of a later message");
    assert_eq!(f.last_body(), "Sent “More”.");
}

// AUI-FR-TYBX / AUI-FR-XMUS: a label that a later message changes names the
// Button in the comment.
#[test]
fn a_changed_label_names_the_button() {
    let f = Fixture::new();
    let surface = f.record(button("root", "Go", "go"), json!({}));
    f.update_components(
        &surface.id,
        json!([{"id":"root_label","component":"Text","text":"Start"}]),
    );
    f.submit(&surface.id, action("root", "go", json!({}))).expect("accepted");
    assert_eq!(f.last_body(), "Sent “Start”.");
}

// AUI-FR-TYBX / AUI-FR-YXMD: a media read accepts a source that a later
// message names, and refuses the source of the replaced component.
#[test]
fn a_media_read_uses_the_folded_state_of_the_log() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.path.join("docs")).unwrap();
    std::fs::write(f.path.join("docs/old.png"), [1u8]).unwrap();
    std::fs::write(f.path.join("docs/new.png"), [2u8]).unwrap();
    let surface = f.record(
        vec![
            json!({"id":"root","component":"Column","children":["m"]}),
            json!({"id":"m","component":"Image","url":"project:docs/old.png"}),
        ],
        json!({}),
    );
    f.update_components(
        &surface.id,
        json!([{"id":"m","component":"Image","url":"project:docs/new.png"}]),
    );
    let read = |source: &str| read_media_impl(&f.root(), &f.root(), &surface.id, source);
    assert_eq!(read("project:docs/new.png").expect("named by the later message").data, "Ag==");
    assert_eq!(read("project:docs/old.png").unwrap_err(), ERR_MEDIA_NOT_NAMED);
}

/// A List of rows, each with a Button whose context reads the row item.
fn template() -> Vec<Value> {
    vec![
        json!({"id":"root","component":"List","children":{"componentId":"row","path":"/items"}}),
        json!({"id":"row","component":"Row","children":["name","pick"]}),
        json!({"id":"name","component":"Text","text":{"@path":"name"}}),
        json!({"id":"pick","component":"Button","child":"pick_label",
               "action":{"event":{"name":"pick","context":{"item":{"@path":"name"},"kind":"row"}}}}),
        json!({"id":"pick_label","component":"Text","text":"Pick"}),
    ]
}

// AUI-FR-AFKN / AUI-FR-XMUS: a Button in a template may bind its context to
// the row item with a relative pointer. The surface is valid, the action takes
// the value that the author sends for that key, and the comment carries it.
// The literal and the key set are still checked.
#[test]
fn a_relative_context_in_a_template_takes_the_sent_value() {
    let model = json!({"items": [{"name": "SQLite"}, {"name": "Files"}]});
    validate_surface(&template(), &model).expect("a relative context in a template is valid");
    let f = Fixture::new();
    let surface = f.record(template(), model);
    assert_eq!(
        f.submit(&surface.id, action("pick", "pick", json!({"item": "Files", "kind": "other"})))
            .unwrap_err(),
        ERR_INVALID_CONTEXT
    );
    assert_eq!(
        f.submit(&surface.id, action("pick", "pick", json!({"item": "Files"}))).unwrap_err(),
        ERR_INVALID_CONTEXT
    );
    f.submit(&surface.id, action("pick", "pick", json!({"item": "Files", "kind": "row"})))
        .expect("accepted");
    assert_eq!(f.last_body(), "Sent “Pick”.\n- item: Files\n- kind: row");
}
