//! A turn that asks the author a set of questions, and the submission that
//! answers it.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::asking_author::{folded_discussion, run_draft_turn};
use super::*;

// ---------------------------------------------------------------------------
// The question set (ADQ-FR-LFDX … ADQ-FR-FQPA, AUC-FR-QSVN)
// ---------------------------------------------------------------------------

fn asks_questions(questions: serde_json::Value) -> ScriptedReply {
    ScriptedReply::calls(
        crate::tools::ask_discussion_questions::NAME,
        serde_json::json!({ "questions": questions }),
    )
}

fn two_questions() -> serde_json::Value {
    serde_json::json!([
        { "question": "One spec or two?", "options": [
            { "label": "one", "description": "Easy to find; it grows large." },
            { "label": "two", "description": "Each stays small." },
        ] },
        { "question": "Where does the diagram go?", "options": [
            { "label": "ui", "description": "Beside the screens it explains." },
            { "label": "core", "description": "Beside the data it moves." },
        ] },
    ])
}

/// CVL-FR-15, ADQ-FR-FQPA, ADQ-FR-DHZK, AGC-FR-VRHM: a turn that records a
/// question set ends on that call, awaiting the author, and appends **no
/// comment** — the one `awaiting_reply` carrying no ordinary contribution.
#[test]
fn a_turn_that_records_a_question_set_ends_on_that_call_and_appends_nothing() {
    let h = Harness::scripted(vec![
        Ok(asks_questions(two_questions())),
        // Never reached: the turn ends on the recorded set.
        Ok(ScriptedReply::answer("unreachable")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(
        turn.state,
        AgentTurnState::AwaitingReply,
        "CVL-FR-15: the turn ends awaiting the author's answers",
    );
    assert_eq!(turn.failure, None, "recording a set is not a failure");
    assert_eq!(h.seam.requests().len(), 1, "CVL-FR-15: the loop makes no further model call",);

    // ADQ-FR-DHZK: the discussion's log is what it was — the opening comment and
    // nothing else.
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(
        folded.comments.len(),
        1,
        "a recorded set appends no comment: {:?}",
        folded.comments.iter().map(|c| &c.body).collect::<Vec<_>>(),
    );

    // ADQ-FR-GMDK: and the durable set is what the author sees instead.
    let set = crate::comments::read_question_set(&h.root(), &h.root(), &thread.id)
        .expect("the set was recorded");
    assert_eq!(set.questions.len(), 2);
    assert_eq!(set.questions[0].text, "One spec or two?");
    assert_eq!(set.discussion_id, thread.id);
}

/// ADQ-FR-BSYE, CVL-FR-14: a refused call ends **nothing**. The refusal reaches
/// the model like any other result and the turn answers with what it has.
#[test]
fn a_refused_question_set_ends_nothing_and_the_turn_answers_with_what_it_has() {
    let h = Harness::scripted(vec![
        // Two options are required, so one refuses (ADQ-FR-PVXK).
        Ok(asks_questions(serde_json::json!([
            { "question": "One spec or two?", "options": [
                { "label": "only one", "description": "The one way." },
            ] },
        ]))),
        Ok(ScriptedReply::answer("Answered with what I had.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, _draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(
        turn.state,
        AgentTurnState::Delivered,
        "CVL-FR-14: a tool refusal is not a turn's failure and ends nothing",
    );
    assert_eq!(h.seam.requests().len(), 2, "the loop carried on and asked the model again",);
    // ADQ-FR-CJRM: nothing was recorded, so the author is never shown a set the
    // agent did not mean to ask.
    assert!(crate::comments::read_question_set(&h.root(), &h.root(), &thread.id).is_none());
}

/// CVL-FR-15, AGC-FR-01: a reply asking for both turn-ending tools makes **one**
/// contribution. The first is dispatched and the second is not carried out.
#[test]
fn a_reply_asking_for_a_set_and_a_question_makes_one_contribution() {
    let reply = asks_questions(two_questions()).and_calls(
        crate::tools::ask_user_comment::NAME,
        serde_json::json!({ "question": "And one more thing?" }),
    );
    let h = Harness::scripted(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, draft_id, thread) = run_draft_turn(&h, "arch");

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    // The set went out and the comment did not: one contribution, not two.
    assert!(crate::comments::read_question_set(&h.root(), &h.root(), &thread.id).is_some());
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(folded.comments.len(), 1, "the second turn-ending call was carried out as well",);
}

/// AUC-FR-QSVN: `ask_user_comment` refuses while a set stands, appends no
/// comment, and does not end the calling turn — a discussion holding a set the
/// author is answering must not gain a question they cannot reply to.
#[test]
fn asking_one_question_is_refused_while_a_question_set_stands() {
    let h = Harness::scripted(vec![
        // The first turn records the set and ends there.
        Ok(asks_questions(two_questions())),
        // The second tries to ask openly instead, and is refused.
        Ok(ScriptedReply::calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": "And one more thing?" }),
        )),
        Ok(ScriptedReply::answer("Answered with what I had.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (first, draft_id, thread) = run_draft_turn(&h, "arch");
    assert_eq!(first.state, AgentTurnState::AwaitingReply);
    assert!(crate::comments::read_question_set(&h.root(), &h.root(), &thread.id).is_some());

    let second = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let settled = wait_for_terminal(&h, &second.id);

    assert_eq!(
        settled.state,
        AgentTurnState::Delivered,
        "AUC-FR-QSVN: the refusal does not end the calling turn",
    );
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    assert_eq!(
        folded.comments.len(),
        2,
        "the opening comment and the delivered answer, and no posted question: {:?}",
        folded.comments.iter().map(|c| &c.body).collect::<Vec<_>>(),
    );
    // And the standing set is exactly as it was.
    let set = crate::comments::read_question_set(&h.root(), &h.root(), &thread.id)
        .expect("the set still stands");
    assert_eq!(set.questions.len(), 2);
}

/// AGC-FR-TQLC, ADQ-FR-ZXAF: a submission retires the asking turn whether or
/// not that agent is among the agents it dispatches to.
///
/// The turn ended before there was an answer and is never resumed, re-entered,
/// or continued. The ordinary retirement is per agent at dispatch, which would
/// leave an agent that asked and was not dispatched to awaiting a reply for
/// good.
#[test]
fn a_submission_retires_the_turn_that_asked() {
    let h = Harness::scripted(vec![Ok(asks_questions(two_questions()))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, _draft_id, thread) = run_draft_turn(&h, "arch");
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);

    // No agent is named: the retirement is by discussion, so it reaches a turn
    // whose agent this submission would never dispatch to.
    let retired = h.turns().retire_awaiting_on_thread(&thread.id);
    assert_eq!(retired, 1, "AGC-FR-TQLC: the turn that asked is retired");

    // And it is gone rather than merely marked, so a second call retires none.
    assert_eq!(h.turns().retire_awaiting_on_thread(&thread.id), 0);
}

/// AGC-FR-TQLC: and it retires the turns of **that** discussion alone.
#[test]
fn retiring_one_discussions_turns_leaves_another_discussions_alone() {
    let h = Harness::scripted(vec![
        Ok(asks_questions(two_questions())),
        Ok(asks_questions(two_questions())),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (_first, _draft_a, thread_a) = run_draft_turn(&h, "arch");
    let (_second, _draft_b, thread_b) = run_draft_turn(&h, "arch");
    assert_ne!(thread_a.id, thread_b.id);

    assert_eq!(h.turns().retire_awaiting_on_thread(&thread_a.id), 1);
    // The other discussion's turn is still owed its answer.
    assert_eq!(h.turns().retire_awaiting_on_thread(&thread_b.id), 1);
}

/// ADQ-FR-ZKXL, ADQ-FR-QUKU: the turn a submission dispatches reads each
/// question comment by its labels. The descriptions the set recorded stay on
/// the comment and out of every section of the input.
#[test]
fn the_turn_after_a_submission_reads_labels_and_no_description() {
    let h = Harness::scripted(vec![
        Ok(asks_questions(two_questions())),
        Ok(ScriptedReply::answer("Then two specs, with the diagram in ui.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (_, draft_id, thread) = run_draft_turn(&h, "arch");

    let answers = vec![
        crate::comments::QuestionAnswer {
            question_position: 1,
            option_position: Some(2),
            option_value: Some("two".into()),
            own_answer: None,
            note: None,
        },
        crate::comments::QuestionAnswer {
            question_position: 2,
            option_position: Some(1),
            option_value: Some("ui".into()),
            own_answer: None,
            note: None,
        },
    ];
    let set = crate::comments::read_question_set(&h.root(), &h.root(), &thread.id).unwrap();
    let human = crate::comments::Participant::Human {
        login: "raver119".into(),
        display_name: None,
        email: None,
    };
    let outcome = crate::comments::commit_question_answers(
        &h.root(),
        &h.root(),
        &thread.id,
        &set.set_id,
        &answers,
        &human,
    )
    .unwrap();
    let folded = folded_discussion(&h, &draft_id, &thread.id);
    assert!(
        folded.comments.iter().any(|c| !c.option_descriptions.is_empty()),
        "the question comments carry their descriptions",
    );

    h.dispatch("arch", ConversationOrigin::of(&folded), &outcome.submitted.final_answer_comment_id)
        .expect("dispatch");
    h.settle();

    let (request, _) = &h.seam.requests()[1];
    let rendered = render_input(request);
    assert!(rendered.contains("1. one\n2. two"), "{rendered}");
    assert!(rendered.contains("**Selected option:** two"), "{rendered}");
    for description in [
        "Easy to find; it grows large.",
        "Each stays small.",
        "Beside the screens it explains.",
        "Beside the data it moves.",
    ] {
        assert!(!rendered.contains(description), "{description} reached the turn");
    }
}
