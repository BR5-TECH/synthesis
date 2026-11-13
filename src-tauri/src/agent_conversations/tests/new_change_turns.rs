//! A turn that proposes one new change of a split
//! (`PNC-propose-new-change-tool.md`, `CVL-conversation-loop.md` CVL-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::*;

const STREAM: &str = "nct-stream";
const CHANGE: &str = "too-big";

fn proposes_new_change(name: &str, position: i64) -> serde_json::Value {
    serde_json::json!({
        "rationale": said("This part ships alone."),
        "name": name,
        "files": [{ "path": "proposal.md", "text": "## Why\n\nA part.\n" }],
        "position": position,
        "total": 2,
    })
}

// PNC-FR-RLNW / CVL-FR-15 / CVL-FR-PEFM: a recorded new change ends the turn
// on the call, and a second call in the same reply is not carried out.
#[test]
fn a_recorded_new_change_ends_the_turn_and_a_second_is_not_carried_out() {
    let reply = ScriptedReply::calls(
        crate::tools::propose_new_change::NAME,
        proposes_new_change("part-one", 1),
    )
    .and_calls(crate::tools::propose_new_change::NAME, proposes_new_change("part-two", 2))
    .with_text("I would also split the rest");
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    h.app.state::<crate::project::ProjectState>().set_root(h.root.path().to_path_buf());
    let wc = tempfile::TempDir::new().expect("tempdir");
    let base = wc.path().join(format!("openspec/changes/{CHANGE}"));
    std::fs::create_dir_all(&base).expect("dirs");
    std::fs::write(base.join("proposal.md"), "## Why\n\nToo big.\n").expect("proposal");
    crate::change_proposals::set_test_stream(
        STREAM,
        "Split stream",
        crate::fs::RootFs::for_root(wc.path()),
    );
    let thread = crate::comments::open_discussion_unchecked(
        &h.root(),
        &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
        None,
        "@arch split it".into(),
        Vec::new(),
        &Participant::Human { login: "raver119".into(), display_name: None, email: None },
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    h.mount();
    h.create_agent("arch", "");
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let turn = wait_for_terminal(&h, &turn.id);

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 1, "the turn ended on its first reply");
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    let thread =
        crate::comments::locate_discussion_of(&h.root(), &target, &thread.id).expect("thread");
    assert_eq!(thread.comments.len(), 2, "one contribution");
    assert!(matches!(
        thread.comments[1].attachments[0],
        crate::comments::Attachment::NewChangeProposal { .. }
    ));
    assert!(!thread.comments.iter().any(|c| c.body.contains("I would also split")));
    let all = crate::new_change_proposals::list_proposals_impl(&h.root(), STREAM, CHANGE);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "part-one");
    assert!(!wc.path().join("openspec/changes/part-one").exists(), "nothing is created");
}

// CVL-FR-15 / CVL-FR-PEFM / AGC-FR-WPAP: `propose_new_change` ends the turn,
// an advisor does not carry it, and a call of it counts as the proposal of a
// group turn.
#[test]
fn the_new_change_tool_ends_the_turn_and_advisors_lack_it() {
    assert!(crate::agent_conversations::turn_body::ends_turn(
        crate::tools::propose_new_change::NAME
    ));
    assert!(crate::agent_conversations::groups::ADVISOR_WITHHELD_TOOLS
        .contains(&crate::tools::propose_new_change::NAME));
    assert!(
        include_str!("../groups.rs").contains("|| name == crate::tools::propose_new_change::NAME")
    );
}
