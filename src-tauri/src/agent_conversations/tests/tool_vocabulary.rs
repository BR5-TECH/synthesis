//! The words of the tool definitions that a conversation turn is offered
//! (`PVC-product-vocabulary.md` PVC-FR-CUAT).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::plan_chaining::plan_project;
use super::*;
use crate::comments::DiscussionTarget;
use crate::prompts::vocabulary_tests::{definition_texts, internal_words};

const CHANGE: &str = "add-auth";
const STREAM: &str = "tv-stream";

/// A stream working copy that holds one OpenSpec change, registered for the
/// turn builders.
fn working_copy() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let base = dir.path().join(format!("openspec/changes/{CHANGE}"));
    std::fs::create_dir_all(&base).expect("dirs");
    std::fs::write(base.join("proposal.md"), "## Why\n\nUsers need to log in.\n")
        .expect("proposal");
    crate::change_proposals::set_test_stream(
        STREAM,
        "Auth stream",
        crate::fs::RootFs::for_root(dir.path()),
    );
    dir
}

/// A whole-target discussion on `target` that addresses `arch`.
fn discussion_on(h: &Harness, target: DiscussionTarget) -> Discussion {
    crate::comments::open_discussion_unchecked(
        &h.root(),
        &target,
        None,
        "@arch what is missing?".into(),
        Vec::new(),
        &human("ada"),
        "2026-03-01T10:00:00Z",
    )
    .expect("a discussion")
}

/// The tools of the one turn that `trigger` of `origin` starts.
fn tools_of_turn(
    h: &Harness,
    origin: ConversationOrigin,
    trigger: &str,
) -> Vec<rig::completion::ToolDefinition> {
    let before = h.seam.requests().len();
    let turn = h.dispatch("arch", origin, trigger).expect("dispatch");
    wait_for_terminal(h, &turn.id);
    h.seam.requests().into_iter().nth(before).expect("the request of the turn").0.tools
}

// PVC-FR-CUAT: no tool description and no parameter description that a turn on
// a change, a file, a note, or an OpenSpec change is offered calls a change a
// "draft", or an implementation a "graduation". The plan tools are attached,
// so their descriptions are read too.
#[test]
fn no_tool_of_a_conversation_turn_describes_itself_with_an_internal_word() {
    let replies = (0..4).map(|_| Ok(ScriptedReply::answer(said("ok")))).collect();
    let h = plan_project(replies);
    let _working_copy = working_copy();

    let (_, change) = seed_discussion(&h, &["@arch what now?"]);
    std::fs::write(h.root().path().join("spec.md"), "# Spec\n\nPayments.\n").expect("a file");
    let file = discussion_on(&h, DiscussionTarget::Artifact { artifact_id: "spec.md".into() });
    let openspec = discussion_on(
        &h,
        DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
    );
    let (_, note) =
        seed_note_discussion(&h, crate::notes::NoteScope::Project, "a project note", "opening");
    let note_trigger = crate::comments::add_comment_to(
        &h.root(),
        crate::comments::ThreadRef::note_discussion(note.note_id().expect("a note origin")),
        note.discussion_id(),
        "@arch so what should I do?".into(),
        Vec::new(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:04:00Z" },
    )
    .expect("a trigger")
    .comments
    .last()
    .expect("the trigger")
    .id
    .clone();

    let mut offered = Vec::new();
    for thread in [&change, &file, &openspec] {
        offered.extend(tools_of_turn(&h, ConversationOrigin::of(thread), &thread.comments[0].id));
    }
    offered.extend(tools_of_turn(&h, note, &note_trigger));

    let names: Vec<&str> = offered.iter().map(|t| t.name.as_str()).collect();
    for expected in [
        crate::tools::propose_draft_changes::NAME,
        crate::tools::propose_new_draft::NAME,
        crate::tools::decide_draft_changes::NAME,
        crate::tools::propose_prompt_changes::NAME,
        crate::tools::propose_diagram_changes::NAME,
        crate::tools::propose_change_edits::NAME,
        crate::tools::propose_new_change::NAME,
        crate::tools::decide_change_edits::NAME,
        crate::tools::request_change_action::NAME,
        crate::tools::propose_plan_changes::NAME,
        crate::tools::draft_read::NAME,
        crate::tools::measure_document::NAME,
    ] {
        assert!(names.contains(&expected), "{expected} is not among {names:?}");
    }
    for definition in &offered {
        let texts = definition_texts(definition);
        assert!(!texts.is_empty(), "{} has no description", definition.name);
        for text in texts {
            let words = internal_words(&text);
            assert!(words.is_empty(), "{} uses {words:?} in {text:?}", definition.name);
        }
    }
}
