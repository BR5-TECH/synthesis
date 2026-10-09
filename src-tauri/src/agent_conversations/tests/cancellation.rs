//! Stopping a turn, and what deleting or closing the thing it was about
//! does to one that is still running.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// AGC-FR-20 / AGR-FR-11, AGR-FR-15: cancellation
// ---------------------------------------------------------------------------

#[test]
fn cancelling_terminates_the_turn_appends_nothing_and_is_idempotent() {
    // AGC-FR-20.
    let h = Harness::with(
        vec![Ok("never delivered".into())],
        Duration::from_millis(400),
        4,
    );
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");

    // Cancel while the model call is actually in flight rather than while the
    // worker is still resolving. The seam records a request as it enters, so
    // this waits on the call rather than on a clock: without it a preempted
    // test thread could reach the cancel after the seam's delay had already
    // elapsed, and the turn would be `delivered` rather than `cancelled`.
    for _ in 0..1000 {
        if h.seam.requests_len() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(h.seam.requests().len(), 1, "the model call never started");

    let handle = h.app.handle().clone();
    let cancelled =
        cancel_impl(&handle, &h.turns(), &h.progress(), &turn.id).expect("cancel");
    assert_eq!(cancelled.state, AgentTurnState::Cancelled);
    // AGC-FR-20: a second cancel of the same turn succeeds and changes nothing.
    let again =
        cancel_impl(&handle, &h.turns(), &h.progress(), &turn.id).expect("idempotent");
    assert_eq!(again, cancelled);

    // The model call is still running on its own thread, and the count below is
    // a **negative** assertion — it passes trivially against a worker that has
    // not got there yet. `settle()` is not enough either: `terminate` empties
    // the in-flight set, so it returns while the worker is still inside the
    // call. So wait for the worker to reach its own end, which it announces by
    // discarding the answer it can no longer deliver (AGC-FR-20). A clock could
    // only ever say "not yet, probably".
    wait_for_record(&turn.id, "agent answer discarded: the turn was cancelled");
    assert_eq!(
        h.threads("spec.md")[0].comments.len(),
        1,
        "a cancelled turn contributes nothing",
    );
}

#[test]
fn cancelling_a_delivered_turn_leaves_the_comment_and_the_state_alone() {
    // AGC-FR-20's second clause / AGC-FR-20: a cancellation racing a delivery
    // never rewrites what was delivered.
    let h = Harness::new(vec![Ok("Delivered before the cancel.".into())]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let delivered = wait_for_terminal(&h, &turn.id);
    assert_eq!(delivered.state, AgentTurnState::Delivered);

    let late = cancel_impl(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &turn.id,
    )
    .expect("a late cancel still succeeds");
    assert_eq!(late.state, AgentTurnState::Delivered);
    assert_eq!(h.threads("spec.md")[0].comments.len(), 2);
    assert_eq!(
        h.threads("spec.md")[0].comments[1].body,
        "Delivered before the cancel.",
    );
}

#[test]
fn a_cancel_that_arrives_after_the_delivery_claim_cannot_take_the_turn() {
    // AGC-FR-20's race, driven at the registry rather than through a sleep: the
    // window between "the answer is in hand" and "the line is on disk" is a file
    // write, and a test that tried to hit it by timing would be measuring the
    // scheduler. What makes the race unwinnable is that the claim and the
    // cancellation take one lock, and that is what this asserts.
    let h = Harness::with_seam(
        Box::new(GatedCompletion {
            gate: Arc::new((Mutex::new(true), Condvar::new())),
            reply: "unused".into(),
        }),
        1,
    );
    let agent = h.create_agent("arch", "");
    let (turn, cancelled) = h.turns().register(
        &agent,
        ConversationOrigin::stub_artifact("t1", "a.md", true),
        "c1",
        PROJECT_KEY,
    );

    // Before the claim, a cancellation takes the turn.
    assert!(h.turns().begin_delivery(&turn.id), "the claim is available");
    // After it, `terminate` refuses to cancel — which is the whole guarantee.
    cancelled.store(true, Ordering::SeqCst);
    assert!(
        h.turns()
            .terminate(&turn.id, AgentTurnState::Cancelled, None)
            .is_none(),
        "a committed delivery cannot be cancelled out from under its append",
    );
    // And the delivering thread's own terminate still lands.
    let (delivered, _, _, _) = h
        .turns()
        .terminate(&turn.id, AgentTurnState::Delivered, None)
        .expect("the delivering thread terminates its own turn");
    assert_eq!(delivered.state, AgentTurnState::Delivered);

    // A cancellation arriving after all that reports what actually happened.
    let late = cancel_impl(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &turn.id,
    )
    .expect("a late cancel still succeeds");
    assert_eq!(late.state, AgentTurnState::Delivered);
}

#[test]
fn a_cancel_before_the_claim_stops_the_delivery() {
    // The other side of the same lock: a turn whose cancel flag is set cannot
    // claim delivery, so it appends nothing (AGC-FR-18).
    let h = Harness::new(vec![]);
    let agent = h.create_agent("arch", "");
    let (turn, cancelled) = h.turns().register(
        &agent,
        ConversationOrigin::stub_artifact("t1", "a.md", true),
        "c1",
        PROJECT_KEY,
    );
    cancelled.store(true, Ordering::SeqCst);
    assert!(!h.turns().begin_delivery(&turn.id));
}

#[test]
fn a_provider_that_never_answers_is_abandoned_at_the_bound() {
    // CVL-FR-17, CVL-FR-18, AGC-FR-18 / CVL-FR-16. The registry's timeout is 400ms in this harness,
    // and `NeverAnswers` drives the *real* `block_on_with_timeout` — so what is
    // exercised is the bound itself, not a scripted `timed_out` string.
    let h = Harness::with_seam(Box::new(NeverAnswers), 1);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");

    // "Abandoned rather than waited on" needs no stopwatch: the seam never
    // returns, so a turn that waited on it would never reach a terminal state at
    // all and `wait_for_terminal` would run out and say so. Reaching `Failed`
    // with `FAIL_TIMED_OUT` IS the claim — a wall-clock bound beside it measured
    // the machine and asserted nothing the two lines above had not already.
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    assert_eq!(
        h.threads("spec.md")[0].comments.len(),
        1,
        "an abandoned turn appends nothing",
    );
}



struct NeverAnswers;

impl CompletionSeam for NeverAnswers {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        block_on_with_timeout(
            async {
                std::future::pending::<()>().await;
                Ok(ModelReply::default())
            },
            timeout,
        )
    }
}

/// A seam that blocks until the test releases it, so a race can be driven
/// deterministically rather than slept at.
struct GatedCompletion {
    gate: Arc<(Mutex<bool>, Condvar)>,
    reply: String,
}

impl CompletionSeam for GatedCompletion {
    fn complete(
        &self,
        _request: &AgentRequest,
        _exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        let (lock, cvar) = &*self.gate;
        let mut released = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !*released {
            released = cvar.wait(released).unwrap_or_else(|e| e.into_inner());
        }
        Ok(ModelReply {
            text: self.reply.clone(),
            tool_calls: Vec::new(),
            native_calls: Vec::new(),
            native_usage: None,
            native_entries_dropped: 0,
            text_format_repaired: false,
            served_by: None,
            prompt_tokens: None,
            output_tokens: None,
            input_tokens: None,
        })
    }
}

#[test]
fn deleting_an_agent_cancels_its_turns_and_a_withdrawal_cancels_only_that_projects() {
    // AGC-FR-20, per AGR-FR-11 and AGR-FR-15.
    let h = Harness::with(
        vec![Ok("a".into()), Ok("b".into())],
        Duration::from_millis(500),
        4,
    );
    let arch = h.create_agent("arch", "");
    let sec = h.create_agent("sec", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let trigger = thread.comments[0].id.clone();
    let arch_turn = h.dispatch("arch", origin.clone(), &trigger).expect("arch");
    let sec_turn = h.dispatch("sec", origin, &trigger).expect("sec");
    assert_eq!(h.turns().in_flight(None).len(), 2);

    // Both model calls must be **in flight** before anything is cancelled: the
    // race AGC-FR-20 is about is a cancellation meeting a running call, and a
    // preempted test thread that cancelled after the seam's delay had elapsed
    // would be testing a delivered turn instead. The seam records a request as
    // it enters, so this waits on the calls rather than on a clock.
    for _ in 0..1000 {
        if h.seam.requests_len() == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(h.seam.requests().len(), 2, "both model calls never started");

    let handle = h.app.handle().clone();
    agents::delete_agent_impl(&h.store(), &arch.id).expect("delete");
    cancel_turns_for_agent(&handle, &h.turns(), &h.progress(), &arch.id, None);
    let live = h.turns().in_flight(None);
    assert_eq!(live.len(), 1, "only the deleted agent's turn was cancelled");
    assert_eq!(live[0].agent_id, sec.id);

    // A withdrawal from *another* project leaves this project's turn alone.
    cancel_turns_for_agent(
        &handle,
        &h.turns(),
        &h.progress(),
        &sec.id,
        Some("some-other-project"),
    );
    assert_eq!(h.turns().in_flight(None).len(), 1);

    cancel_turns_for_agent(&handle, &h.turns(), &h.progress(), &sec.id, Some(PROJECT_KEY));
    assert!(h.turns().in_flight(None).is_empty());
    // The count below is a **negative** assertion, so it passes trivially
    // against workers that have not reached their delivery yet. Both workers
    // are waited out on the record each one writes as it discards the answer it
    // can no longer deliver — the loop this replaces named neither turn and
    // slept past both, which is the same as not waiting at all.
    for id in [&arch_turn.id, &sec_turn.id] {
        wait_for_record(id, "agent answer discarded: the turn was cancelled");
    }
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1);
}

// ---------------------------------------------------------------------------
// AGC-FR-18 — cancellation, deletion, and closing (AGC-FR-20, AGC-FR-28)
// ---------------------------------------------------------------------------

#[test]
fn an_awaiting_turn_is_unchanged_by_cancel_and_discarded_with_its_agent() {
    let h = Harness::scripted(vec![Ok(asks("One spec or two?"))]);
    h.mount();
    let agent = h.create_agent("arch", "");
    let (turn, thread) = run_one(&h, "arch");
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);

    // AGC-FR-20: it has terminated, so a cancel succeeds and changes nothing.
    let cancelled = cancel_impl(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &turn.id,
    )
    .expect("cancel succeeds against a terminated turn");
    assert_eq!(cancelled.state, AgentTurnState::AwaitingReply);
    assert_eq!(
        folded(&h, "spec.md", &thread.id).comments.len(),
        2,
        "AGC-FR-18: the question stays where it was posted",
    );

    // AGC-FR-28: the agent is gone, so what it was owed goes with it.
    cancel_turns_for_agent(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &agent.id,
        Some(PROJECT_KEY),
    );
    assert!(
        h.turns().in_flight(None).is_empty(),
        "AGC-FR-28: an agent that is gone is owed nothing",
    );
    assert_eq!(
        folded(&h, "spec.md", &thread.id).comments.len(),
        2,
        "and its question is still in the conversation",
    );
}

#[test]
fn closing_the_project_discards_what_agents_were_owed() {
    let h = Harness::scripted(vec![Ok(asks("One spec or two?"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, _) = run_one(&h, "arch");
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.turns().in_flight(None).len(), 1);

    cancel_all_turns(&h.app.handle().clone(), &h.turns(), &h.progress());
    assert!(
        h.turns().in_flight(None).is_empty(),
        "AGC-FR-28: a closed project holds no conversation to be owed one in",
    );
}

#[test]
fn a_second_question_in_one_reply_is_never_posted_even_when_the_first_refused() {
    // CVL-FR-15 / AGC-FR-01. The dangerous shape: the first ask refuses, so the
    // loop keeps running — and the second must not then post, or the turn would
    // contribute a question *and* an answer.
    let h = Harness::scripted(vec![
        Ok(asks("   ").and_calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!({ "question": "Or three?" }),
        )),
        Ok(ScriptedReply::answer("Two specs, then.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, thread) = run_one(&h, "arch");

    assert_eq!(
        turn.state,
        AgentTurnState::Delivered,
        "the first ask refused, so the turn ran on",
    );

    let thread = folded(&h, "spec.md", &thread.id);
    assert!(
        !thread.comments.iter().any(|c| c.body == "Or three?"),
        "CVL-FR-15: the second question was never posted",
    );
    assert_eq!(
        thread.comments.len(),
        2,
        "AGC-FR-01: one contribution — the answer — and no question beside it",
    );
    assert_eq!(thread.comments[1].body, "Two specs, then.");

    // The model was told why, rather than left with a call nothing answered.
    let second = &h.seam.exchanges()[1];
    let results: Vec<String> = second
        .iter()
        .filter_map(|message| match message {
            rig::completion::Message::User { content } => Some(
                content
                    .iter()
                    .filter_map(|c| match c {
                        rig::completion::message::UserContent::ToolResult(result) => Some(
                            result
                                .content
                                .iter()
                                .map(|part| match part {
                                    rig::completion::message::ToolResultContent::Text(text) => {
                                        text.text.clone()
                                    }
                                    _ => String::new(),
                                })
                                .collect::<String>(),
                        ),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(
        results
            .iter()
            .any(|r| r.contains("one question can be asked at a time")),
        "the second call was answered rather than left dangling: {results:?}",
    );
}


#[test]
fn a_withdrawal_from_one_project_leaves_another_projects_question_owed() {
    // AGC-FR-28, per AGR-FR-15: a withdrawal is from this conversation, not from
    // the machine. Without the `project_key` predicate on the discard, removing
    // `arch` from one project would forget what it is owed everywhere.
    const OTHER_PROJECT: &str = "other-proj";
    let h = Harness::scripted(vec![
        Ok(asks("One spec or two?")),
        Ok(asks("And the archive part?")),
    ]);
    h.mount();
    let agent = h.create_agent("arch", "");
    // The same persona, enrolled in a second project — which is the whole point:
    // a persona is described once and enrolled per project (AGR-FR-15).
    agents::enrol_project_agent_impl(&h.store(), OTHER_PROJECT, &agent.id).expect("enrol");

    let here = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let there = h.seed_artifact_thread("other.md", "More source.");

    let owed_here = h
        .dispatch_in(
            PROJECT_KEY,
            "arch",
            ConversationOrigin::of(&here),
            &here.comments[0].id,
        )
        .expect("dispatch");
    wait_for_terminal(&h, &owed_here.id);
    let owed_there = h
        .dispatch_in(
            OTHER_PROJECT,
            "arch",
            ConversationOrigin::of(&there),
            &there.comments[0].id,
        )
        .expect("dispatch");
    wait_for_terminal(&h, &owed_there.id);
    assert_eq!(h.turns().in_flight(None).len(), 2);

    // Withdrawn from this project alone.
    cancel_turns_for_agent(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &agent.id,
        Some(PROJECT_KEY),
    );

    let outstanding: Vec<String> = h
        .turns()
        .in_flight(None)
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(
        outstanding,
        vec![owed_there.id.clone()],
        "AGC-FR-28: only this project's registration was discarded",
    );

    // Deleted from the machine: everything it was owed goes, whichever project.
    cancel_turns_for_agent(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &agent.id,
        None,
    );
    assert!(h.turns().in_flight(None).is_empty());
}

#[test]
fn a_withdrawal_leaves_another_agents_question_owed() {
    // The other half of the predicate: the discard is by agent as well as by
    // project, so withdrawing one agent forgets nothing another is owed.
    let h = Harness::scripted(vec![Ok(asks("One?")), Ok(asks("Two?"))]);
    h.mount();
    let arch = h.create_agent("arch", "");
    h.create_agent("sec", "");

    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    for nickname in ["arch", "sec"] {
        let turn = h
            .dispatch(nickname, origin.clone(), &thread.comments[0].id)
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);
    }
    assert_eq!(h.turns().in_flight(None).len(), 2);

    cancel_turns_for_agent(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &arch.id,
        Some(PROJECT_KEY),
    );

    let left: Vec<String> = h
        .turns()
        .in_flight(None)
        .into_iter()
        .map(|t| t.nickname)
        .collect();
    assert_eq!(left, vec!["sec".to_string()]);
}

#[test]
fn nothing_about_a_question_awaiting_an_answer_reaches_disk() {
    // AGC-FR-28, AGC-FR-29, CTA-FR-LCFU, CTA-FR-QUXJ, CTA-FR-JQDM / AGC-FR-23: the registration is memory, so a relaunch finds
    // nothing recording that a reply is owed — which costs the conversation
    // nothing, routing never having consulted it: who a later comment reaches is
    // read back out of the comments the relaunch did not touch (CTA-FR-EVGD,
    // CTA-FR-BMBL).
    let h = Harness::scripted(vec![Ok(asks("One spec or two?"))]);
    h.mount();
    h.create_agent("arch", "");
    let (turn, thread) = run_one(&h, "arch");
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);

    let on_disk = crate::tools::tests::tree_snapshot(h.root.path());
    let rendered = String::from_utf8_lossy(
        &on_disk
            .iter()
            .flat_map(|(path, bytes)| {
                let mut out = path.clone().into_bytes();
                out.extend_from_slice(bytes);
                out
            })
            .collect::<Vec<u8>>(),
    )
    .to_string();
    assert!(
        !rendered.contains("awaiting_reply"),
        "AGC-FR-23: no file records that a reply is owed",
    );
    assert!(
        !rendered.contains(&turn.id),
        "AGC-FR-23: no file names the turn",
    );
    // The question itself is in the conversation, because that is a comment.
    assert_eq!(folded(&h, "spec.md", &thread.id).comments.len(), 2);
}
