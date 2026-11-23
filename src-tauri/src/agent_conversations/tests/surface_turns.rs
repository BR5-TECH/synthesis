//! A turn that shows the author a surface (`SSF-show-surface-tool.md`,
//! `CVL-conversation-loop.md` CVL-FR-15, `AGC-agent-conversations.md`
//! AGC-FR-JYBM, AGC-FR-TJDG).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::asking_author::{folded_discussion, run_draft_turn};
use super::*;
use crate::agent_surfaces::store::{self as surfaces, SurfaceInput};
use crate::agent_surfaces::{SurfaceAction, SURFACES_DIR};

fn panel() -> serde_json::Value {
    serde_json::json!([
        {"id":"root","component":"Column","children":["q","pick","send"]},
        {"id":"q","component":"Text","text":"Which storage?"},
        {"id":"pick","component":"ChoicePicker","options":[{"label":"SQLite","value":"sqlite"}],"value":{"@path":"/choice"}},
        {"id":"send","component":"Button","child":"l","action":{"event":{"name":"choose","context":{"choice":{"@path":"/choice"}}}}},
        {"id":"l","component":"Text","text":"Send"}
    ])
}

fn shows(components: serde_json::Value) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::show_surface::NAME,
        serde_json::json!({
            "message": said("Pick a storage."),
            "components": components,
            "data_model": {"choice": []},
        }),
    )
}

fn recorded_surfaces(h: &Harness) -> usize {
    std::fs::read_dir(h.root.path().join(SURFACES_DIR)).map(|d| d.count()).unwrap_or(0)
}

// CVL-FR-15 / SSF-FR-BWTD / SSF-FR-YIXE: a shown surface ends the turn on the
// call, the prose of the reply is not appended, and a second call of the tool
// in the same reply records nothing.
#[test]
fn a_shown_surface_ends_the_turn_and_a_second_is_not_carried_out() {
    let reply = shows(panel())
        .and_calls(
            crate::tools::show_surface::NAME,
            serde_json::json!({"message": said("And another."), "components": panel()}),
        )
        .with_text("I would also ask about the cache");
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 1, "the turn ended on its first reply");
    let thread = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(thread.comments.len(), 2, "one contribution");
    assert!(matches!(
        thread.comments[1].attachments.as_slice(),
        [crate::comments::Attachment::AgentSurface { .. }]
    ));
    assert!(!thread.comments.iter().any(|c| c.body.contains("also ask")));
    assert_eq!(recorded_surfaces(&h), 1);
}

// SSF-FR-YIXE / CVL-FR-15: where the first call refused, the second call in
// the reply gets the one at a time refusal, and the loop goes on.
#[test]
fn a_second_call_after_a_refused_one_gets_the_one_at_a_time_refusal() {
    let mut invalid = panel();
    invalid[1]["component"] = serde_json::json!("Modal");
    let reply = shows(invalid).and_calls(
        crate::tools::show_surface::NAME,
        serde_json::json!({"message": said("Pick a storage."), "components": panel()}),
    );
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("I could not."))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, _draft_id, _thread) = run_draft_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.requests().len(), 2, "the loop went on after the refusal");
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(results.iter().any(|r| r.contains("The panel is not valid")), "{results:?}");
    assert!(
        results.iter().any(|r| r.contains(crate::tools::show_surface::ONE_AT_A_TIME)),
        "{results:?}"
    );
    assert_eq!(recorded_surfaces(&h), 0, "nothing was recorded");
}

// CVL-FR-15 / CVL-FR-HNGB / SSF-FR-DNZK: the tool ends the turn, and an
// advisor turn does not carry it.
#[test]
fn the_surface_tool_ends_the_turn_and_advisors_lack_it() {
    assert!(crate::agent_conversations::turn_body::ends_turn(crate::tools::show_surface::NAME));
    assert!(crate::agent_conversations::groups::ADVISOR_WITHHELD_TOOLS
        .contains(&crate::tools::show_surface::NAME));
}

/// A draft discussion whose agent comment carries a recorded surface, and a
/// later comment of the author. Returns the thread, the surface, and the
/// author comment.
fn seeded_surface(h: &Harness) -> (Discussion, crate::agent_surfaces::AgentSurface, String) {
    let (_draft_id, thread) = seed_discussion(h, &["@arch which storage?"]);
    let components: Vec<serde_json::Value> = serde_json::from_value(panel()).unwrap();
    let agent = crate::comments::Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: None,
        title: None,
    };
    let surface = surfaces::record_with(
        h.app.handle(),
        &crate::logging::BUFFER,
        &h.root(),
        SurfaceInput {
            components: &components,
            data_model: &serde_json::json!({"choice": []}),
            message: "Pick a storage. <spoken>Pick a storage.</spoken>",
            agent: &agent,
            origin: &ConversationOrigin::of(&thread),
        },
    )
    .expect("recorded");
    crate::comments::add_comment_with_id(
        &h.root(),
        thread.log_ref(),
        &thread.id,
        "c-after".into(),
        "@arch see above".into(),
        &human("ada"),
        // After the agent comment, which carries the time it was recorded.
        "2999-01-01T00:00:00Z",
    )
    .expect("comment");
    (refolded(h, &surface), surface, "c-after".into())
}

/// The thread of `surface` as it now stands on disk.
fn refolded(h: &Harness, surface: &crate::agent_surfaces::AgentSurface) -> Discussion {
    crate::comments::fold_discussion(&h.root(), &surface_draft(surface))
        .into_iter()
        .find(|t| t.id == surface.discussion_id)
        .expect("thread")
}

// AGC-FR-JYBM / AGC-FR-09: an open surface reads in the history with its id,
// its state, its components with their texts, and its data model.
#[test]
fn an_open_surface_reads_in_the_history() {
    let h = Harness::new(vec![]);
    h.mount();
    let (thread, surface, trigger) = seeded_surface(&h);
    let sections = build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &trigger);
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    let text = &history.body;
    assert!(text.contains(&format!("[agent surface {}: open]", surface.id)), "{text}");
    assert!(text.contains("- q Text: \"Which storage?\""), "{text}");
    assert!(text.contains("- pick ChoicePicker: \"SQLite\""), "{text}");
    assert!(text.contains("- send Button"), "{text}");
    assert!(text.contains("data model: {\"choice\":[]}"), "{text}");
    assert!(!text.contains("pressed:"), "{text}");
}

// AGC-FR-TJDG: a submitted surface also names the pressed button, its
// context, and the submitted data model.
#[test]
fn a_submitted_surface_names_what_the_author_sent() {
    let h = Harness::new(vec![]);
    h.mount();
    let (_thread, surface, _) = seeded_surface(&h);
    let result = surfaces::submit_impl(
        h.app.handle(),
        &crate::logging::BUFFER,
        &h.root(),
        &surface.id,
        SurfaceAction {
            name: "choose".into(),
            source_component_id: "send".into(),
            context: serde_json::json!({"choice": ["sqlite"]}),
            data_model: serde_json::json!({"choice": ["sqlite"]}),
        },
        &human("ada"),
    )
    .expect("submitted");
    let thread = refolded(&h, &surface);
    let sections =
        build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &result.comment_id);
    let text = &section_of(&sections, TAG_DISCUSSION_HISTORY).body;
    assert!(text.contains(&format!("[agent surface {}: submitted]", surface.id)), "{text}");
    assert!(text.contains("pressed: choose on send"), "{text}");
    assert!(text.contains("context: {\"choice\":[\"sqlite\"]}"), "{text}");
    assert!(text.contains("submitted data model: {\"choice\":[\"sqlite\"]}"), "{text}");
    let current = &section_of(&sections, TAG_CURRENT_COMMENT).body;
    assert!(current.contains("Sent “Send”."), "{current}");
}

fn surface_draft(surface: &crate::agent_surfaces::AgentSurface) -> String {
    match &surface.target {
        crate::comments::DiscussionTarget::Draft { draft_id } => draft_id.clone(),
        other => panic!("a draft target: {other:?}"),
    }
}

// AGC-FR-TJDG: a surface whose record does not read is one line with its id,
// and the input still builds.
#[test]
fn a_surface_that_does_not_read_is_one_line() {
    let h = Harness::new(vec![]);
    h.mount();
    let (thread, surface, trigger) = seeded_surface(&h);
    std::fs::remove_file(h.root.path().join(SURFACES_DIR).join(format!("{}.json", surface.id)))
        .unwrap();
    let sections = build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &trigger);
    let text = &section_of(&sections, TAG_DISCUSSION_HISTORY).body;
    assert!(text.contains(&format!("[agent surface {}]", surface.id)), "{text}");
    assert!(!text.contains("Which storage?"), "{text}");
}

// AGC-FR-JYBM: a surface on the comment that addressed the agent reads in
// `current_comment`.
#[test]
fn a_surface_on_the_current_comment_reads_there() {
    let h = Harness::new(vec![]);
    h.mount();
    let (thread, surface, _) = seeded_surface(&h);
    let sections =
        build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &surface.comment_id);
    let text = &section_of(&sections, TAG_CURRENT_COMMENT).body;
    assert!(text.contains(&format!("[agent surface {}: open]", surface.id)), "{text}");
}

// SSF-FR-FCDS / CVL-FR-VSSU: the comment of a surface is its message less its
// citation markers.
#[test]
fn the_message_of_a_surface_carries_no_citation_marker() {
    const SPAN: &str = "\u{E200}cite\u{E202}turn0search1\u{E201}";
    let h = Harness::scripted(vec![Ok(ScriptedReply::calls(
        crate::tools::show_surface::NAME,
        serde_json::json!({
            "message": said(format!("Pick a storage {SPAN}.")),
            "components": panel(),
            "data_model": {"choice": []},
        }),
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    let thread = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(thread.comments[1].body, said("Pick a storage."));
}
