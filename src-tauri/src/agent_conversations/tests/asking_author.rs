//! A turn that stops to ask the author something, and the answer that
//! starts a fresh one.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// Asking the author (CVL-FR-15 … AGC-FR-29)
// ---------------------------------------------------------------------------



// ---------------------------------------------------------------------------
// AGC-FR-01, AGC-FR-17, AGC-FR-21 … AGC-FR-13, AGC-FR-07 — proposing a change to a draft file (CVL-FR-15, AGC-FR-29)
// ---------------------------------------------------------------------------

/// A reply that proposes one change to the draft's prompt.
///
/// The change names the text it replaces, which is what the tool now takes
/// (PDC-FR-04); the seeded prompt holds `Loose notes.`.
fn proposes(path: &str, content: &str, rationale: &str) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::propose_draft_changes::NAME,
        serde_json::json!({
            "path": path,
            "rationale": rationale,
            "hunks": [{
                "kind": "replace",
                "before": "Loose notes.",
                "after": content,
            }],
        }),
    )
}

/// A draft with a discussion on it, and a turn dispatched into that discussion.
pub(super) fn run_draft_turn(h: &Harness, nickname: &str) -> (AgentTurn, String, Discussion) {
    let (draft_id, thread) = seed_discussion(h, &[&format!("@{nickname} what would you change?")]);
    let turn = h
        .dispatch(
            nickname,
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let terminal = wait_for_terminal(h, &turn.id);
    (terminal, draft_id, thread)
}

/// The draft's discussion as it now stands on disk.
pub(super) fn folded_discussion(h: &Harness, draft_id: &str, thread_id: &str) -> Discussion {
    crate::comments::fold_discussion(&h.root(), draft_id)
        .into_iter()
        .find(|t| t.id == thread_id)
        .expect("thread")
}

#[test]
fn a_turn_that_proposes_a_change_ends_on_that_call() {
    // CVL-FR-15, AGC-FR-01, AGC-FR-17, AGC-FR-21.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "spec.md" }),
        )),
        Ok(proposes("artifact-window.md", "A better UI half.", "The half buries its point.")),
        // Never reached: the turn ends on the proposal.
        Ok(ScriptedReply::answer("unreachable")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(
        turn.state,
        AgentTurnState::AwaitingReply,
        "CVL-FR-15: the turn ends awaiting the author's decision",
    );
    assert_eq!(turn.failure, None, "proposing is not a failure");
    assert_eq!(
        h.seam.requests().len(),
        2,
        "CVL-FR-15: no further model call once the proposal was recorded",
    );

    // AGC-FR-01: exactly one contribution, and it is the proposal.
    let thread = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(thread.comments.len(), 2);
    let posted = &thread.comments[1];
    assert_eq!(posted.body, "The half buries its point.");
    assert!(matches!(posted.author, Participant::Agent { .. }));
    // AGC-FR-17: the one comment an agent carries an attachment on, and it is a
    // proposal reference rather than a file.
    assert_eq!(posted.attachments.len(), 1);
    assert!(matches!(
        posted.attachments[0],
        crate::comments::Attachment::Proposal { .. }
    ));

    // The candidate is beside the draft and the draft's own file is untouched.
    let proposals =
        crate::draft_proposals::list_proposals_impl(&h.root(), &draft_id).expect("list");
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].state, crate::draft_proposals::ProposalState::Pending);
    assert_eq!(
        crate::drafts::load_draft_file_impl(&h.root(), &draft_id, "artifact-window.md")
            .expect("file")
            .body,
        "Loose notes.",
        "CVL-FR-09: a turn writes no file of the draft",
    );
}

// ---------------------------------------------------------------------------
// CVL-FR-15, AGC-FR-01, AGC-FR-17, AGC-FR-21 … CVL-FR-15 — proposing a change to a prompt artifact
// (CVL-FR-08, CVL-FR-15, AGC-FR-01, AGC-FR-17)
// ---------------------------------------------------------------------------

/// A reply that proposes a new version of a prompt the project holds.
fn proposes_prompt(path: &str, content: &str, rationale: &str) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::propose_prompt_changes::NAME,
        serde_json::json!({
            "path": path,
            "content": content,
            "rationale": rationale,
        }),
    )
}

const PROMPT_PATH: &str = "prompts/review.md";
const PROMPT_BODY: &str = "# Review\n\nRead the file, summarise it and file a note.\n";

/// A prompt artifact with a discussion on it, typed by a file-scope assignment
/// so its resolved type is exactly `prompt` (ASC-FR-06 level 1).
fn seed_prompt_discussion(h: &Harness, nickname: &str) -> Discussion {
    let root = h.root();
    std::fs::create_dir_all(root.path().join("prompts")).expect("dirs");
    std::fs::write(root.path().join(PROMPT_PATH), PROMPT_BODY).expect("prompt");
    std::fs::create_dir_all(root.path().join(".synthesis")).expect("dirs");
    std::fs::write(
        root.path().join(".synthesis/library.toml"),
        format!(
            "[assignments.\"{PROMPT_PATH}\"]\ntype = \"prompt\"\nscope = \"file\"\n"
        ),
    )
    .expect("assignment");
    crate::comments::open_discussion_in(
        &root,
        &root,
        &crate::comments::DiscussionTarget::Artifact {
            artifact_id: PROMPT_PATH.into(),
        },
        None,
        format!("@{nickname} what would you change?"),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion")
}

fn run_prompt_turn(h: &Harness, nickname: &str) -> (AgentTurn, Discussion) {
    let thread = seed_prompt_discussion(h, nickname);
    let turn = h
        .dispatch(
            nickname,
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    (wait_for_terminal(h, &turn.id), thread)
}

fn folded_prompt_discussion(h: &Harness, thread_id: &str) -> Discussion {
    crate::comments::fold_artifact_discussion(&h.root(), PROMPT_PATH)
        .into_iter()
        .find(|t| t.id == thread_id)
        .expect("thread")
}

#[test]
fn a_turn_that_proposes_a_prompt_change_ends_on_that_call() {
    // CVL-FR-15, AGC-FR-01, AGC-FR-17, AGC-FR-21.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": PROMPT_PATH }),
        )),
        Ok(proposes_prompt(
            PROMPT_PATH,
            "# Review\n\nRead the file.\n\nSummarise it, then file a note.\n",
            "The instructions bury the important step.",
        )),
        // Never reached: the turn ends on the proposal.
        Ok(ScriptedReply::answer("unreachable")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, thread) = run_prompt_turn(&h, "arch");
    assert_eq!(
        turn.state,
        AgentTurnState::AwaitingReply,
        "CVL-FR-15: the turn ends awaiting the author's decision",
    );
    assert_eq!(turn.failure, None, "proposing is not a failure");
    assert_eq!(
        h.seam.requests().len(),
        2,
        "CVL-FR-15: no further model call once the proposal was recorded",
    );

    // AGC-FR-01: exactly one contribution, and it is the proposal.
    let thread = folded_prompt_discussion(&h, &thread.id);
    assert_eq!(thread.comments.len(), 2);
    let posted = &thread.comments[1];
    assert_eq!(posted.body, "The instructions bury the important step.");
    assert!(matches!(posted.author, Participant::Agent { .. }));
    // AGC-FR-17: one reference attachment and nothing beside it, and it is the
    // `prompt_proposal` kind rather than the draft's.
    assert_eq!(posted.attachments.len(), 1);
    assert!(matches!(
        posted.attachments[0],
        crate::comments::Attachment::PromptProposal { .. }
    ));

    // CVL-FR-09: the candidate is beside the project and the artifact is
    // untouched.
    let proposals =
        crate::prompt_proposals::list_proposals_impl(&h.root(), PROMPT_PATH);
    assert_eq!(proposals.len(), 1);
    assert_eq!(
        proposals[0].state,
        crate::prompt_proposals::PromptProposalState::Pending,
    );
    assert_eq!(
        std::fs::read_to_string(h.root().path().join(PROMPT_PATH)).expect("prompt"),
        PROMPT_BODY,
        "a turn writes no file of the project",
    );
}

#[test]
fn a_refused_prompt_proposal_ends_nothing_and_the_turn_answers_with_what_it_has() {
    // CVL-FR-15, CVL-FR-14, PPC-FR-06, PPC-FR-09, PPC-FR-16.
    // A file whose resolved type is not `prompt` is refused, the loop keeps
    // running, and the turn delivers the prose it answers with instead.
    let h = Harness::scripted(vec![
        Ok(proposes_prompt(
            "spec.md",
            "# Spec\n\nA rewrite.\n",
            "It reads better this way.",
        )),
        Ok(ScriptedReply::answer("I would split the Steps section.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    std::fs::write(h.root().path().join("spec.md"), "# Spec\n").expect("spec");
    let (turn, thread) = run_prompt_turn(&h, "arch");

    assert_eq!(
        turn.state,
        AgentTurnState::Delivered,
        "CVL-FR-14: a tool refusal is not a turn's failure and ends nothing",
    );
    assert_eq!(h.seam.requests().len(), 2, "the loop made a further call");
    assert!(
        crate::prompt_proposals::list_proposals_impl(&h.root(), "spec.md").is_empty(),
        "and the project gained no candidate",
    );
    let thread = folded_prompt_discussion(&h, &thread.id);
    assert_eq!(thread.comments.len(), 2);
    assert_eq!(thread.comments[1].body, "I would split the Steps section.");
}

#[test]
fn a_reply_asking_for_both_ending_tools_on_an_artifact_makes_one_contribution() {
    // CVL-FR-15, AGC-FR-01.
    let reply = proposes_prompt(
        PROMPT_PATH,
        "# Review\n\nRead the file.\n",
        "Leads with the point.",
    )
    .and_calls(
        crate::tools::ask_user_comment::NAME,
        serde_json::json!({ "question": "Or shall I?" }),
    )
    .and_calls("read_file", serde_json::json!({ "path": PROMPT_PATH }))
    .with_text("I would also split the Steps");

    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, thread) = run_prompt_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 1, "the turn ended on its first reply");

    let thread = folded_prompt_discussion(&h, &thread.id);
    assert_eq!(
        thread.comments.len(),
        2,
        "one contribution and not two (AGC-FR-01)",
    );
    assert_eq!(thread.comments[1].body, "Leads with the point.");
    assert!(
        !thread
            .comments
            .iter()
            .any(|c| c.body.contains("I would also split the Steps")),
        "CVL-FR-15: prose alongside an ending call is appended nowhere",
    );
}

#[test]
fn a_reply_asking_for_both_ending_tools_makes_one_contribution() {
    // CVL-FR-15, AGC-FR-01. The first of the two is dispatched and the
    // other is not, whichever order the reply put them in.
    let reply = proposes("artifact-window.md", "A better UI half.", "Leads with the point.")
        .and_calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": "Or shall I?" }),
        )
        .and_calls("search_specifications", serde_json::json!({ "query": "drafts" }))
        .with_text("I would also split the Intent");

    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 1, "the turn ended on its first reply");

    let thread = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(
        thread.comments.len(),
        2,
        "one contribution and not two (AGC-FR-01)",
    );
    assert_eq!(
        thread.comments[1].body, "Leads with the point.",
        "the proposal, not the question",
    );
    assert!(
        !thread
            .comments
            .iter()
            .any(|c| c.body.contains("I would also split the Intent")),
        "CVL-FR-15: prose alongside an ending call is appended nowhere",
    );
}

#[test]
fn a_refused_proposal_ends_nothing_and_the_turn_answers_with_what_it_has() {
    // PDC-FR-08, PDC-FR-15 / CVL-FR-15, CVL-FR-14. A proposal refused because one already
    // stands leaves the loop running, exactly as any other tool refusal does.
    let h = Harness::scripted(vec![
        Ok(proposes("artifact-window.md", "The first rewrite.", "One.")),
        Ok(proposes("artifact-window.md", "The second rewrite.", "Two.")),
        Ok(ScriptedReply::answer("I will leave the second for later.")),
    ]);
    h.mount();
    h.create_agent("arch", "");

    // The first turn proposes and ends awaiting a decision.
    let (first, draft_id, thread) = run_draft_turn(&h, "arch");
    assert_eq!(first.state, AgentTurnState::AwaitingReply);

    // A second turn in the same conversation finds the slot taken.
    let second = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let second = wait_for_terminal(&h, &second.id);

    assert_eq!(
        second.state,
        AgentTurnState::Delivered,
        "CVL-FR-14: a tool refusal is not a turn's failure and ends nothing",
    );
    assert_eq!(
        crate::draft_proposals::list_proposals_impl(&h.root(), &draft_id)
            .expect("list")
            .len(),
        1,
        "and no second candidate was recorded",
    );
    let thread = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(
        thread.comments.last().expect("last").body,
        "I will leave the second for later.",
        "the turn answered with what it had",
    );
}

#[test]
fn a_decision_dispatches_a_fresh_turn_reading_the_changed_file() {
    // AGC-FR-21 / AGC-FR-29, AGC-FR-13, AGC-FR-07. The awaiting turn retires, and the
    // new turn's input is assembled from the conversation and the draft as they
    // now stand — so the agent reads what its rewrite became rather than being
    // told.
    let h = Harness::scripted(vec![
        Ok(proposes("artifact-window.md", "The accepted UI half.", "Leads with the point.")),
        Ok(ScriptedReply::answer("Glad it landed.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (first, draft_id, thread) = run_draft_turn(&h, "arch");
    assert_eq!(first.state, AgentTurnState::AwaitingReply);
    assert!(
        h.turns().in_flight(None).iter().any(|t| t.id == first.id),
        "AGC-FR-22: it is outstanding while the author has not decided",
    );

    // The author accepts, which writes the file and appends the decision.
    let proposal = crate::draft_proposals::list_proposals_impl(&h.root(), &draft_id)
        .expect("list")
        .remove(0);
    let decided = crate::draft_proposals::accept_for_test(
        &h.app.handle().clone(),
        &h.root(),
        &h.root(),
        &proposal.id,
        Some("Good — keep going."),
        &human("ada"),
    )
    .expect("accepted");
    assert_eq!(
        decided.proposal.state,
        crate::draft_proposals::ProposalState::Accepted,
    );

    // The decision comment is what dispatches the fresh turn (DCR-FR-15).
    let thread_now = folded_discussion(&h, &draft_id, &thread.id);
    let decision = thread_now.comments.last().expect("decision");
    let second = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &decision.id,
        )
        .expect("dispatch");
    wait_for_terminal(&h, &second.id);

    // AGC-FR-29: the awaiting turn retired rather than resuming.
    assert!(
        !h.turns().in_flight(None).iter().any(|t| t.id == first.id),
        "the awaiting turn is no longer outstanding",
    );

    let (request, _) = h.seam.requests().pop().expect("the second turn's request");
    let history = section_of(&request.input, TAG_DISCUSSION_HISTORY).body.clone();
    assert!(
        history.contains("Leads with the point."),
        "the agent's own proposal is in the history it re-reads: {history}",
    );
    // The decision is the comment that addressed the agent, so it occupies
    // `current_comment` rather than the history behind it (AGC-FR-06) — which is
    // where a reply to a question lands too.
    let current = section_of(&request.input, TAG_CURRENT_COMMENT).body.clone();
    assert!(
        current.contains("Accepted") && current.contains("Good — keep going."),
        "the decision and the author's feedback reach the agent: {current}",
    );
    let artifact = section_of(&request.input, TAG_ARTIFACT).body.clone();
    assert!(
        artifact.contains("The accepted UI half."),
        "AGC-FR-07: the artifact section carries the draft as it now stands: {artifact}",
    );
    assert!(
        !artifact.contains("The UI half.\n") || artifact.contains("The accepted UI half."),
        "rather than what the agent read when it proposed",
    );
}

// ---------------------------------------------------------------------------
// AGC-FR-21 — a reply asking the author ends the turn (CVL-FR-15, AGC-FR-01)
// ---------------------------------------------------------------------------

#[test]
fn a_turn_that_asks_the_author_ends_on_that_call() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "spec.md" }),
        )),
        Ok(asks("One spec or two?")),
        // Never reached: the turn ends on the ask.
        Ok(ScriptedReply::answer("unreachable")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(
        turn.state,
        AgentTurnState::AwaitingReply,
        "CVL-FR-15: the turn ends awaiting a reply",
    );
    assert_eq!(turn.failure, None, "CVL-FR-15: asking is not a failure");
    assert!(turn.ended_at.is_some());
    assert_eq!(
        h.seam.requests().len(),
        2,
        "CVL-FR-15: no further model call once the question posted",
    );

    // AGC-FR-01: exactly one contribution, and it is the question.
    let thread = folded(&h, "spec.md", &thread.id);
    assert_eq!(thread.comments.len(), 2);
    assert_eq!(thread.comments[1].body, "One spec or two?");
    assert!(matches!(
        thread.comments[1].author,
        Participant::Agent { .. }
    ));

    // AGC-FR-21: one terminal event for this turn and no other.
    let events = h
        .terminal
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|t| t.id == turn.id)
        .count();
    assert_eq!(events, 1);
}

// ---------------------------------------------------------------------------
// CVL-FR-15 — the rest of that reply is left undispatched (CVL-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn asking_leaves_the_rest_of_the_reply_undispatched_and_its_prose_unappended() {
    // A second question in the same reply is the one undispatched call that
    // leaves evidence: were it dispatched, the thread would gain two comments.
    // The read-only tools beside it leave none, so this is what makes
    // "every other call is left undispatched" observable rather than asserted.
    let h = Harness::scripted(vec![Ok(asks("One spec or two?")
        .and_calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": "Or three?" }),
        )
        .and_calls("read_file", serde_json::json!({ "path": "spec.md" }))
        .and_calls("search_specifications", serde_json::json!({ "query": "a" }))
        .with_text("I think it is two specs"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(
        h.seam.requests().len(),
        1,
        "CVL-FR-15: the loop ended on the ask",
    );

    let thread = folded(&h, "spec.md", &thread.id);
    assert_eq!(
        thread.comments.len(),
        2,
        "CVL-FR-15: the question alone — the reply's prose is appended nowhere",
    );
    assert_eq!(thread.comments[1].body, "One spec or two?");

    assert!(
        !thread.comments.iter().any(|c| c.body == "Or three?"),
        "CVL-FR-15: the second question in that reply was never dispatched — \
         one thing at a time",
    );
}

#[test]
fn a_turn_ending_on_a_question_reports_every_call_of_that_reply_finished() {
    // CVL-FR-31, CVL-FR-25 / CVL-FR-33 / AGC-FR-33, AGC-FR-34, AGC-FR-21: a call abandoned "by the turn ending on
    // the call itself" is reported finished like any other, so no call is left
    // active after the loop has stopped working on it. The reply asks for four,
    // one of which ends the turn and three of which are never dispatched.
    let h = Harness::scripted(vec![Ok(asks("One spec or two?")
        .and_calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": "Or three?" }),
        )
        .and_calls("read_file", serde_json::json!({ "path": "spec.md" }))
        .and_calls("search_specifications", serde_json::json!({ "query": "a" })))]);
    h.mount();
    h.create_agent("arch", "");

    let (tx, rx) = mpsc::channel::<AgentTurn>();
    h.app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
        if let Ok(turn) = serde_json::from_str::<AgentTurn>(event.payload()) {
            let _ = tx.send(turn);
        }
    });

    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::AwaitingReply);

    let payloads: Vec<AgentTurn> = rx.try_iter().filter(|t| t.id == terminal.id).collect();
    // The registration, four begins, four finishes, and the terminal event.
    assert_eq!(
        payloads.len(),
        2 + 4 * 2,
        "every call the reply asked for was reported begun and finished: {:?}",
        payloads
            .iter()
            .map(|t| t.active_tool_calls.len())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        payloads
            .iter()
            .map(|t| t.active_tool_calls.len())
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 3, 2, 1, 0, 0],
        "all four begun before any finished, then emptied one by one",
    );
    // AGC-FR-34: the terminal event carries an empty list whatever was active,
    // and no activity event carries the turn's id after it.
    let last = payloads.last().expect("a terminal event");
    assert!(last.ended_at.is_some() && last.active_tool_calls.is_empty());
    assert!(
        payloads[..payloads.len() - 1]
            .iter()
            .all(|t| t.state == AgentTurnState::Running),
        "AGC-FR-21: exactly one terminal event, and it is the last",
    );
    // AGC-FR-22: nothing is left active on the outstanding record either.
    let outstanding = h.turns().in_flight(None);
    assert_eq!(outstanding.len(), 1);
    assert!(outstanding[0].active_tool_calls.is_empty());
}

// ---------------------------------------------------------------------------
// AUC-FR-10, AUC-FR-12 — a refused question ends nothing (CVL-FR-15, CVL-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn a_refused_question_leaves_the_loop_running_and_the_turn_delivers() {
    // A blank question is the tool's own retryable refusal (AUC-FR-06), which
    // is the refusal branch of CVL-FR-15 reached deterministically.
    let h = Harness::scripted(vec![
        Ok(asks("   ")),
        Ok(ScriptedReply::answer("Two specs, then.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(
        turn.state,
        AgentTurnState::Delivered,
        "CVL-FR-15: a call that posted nothing ends nothing",
    );
    assert_eq!(
        h.seam.requests().len(),
        2,
        "CVL-FR-14: the refusal went back to the model and the loop continued",
    );

    let thread = folded(&h, "spec.md", &thread.id);
    assert_eq!(thread.comments.len(), 2);
    assert_eq!(
        thread.comments[1].body, "Two specs, then.",
        "the answer, and no question",
    );
}

// ---------------------------------------------------------------------------
// CVL-FR-16, AGC-FR-28, CVL-FR-24, AGC-FR-25, AGC-FR-24 — the turn is discarded, and nothing bounds the waiting
// (AGC-FR-28, CVL-FR-24, AGC-FR-25, AGC-FR-24, CVL-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn a_turn_awaiting_a_reply_holds_nothing_and_is_bounded_by_nothing() {
    // Under the *short* whole-turn deadline, so the wait below genuinely
    // outlives the clock CVL-FR-16 runs a turn against. Against the generous
    // deadline this test would pass even if an awaiting turn were collected
    // when its turn's time ran out.
    let h = Harness::scripted_impatient(vec![Ok(asks("One spec or two?"))], Duration::ZERO);
    h.mount();
    h.create_agent("arch", "");
    let (turn, _) = run_one(&h, "arch");
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);

    // CVL-FR-24: the session and its temp directory went with the turn.
    wait_for_sessions(&h, 0);

    // AGC-FR-24: the progress operation ended — an agent waiting on a person is
    // not working.
    let operations = h.progress().in_flight();
    assert!(
        !operations.iter().any(|op| op.kind == "agent"),
        "AGC-FR-24: no agent operation is still in flight: {operations:?}",
    );

    // AGC-FR-25: the slot was surrendered, so the bound is fully available.
    assert_eq!(
        h.turns().permits_available(),
        DEFAULT_CONCURRENCY,
        "AGC-FR-25: a turn that asked holds no slot while it waits",
    );

    // CVL-FR-16: the whole-turn timeout is spent, and nothing bounds the wait —
    // it is not collected, not retried, and not reported as a failure. Slept
    // well past the harness's own turn deadline, because a sleep shorter than it
    // would pass just as happily against an implementation that *did* collect an
    // awaiting turn when the turn's clock ran out.
    let waited = SHORT_TIMEOUT * 3;
    std::thread::sleep(waited);
    assert!(waited > SHORT_TIMEOUT, "the wait outlasts the turn deadline");
    let still = h.turns().in_flight(None);
    assert_eq!(still.len(), 1);
    assert_eq!(still[0].state, AgentTurnState::AwaitingReply);
    assert_eq!(still[0].failure, None);
}

// ---------------------------------------------------------------------------
// AGC-FR-22, AGC-FR-29, AGC-FR-21 — outstanding, and retired by the dispatch its answer causes
// (AGC-FR-22, AGC-FR-29, AGC-FR-21)
// ---------------------------------------------------------------------------

#[test]
fn an_awaiting_turn_is_outstanding_until_a_dispatch_for_that_agent_retires_it() {
    let h = Harness::scripted(vec![
        Ok(asks("One spec or two?")),
        Ok(ScriptedReply::answer("Two specs, then.")),
        Ok(ScriptedReply::answer("Unrelated.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    h.create_agent("sec", "");

    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let asked = h
        .dispatch("arch", origin.clone(), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &asked.id);

    // AGC-FR-22: outstanding, so a surface mounted later knows a reply is owed.
    let outstanding = h.turns().in_flight(Some(&origin));
    assert_eq!(outstanding.len(), 1);
    assert_eq!(outstanding[0].id, asked.id);
    assert_eq!(outstanding[0].state, AgentTurnState::AwaitingReply);

    // A dispatch for another agent in the same conversation leaves it alone.
    let other = h
        .dispatch("sec", origin.clone(), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &other.id);
    assert!(
        h.turns()
            .in_flight(Some(&origin))
            .iter()
            .any(|t| t.id == asked.id),
        "AGC-FR-29: only a dispatch for the same agent retires it",
    );

    // AGC-FR-29: the author's answer dispatches for `arch`, which retires it.
    let events_before = h
        .terminal
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|t| t.id == asked.id)
        .count();
    let answering = h
        .dispatch("arch", origin.clone(), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &answering.id);

    assert!(
        !h.turns()
            .in_flight(Some(&origin))
            .iter()
            .any(|t| t.id == asked.id),
        "AGC-FR-29: the wait is over",
    );
    // Retired rather than forgotten: it delivered, its contribution being the
    // question. An implementation that dropped the entry without recording it
    // would leave the turn unaccountable to a late cancellation (AGC-FR-20).
    assert_eq!(
        h.turns().terminated(&asked.id).map(|t| t.state),
        Some(AgentTurnState::Delivered),
        "AGC-FR-29: retired to `delivered`",
    );
    let events_after = h
        .terminal
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|t| t.id == asked.id)
        .count();
    assert_eq!(
        events_before, events_after,
        "AGC-FR-21 / AGC-FR-29: retirement emits nothing",
    );
}

// ---------------------------------------------------------------------------
// AGC-FR-29, AGC-FR-13, AGC-FR-06 — the answer is a fresh turn carrying the whole history
// (AGC-FR-29, AGC-FR-13, AGC-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn the_answering_turn_reads_the_question_and_the_answer_as_ordinary_history() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "spec.md" }),
        )),
        Ok(asks("One spec or two?")),
        Ok(ScriptedReply::answer("Two specs, then.")),
    ]);
    h.mount();
    h.create_agent("arch", "");

    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let asked = h
        .dispatch("arch", origin.clone(), &thread.comments[0].id)
        .expect("dispatch");
    let asked_terminal = wait_for_terminal(&h, &asked.id);
    assert_eq!(
        asked_terminal.state,
        AgentTurnState::AwaitingReply,
        "the first turn asked: {:?}",
        asked_terminal.failure,
    );

    // The author answers, in their own words.
    // Stamped later than the question the tool posted, which carries the real
    // clock: the fold orders by `at`, so a fixture timestamp in the past would
    // sort the author's answer *before* the question it answers.
    let answered = crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "two, definitely".into(),
        Vec::new(),
        Vec::new(),
        &human("ada"),
        "2099-01-01T00:00:00Z",
    )
    .expect("reply");
    let reply_id = answered
        .comments
        .iter()
        .find(|c| c.body == "two, definitely")
        .expect("the reply")
        .id
        .clone();

    let answering = h
        .dispatch("arch", origin.clone(), &reply_id)
        .expect("dispatch");
    wait_for_terminal(&h, &answering.id);

    // AGC-FR-29 / AGC-FR-13: a fresh assembly of the conversation as it now
    // stands, with the question and the answer among its history.
    let (request, _) = h.seam.requests().last().cloned().expect("a request");
    let history = request
        .input
        .iter()
        .find(|s| s.tag == TAG_DISCUSSION_HISTORY)
        .expect("discussion history")
        .body
        .clone();
    assert!(
        history.contains("One spec or two?"),
        "AGC-FR-29: the agent's own question is in the history — {history}",
    );
    // AGC-FR-06: the comment that addressed the agent is the current comment,
    // so the answer sits there rather than in the history behind it.
    let current = request
        .input
        .iter()
        .find(|s| s.tag == TAG_CURRENT_COMMENT)
        .expect("current comment")
        .body
        .clone();
    assert!(
        current.contains("two, definitely"),
        "AGC-FR-29: the author's answer is what this turn was asked about — {current}",
    );

    // Nothing of the earlier turn carried over: this exchange opens with its
    // own input and holds no tool result the first turn gathered.
    // The answering turn is a turn like any other: its exchange opens with its
    // own input and carries nothing the turn that asked had gathered.
    let exchanges = h.seam.exchanges();
    let answering_exchange = exchanges.last().expect("an exchange");
    assert_eq!(
        answering_exchange.len(),
        1,
        "AGC-FR-29: the answering turn begins with its input alone",
    );
}
