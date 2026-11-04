//! A turn that proposes one new draft of a decomposition
//! (`PND-propose-new-draft-tool.md`, `CVL-conversation-loop.md` CVL-FR-15).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::asking_author::{folded_discussion, run_draft_turn};
use super::*;

fn proposes_new_draft(name: &str, position: i64) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::propose_new_draft::NAME,
        serde_json::json!({
            "rationale": said("This part ships alone."),
            "name": name,
            "body": "# Part\n\nThe part.\n",
            "position": position,
            "total": 3,
        }),
    )
}

// PND-FR-KGRN / PND-FR-YPCH / CVL-FR-15: a recorded new draft ends the turn on
// the call, and a second call in the same reply is not carried out.
#[test]
fn a_recorded_new_draft_ends_the_turn_and_a_second_is_not_carried_out() {
    let reply = proposes_new_draft("relay-auth", 1)
        .and_calls(
            crate::tools::propose_new_draft::NAME,
            serde_json::json!({
                "rationale": said("And the next."),
                "name": "relay-sync",
                "body": "x",
                "position": 2,
                "total": 3,
            }),
        )
        .with_text("I would also split the sync part");
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 1, "the turn ended on its first reply");
    let thread = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(thread.comments.len(), 2, "one contribution");
    assert!(matches!(
        thread.comments[1].attachments[0],
        crate::comments::Attachment::NewDraftProposal { .. }
    ));
    assert!(!thread.comments.iter().any(|c| c.body.contains("I would also split")));
    let all = crate::new_draft_proposals::list_impl(&h.root(), &draft_id).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "relay-auth");
}

/// A draft whose discussion opens with `opening`, holding one pending
/// new-draft proposal of the agent `arch`. Answers the draft, the discussion,
/// and the proposal id.
fn draft_with_pending_new_draft(h: &Harness, opening: &str) -> (String, Discussion, String) {
    let (draft_id, thread) = super::support::seed_discussion(h, &[opening]);
    let agent = crate::comments::Participant::Agent {
        agent_id: "a".into(),
        handle: "arch".into(),
        model: None,
        title: None,
    };
    let recorded = crate::new_draft_proposals::record_new_draft_proposal(
        &h.app.handle().clone(),
        &h.root(),
        &h.root(),
        crate::new_draft_proposals::NewDraftInput {
            draft_id: &draft_id,
            name: "relay-auth",
            body: "body",
            position: 1,
            total: 2,
            rationale: "<spoken>x</spoken>",
            agent: &agent,
            target: crate::comments::ThreadRef::discussion(&draft_id),
            thread_id: &thread.id,
        },
    )
    .expect("recorded");
    (draft_id, thread, recorded.id)
}

/// The replies of the turn, set once the proposal id is known.
fn script(h: &Harness, replies: Vec<Result<ScriptedReply, &'static str>>) {
    *h.seam.replies.lock().unwrap() = replies.into_iter().collect();
}

fn decides(proposal_id: &str, instruction: &str) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::decide_draft_changes::NAME,
        serde_json::json!({
            "instruction": instruction,
            "decisions": [{ "id": proposal_id, "action": "accept" }],
        }),
    )
}

// DDC-FR-ELAE / AGC-FR-DTCL / AGC-FR-TFEN: a dispatched turn whose trigger is
// an author comment decides through the tool. The tool is bound to that turn.
#[test]
fn a_dispatched_turn_on_an_author_comment_decides() {
    let h = Harness::scripted(Vec::new());
    h.mount();
    h.create_agent("arch", "");
    let (draft_id, thread, proposal) = draft_with_pending_new_draft(&h, "@arch accept that draft");
    script(
        &h,
        vec![Ok(decides(&proposal, "accept that draft")), Ok(ScriptedReply::answer("Accepted."))],
    );
    crate::agent_conversations::note_author_comment(
        &h.app.handle().clone(),
        &thread.comments[0].id,
    );
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    let all = crate::new_draft_proposals::list_impl(&h.root(), &draft_id).unwrap();
    assert_eq!(all[0].state, crate::new_draft_proposals::NewDraftState::Accepted);
}

// DDC-FR-ELAE: a dispatched turn whose trigger the app did not note as an
// author comment, as a decision comment, decides nothing.
#[test]
fn a_dispatched_turn_on_a_comment_that_is_not_the_author_s_decides_nothing() {
    let h = Harness::scripted(Vec::new());
    h.mount();
    h.create_agent("arch", "");
    let (draft_id, thread, proposal) = draft_with_pending_new_draft(&h, "@arch accept that draft");
    script(
        &h,
        vec![Ok(decides(&proposal, "accept that draft")), Ok(ScriptedReply::answer("I can not."))],
    );
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    let all = crate::new_draft_proposals::list_impl(&h.root(), &draft_id).unwrap();
    assert_eq!(all[0].state, crate::new_draft_proposals::NewDraftState::Pending);
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(results.iter().any(|r| r.contains("not one the author wrote or spoke")), "{results:?}");
}
