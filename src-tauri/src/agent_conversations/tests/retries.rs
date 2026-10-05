//! Bounded automatic retries, and the registry a recovered turn is read
//! back through.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// CVL-FR-19, CVL-FR-20, AGC-FR-21, CVL-FR-12 … CVL-FR-22: bounded automatic retries and the recovery registry
// ---------------------------------------------------------------------------

/// Script `failure` for `n` physical attempts, then answer.
fn failing_then_answering(n: usize, failure: &'static str, answer: &str) -> Harness {
    let mut script: Vec<Result<String, &'static str>> = vec![Err(failure); n];
    script.push(Ok(answer.into()));
    Harness::new(script)
}

#[test]
fn a_transient_failure_is_repeated_until_it_answers() {
    // CVL-FR-19, CVL-FR-20, AGC-FR-21, CVL-FR-12.
    for failures_before_success in [1usize, 2] {
        let h = failing_then_answering(failures_before_success, FAIL_UNREACHABLE, "the answer");
        h.create_agent("arch", "");
        let (terminal, _) = run_one(&h, "arch");

        assert_eq!(terminal.state, AgentTurnState::Delivered);
        assert_eq!(
            h.seam.call_count(),
            failures_before_success + 1,
            "the seam was reached once per physical attempt",
        );
        let threads = h.threads("spec.md");
        assert_eq!(threads[0].comments.len(), 2, "exactly one comment was appended");
        assert_eq!(threads[0].comments[1].body, "the answer");
        // AGC-FR-21 / CVL-FR-20: nothing about a retry reaches a surface. Two
        // events for the turn — the registration and the terminal state — and
        // none for the attempts in between.
        let events = h
            .events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|t| t.id == terminal.id)
            .count();
        assert_eq!(events, 2, "a retry emits no event of its own");
        // AGC-FR-31: a turn that succeeded leaves no offer behind.
        assert!(h.turns().recoverable_failures(None).is_empty());
    }
}

#[test]
fn an_exhausted_budget_fails_the_turn_for_each_recoverable_class() {
    // CVL-FR-18, CVL-FR-19, AGC-FR-18, AGC-FR-31.
    for failure in [FAIL_UNREACHABLE, FAIL_TIMED_OUT, FAIL_EMPTY_REPLY] {
        let h = Harness::new(vec![Err(failure); MAX_ATTEMPTS]);
        h.create_agent("arch", "");
        let (terminal, thread) = run_one(&h, "arch");

        assert_eq!(terminal.state, AgentTurnState::Failed, "{failure}");
        assert_eq!(terminal.failure.as_deref(), Some(failure));
        assert!(terminal.retry_permitted, "{failure} is recoverable");
        assert_eq!(h.seam.call_count(), MAX_ATTEMPTS, "attempts for {failure}");
        assert_eq!(
            h.threads("spec.md")[0].comments.len(),
            1,
            "the thread gained no line for {failure}",
        );
        // AGC-FR-31: the offer stands, naming the same agent, origin, and
        // trigger comment the failed turn named.
        let entries = h.turns().recoverable_failures(None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, terminal.id);
        assert_eq!(entries[0].nickname, "arch");
        assert_eq!(entries[0].trigger_comment_id, thread.comments[0].id);
    }
}

#[test]
fn an_empty_reply_carrying_neither_prose_nor_a_tool_request_is_repeated() {
    // CVL-FR-18, CVL-FR-19, AGC-FR-18 (the `empty_reply` half, through the ordinary reply path rather
    // than a scripted transport error) / CVL-FR-18, CVL-FR-19.
    let script: Vec<Result<ScriptedReply, &'static str>> =
        (0..MAX_ATTEMPTS).map(|_| Ok(ScriptedReply::answer(""))).collect();
    let h = Harness::scripted(script);
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.failure.as_deref(), Some(FAIL_EMPTY_REPLY));
    assert!(terminal.retry_permitted);
    assert_eq!(h.seam.call_count(), MAX_ATTEMPTS);
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1);
}

#[test]
fn an_empty_reply_that_answers_on_a_later_attempt_delivers() {
    // CVL-FR-19, CVL-FR-20, AGC-FR-21, CVL-FR-12 for the completion-outcome half: an empty reply is retried in
    // place, and a reply that carries prose ends the invocation normally.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::answer("")),
        Ok(ScriptedReply::answer("   ")),
        Ok(ScriptedReply::answer("said at last")),
    ]);
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), MAX_ATTEMPTS);
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 2);
    assert_eq!(threads[0].comments[1].body, "said at last");
}

#[test]
fn each_invocation_of_the_loop_gets_its_own_attempt_budget() {
    // CVL-FR-18, AGC-FR-18 (the per-invocation clause) / CVL-FR-19, CVL-FR-13.
    //
    // Every other retry test here confines its failures to a single logical
    // invocation, so all of them pass equally under a budget that counts
    // *failures* per turn rather than attempts per invocation. This is the one
    // that tells those two readings apart: two invocations, each spending two
    // failures before it answers, is four failures in a turn whose per-invocation
    // budget is three. Under the turn-scoped reading this turn fails on its
    // fourth physical call — by which point the tool has already been dispatched,
    // since invocation one succeeds on its third attempt and that attempt is the
    // reply asking for it. So it is the delivery that goes red, not the tool.
    let h = Harness::scripted(vec![
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Ok(ScriptedReply::answer("the answer")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(
        terminal.state,
        AgentTurnState::Delivered,
        "a spent invocation is not a spent turn",
    );
    assert_eq!(h.seam.call_count(), 6, "the seam was reached once per physical attempt");
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 2, "exactly one comment was appended");
    assert_eq!(threads[0].comments[1].body, "the answer");

    let records = records_where("turnId", &terminal.id);
    assert_eq!(
        records
            .iter()
            .filter(|r| r.message == "agent tool call completed")
            .count(),
        1,
        "the tool between the two invocations was dispatched exactly once",
    );

    // CVL-FR-13: the two counts are reported separately, because a turn that made
    // two calls and one that made two across six attempts are the same turn to
    // the loop and a very different one to the endpoint. This pins the counts and
    // not their independence: both invocations spend a full budget here, so 6 is
    // also 3 × 2, and a report that multiplied the logical count by the attempt
    // ceiling would satisfy it.
    let ended = find(&records, "agent turn ended");
    assert_eq!(field(&ended, "modelCalls"), "2", "two logical invocations");
    assert_eq!(field(&ended, "physicalCalls"), "6", "six physical attempts");

    // The restart is the whole assertion: `1, 2` and then `1, 2` again rather
    // than `1, 2, 3, 4`. A schedule read back from the log is what CVL-FR-28
    // makes this observable at all.
    let failed: Vec<(String, String)> = records
        .iter()
        .filter(|r| r.message == "model call failed")
        .map(|r| (field(r, "modelCall"), field(r, "attempt")))
        .collect();
    assert_eq!(
        failed,
        vec![
            ("1".to_string(), "1".to_string()),
            ("1".to_string(), "2".to_string()),
            ("2".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ],
        "the second invocation began at attempt one again",
    );
}

#[test]
fn a_credential_or_configuration_failure_is_never_repeated() {
    // CVL-FR-18, AGC-FR-31.
    for failure in [FAIL_REJECTED, FAIL_KEYCHAIN_UNAVAILABLE] {
        let h = Harness::new(vec![Err(failure); MAX_ATTEMPTS]);
        h.create_agent("arch", "");
        let (terminal, _) = run_one(&h, "arch");

        assert_eq!(terminal.failure.as_deref(), Some(failure));
        assert!(!terminal.retry_permitted, "{failure} offers no retry");
        assert_eq!(h.seam.call_count(), 1, "{failure} was attempted once");
        assert!(
            h.turns().recoverable_failures(None).is_empty(),
            "{failure} left no offer",
        );
    }

    // `context_unavailable` never reaches the seam at all: it is refused before
    // a call is made, so its non-recoverability is a property of the path rather
    // than of a retry decision.
    let h = Harness::new(vec![Ok("unreachable answer".into())]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    std::fs::remove_file(h.root().path().join("spec.md")).expect("remove the artifact");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert!(!terminal.retry_permitted);
    assert!(h.turns().recoverable_failures(None).is_empty());
}

#[test]
fn the_default_retry_schedule_is_the_one_the_spec_states() {
    // CVL-FR-28 / CVL-FR-19, CVL-FR-22.
    let policy = RetryPolicy::default();
    assert_eq!(policy.max_attempts, 3);
    assert_eq!(policy.first_base, Duration::from_millis(500));
    assert_eq!(policy.ceiling, Duration::from_secs(2));
    assert_eq!(policy.call_timeout, Duration::from_secs(300));

    // The midpoint fraction is the base exactly; the two ends are ±20% of it.
    assert_eq!(backoff_delay(&policy, 2, &FixedJitter(0.5)), Duration::from_millis(500));
    assert_eq!(backoff_delay(&policy, 3, &FixedJitter(0.5)), Duration::from_millis(1000));
    assert_eq!(backoff_delay(&policy, 2, &FixedJitter(0.0)), Duration::from_millis(400));
    assert_eq!(backoff_delay(&policy, 2, &FixedJitter(1.0)), Duration::from_millis(600));
    assert_eq!(backoff_delay(&policy, 3, &FixedJitter(0.0)), Duration::from_millis(800));
    assert_eq!(backoff_delay(&policy, 3, &FixedJitter(1.0)), Duration::from_millis(1200));

    // CVL-FR-19: never above two seconds, whatever the schedule says. A source
    // outside `[0, 1]` is clamped rather than trusted.
    let long = RetryPolicy {
        first_base: Duration::from_secs(30),
        ..policy
    };
    assert_eq!(backoff_delay(&long, 2, &FixedJitter(1.0)), BACKOFF_CEILING);
    assert_eq!(backoff_delay(&long, 3, &FixedJitter(9.0)), BACKOFF_CEILING);
    // Beyond the schedule there is no wait, because there is no further attempt.
    assert_eq!(backoff_delay(&policy, 4, &FixedJitter(0.5)), Duration::ZERO);
}

#[test]
fn the_schedule_is_defined_for_whatever_budget_the_project_holds() {
    // CVL-FR-19, CVL-FR-22, CVL-FR-PDXK / PSS-FR-WPKS: the retry budget is the
    // project's, so the schedule has to be defined past the two waits a default
    // budget takes. Each retry doubles the one before, and the ceiling is what
    // keeps that bounded however large the budget is.
    let wide = RetryPolicy {
        max_attempts: 10,
        ..RetryPolicy::default()
    };
    assert_eq!(backoff_delay(&wide, 2, &FixedJitter(0.5)), Duration::from_millis(500));
    assert_eq!(backoff_delay(&wide, 3, &FixedJitter(0.5)), Duration::from_millis(1000));
    assert_eq!(backoff_delay(&wide, 4, &FixedJitter(0.5)), Duration::from_millis(2000));
    // From here the doubling is past the ceiling, and the ceiling holds.
    for attempt in 5..=10 {
        assert_eq!(
            backoff_delay(&wide, attempt, &FixedJitter(1.0)),
            BACKOFF_CEILING,
            "attempt {attempt} waits no longer than the ceiling"
        );
    }
    // Past the budget there is no wait, because there is no further attempt.
    assert_eq!(backoff_delay(&wide, 11, &FixedJitter(0.5)), Duration::ZERO);

    // A budget of one is the original call and no retry at all.
    let single = RetryPolicy {
        max_attempts: 1,
        ..RetryPolicy::default()
    };
    assert_eq!(backoff_delay(&single, 2, &FixedJitter(0.5)), Duration::ZERO);
}

#[test]
fn the_chosen_backoff_is_the_one_the_log_reports() {
    // CVL-FR-19 (the log half) / CVL-FR-28, CVL-FR-19.
    let mut script: Vec<Result<String, &'static str>> = vec![Err(FAIL_UNREACHABLE); 2];
    script.push(Ok("answered".into()));
    // The shipped schedule really waited, so the whole-turn deadline has to be
    // comfortably beyond the 1.5 seconds those two waits take.
    let h = Harness::with_retries(script, RetryPolicy::default(), 0.5, Duration::from_secs(30));
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let planned: Vec<String> = records_where("turnId", &terminal.id)
        .iter()
        .filter(|r| r.message == "model call failed")
        .map(|r| field(r, "plannedBackoffMs"))
        .collect();
    assert_eq!(planned, vec!["500".to_string(), "1000".to_string()]);
}

#[test]
fn a_cancellation_during_a_backoff_wait_abandons_the_retry() {
    // AGC-FR-19 / CVL-FR-19, CVL-FR-25, AGC-FR-31.
    // Long enough that the cancellation lands inside the wait rather than racing
    // it, and far below the whole-turn deadline.
    let h = Harness::with_retries(
        vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS],
        RetryPolicy {
            first_base: Duration::from_millis(400),
            ..RetryPolicy::default()
        },
        0.5,
        Duration::from_secs(30),
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

    // Wait until the first attempt has failed and the turn is in its backoff.
    for _ in 0..500 {
        if h.seam.call_count() >= 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    // Asserted rather than assumed: a budget that ran out leaves a turn that has
    // not attempted anything yet, and cancelling *that* is a different test than
    // this one — one that would still pass.
    assert!(h.seam.call_count() >= 1, "the first attempt never began");
    let cancelled = cancel_impl(&h.app.handle().clone(), &h.turns(), &h.progress(), &turn.id)
        .expect("cancel");
    assert_eq!(cancelled.state, AgentTurnState::Cancelled);
    h.settle();

    assert_eq!(
        h.seam.call_count(),
        1,
        "the retry the wait was for was never made",
    );
    // AGC-FR-31: a cancelled turn is not a recoverable failure, so nothing is
    // offered — the author already said they were done with it.
    assert!(!cancelled.retry_permitted);
    assert!(h.turns().recoverable_failures(None).is_empty());
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1);
}

#[test]
fn the_whole_turn_deadline_expiring_in_a_backoff_is_not_recoverable() {
    // CVL-FR-16, CVL-FR-18, AGC-FR-31.
    let h = Harness::with_retries(
        vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS],
        RetryPolicy {
            // Longer than the harness's whole-turn deadline, so the wait is what
            // the deadline lands in.
            first_base: SHORT_TIMEOUT * 4,
            ceiling: SHORT_TIMEOUT * 8,
            ..RetryPolicy::default()
        },
        0.5,
        SHORT_TIMEOUT,
    );
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    // CVL-FR-18: the whole-turn deadline, whose budget is spent by definition.
    // It shares a spelling with the per-call one and not its recoverability.
    assert!(!terminal.retry_permitted);
    assert!(h.turns().recoverable_failures(None).is_empty());
    assert_eq!(h.seam.call_count(), 1, "no retry began after the deadline");
    // The record says which deadline it was, the value alone being ambiguous.
    let record = find(&records_where("turnId", &terminal.id), "agent turn ran out of time");
    assert_eq!(field(&record, "deadline"), "whole_turn");
}

#[test]
fn a_per_call_deadline_is_the_lesser_of_five_minutes_and_what_the_turn_has_left() {
    // CVL-FR-16, CVL-FR-18 / CVL-FR-17.
    //
    // Asserted on the timeout the seam was actually handed, which is the only
    // observable that distinguishes the two bounds without waiting out either.
    let h = Harness::new(vec![Ok("answered".into())]);
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let record = find(&records_where("turnId", &terminal.id), "calling model");
    let given: u64 = field(&record, "timeoutMs").parse().expect("a timeout");
    // The harness runs under a whole-turn deadline far below five minutes, so
    // the cap is what decided this call's deadline.
    assert!(
        given <= SHORT_TIMEOUT.as_millis() as u64,
        "the per-call deadline was capped by the turn's remaining time, got {given}ms",
    );
    assert!(
        given < PROVIDER_CALL_TIMEOUT.as_millis() as u64,
        "five minutes did not outlive the turn it belongs to",
    );

    // And with a turn deadline beyond five minutes, five minutes is the lesser.
    let patient = Harness::with_retries(
        vec![Ok("answered".into())],
        RetryPolicy::default(),
        0.5,
        PROVIDER_CALL_TIMEOUT * 2,
    );
    patient.create_agent("arch", "");
    let (patient_turn, _) = run_one(&patient, "arch");
    let record = find(&records_where("turnId", &patient_turn.id), "calling model");
    let given: u64 = field(&record, "timeoutMs").parse().expect("a timeout");
    assert_eq!(
        given,
        PROVIDER_CALL_TIMEOUT.as_millis() as u64,
        "a turn with more than five minutes left gives the call five minutes",
    );
}

#[test]
fn a_retry_repeats_the_call_and_nothing_else() {
    // CVL-FR-20, CVL-FR-09, CVL-FR-24.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Ok(ScriptedReply::answer("answered")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    // The tool ran once, however many times the call after it was repeated.
    let dispatched = records_where("turnId", &terminal.id)
        .iter()
        .filter(|r| r.message == "agent tool call completed")
        .count();
    assert_eq!(dispatched, 1, "no tool was dispatched a second time");

    // CVL-FR-20: the exchange each retry carried is byte-identical to the one
    // the attempt before it carried — the same turn making the same request.
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 4);
    assert_eq!(
        format!("{:?}", exchanges[1]),
        format!("{:?}", exchanges[2]),
        "the first retry carried the exchange exactly as it stood",
    );
    assert_eq!(
        format!("{:?}", exchanges[2]),
        format!("{:?}", exchanges[3]),
        "and so did the second",
    );

    assert_eq!(h.threads("spec.md")[0].comments.len(), 2, "one comment, not four");
    assert_eq!(h.agent_sessions(), 0, "one session, ended with the turn");
}

#[test]
fn the_recovery_registry_answers_for_a_conversation_and_carries_nothing_else() {
    // AGC-FR-31, AGC-FR-22, AGC-FR-26, CVL-FR-26.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS]);
    h.create_agent("arch", "");
    let (terminal, thread) = run_one(&h, "arch");
    let origin = ConversationOrigin::of(&thread);

    let entries = h.turns().recoverable_failures(Some(&origin));
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.id, terminal.id);
    assert_eq!(entry.agent_id, terminal.agent_id);
    assert_eq!(entry.nickname, "arch");
    assert_eq!(entry.origin, origin);
    assert_eq!(entry.trigger_comment_id, thread.comments[0].id);
    assert_eq!(entry.failure.as_deref(), Some(FAIL_UNREACHABLE));
    assert!(entry.retry_permitted);

    // AGC-FR-22: an entry is not outstanding under either form.
    assert!(h.turns().in_flight(None).is_empty());
    assert!(h.turns().in_flight(Some(&origin)).is_empty());

    // AGC-FR-26 / CVL-FR-26: the whole record, serialised, holds no request, no
    // exchange, no tool result, no key, and no answer — the shape is the turn
    // and nothing beside it.
    let json = serde_json::to_value(entry).expect("serialise");
    let mut keys: Vec<&str> = json
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut expected = vec![
        "id",
        "agentId",
        "nickname",
        "origin",
        "triggerCommentId",
        "state",
        "failure",
        "retryPermitted",
        "startedAt",
        "endedAt",
        // AGC-FR-34: carried, and empty — a turn in any terminal state holds no
        // active call, so the entry names no tool either.
        "activeToolCalls",
        // AGC-FR-37: carried on every payload a turn appears in.
        "imagesOmitted",
    ];
    expected.sort_unstable();
    assert_eq!(keys, expected, "the entry is the turn and nothing beside it");
    assert_eq!(
        json["activeToolCalls"],
        serde_json::json!([]),
        "a terminated turn holds no active tool call",
    );
    let text = json.to_string();
    // `activeToolCalls` is the entry's own key and carries no tool name, so the
    // guard against a future field named `tools` stands unchanged.
    for forbidden in ["instructions", "exchange", "apiKey", "\"tools\"", "input", "answer"] {
        assert!(!text.contains(forbidden), "{forbidden} reached the registry");
    }
}

#[test]
fn a_manual_retry_starts_a_fresh_turn_and_consumes_the_offer() {
    // AGC-FR-32, AGC-FR-13, AGC-FR-31.
    let mut script: Vec<Result<String, &'static str>> = vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS];
    script.push(Ok("the retried answer".into()));
    let h = Harness::new(script);
    h.create_agent("arch", "");
    let (failed, thread) = run_one(&h, "arch");
    assert!(failed.retry_permitted);

    let app = h.app.handle().clone();
    let retried = retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &failed.id).expect("retry");
    assert_ne!(retried.id, failed.id, "a new turn rather than the old one");
    assert_eq!(retried.state, AgentTurnState::Running);
    assert_eq!(retried.nickname, failed.nickname);
    assert_eq!(retried.agent_id, failed.agent_id);
    assert_eq!(retried.origin, failed.origin);
    assert_eq!(retried.trigger_comment_id, failed.trigger_comment_id);
    assert!(!retried.retry_permitted);

    // AGC-FR-31: registering the new turn consumed the offer.
    assert!(h.turns().recoverable_failures(None).is_empty());
    // AGC-FR-32: the failed turn is neither mutated nor resurrected.
    let still = h.turns().terminated(&failed.id).expect("the failed turn stands");
    assert_eq!(still.state, AgentTurnState::Failed);
    assert_eq!(still.failure.as_deref(), Some(FAIL_UNREACHABLE));

    let terminal = wait_for_terminal(&h, &retried.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let threads = h.threads("spec.md");
    assert_eq!(
        threads[0].comments.len(),
        2,
        "the human comment was not reposted; one answer was appended",
    );
    assert_eq!(threads[0].comments[0].id, thread.comments[0].id);
    assert_eq!(threads[0].comments[1].body, "the retried answer");

    // A second retry of the same turn finds nothing to take.
    assert_eq!(
        retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &failed.id).unwrap_err(),
        ERR_TURN_NOT_FOUND,
    );
}

#[test]
fn a_retry_of_anything_but_the_current_entry_is_refused() {
    // AGC-FR-32.
    let h = Harness::new(vec![Ok("delivered".into()), Err(FAIL_REJECTED)]);
    h.create_agent("arch", "");
    let app = h.app.handle().clone();

    // A turn that delivered.
    let (delivered, _) = run_one(&h, "arch");
    assert_eq!(
        retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &delivered.id).unwrap_err(),
        ERR_TURN_NOT_FOUND,
    );

    // A turn whose failure was not recoverable.
    let (rejected, _) = run_one(&h, "arch");
    assert_eq!(rejected.failure.as_deref(), Some(FAIL_REJECTED));
    assert_eq!(
        retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &rejected.id).unwrap_err(),
        ERR_TURN_NOT_FOUND,
    );

    // An id no turn ever carried.
    assert_eq!(
        retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, "turn-nonesuch").unwrap_err(),
        ERR_TURN_NOT_FOUND,
    );
}

#[test]
fn a_refused_retry_leaves_the_offer_standing() {
    // AGC-FR-32 (the locked half) / AGC-FR-32, CTA-FR-QDDG.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS]);
    h.create_agent("arch", "");
    let (failed, thread) = run_one(&h, "arch");
    assert!(failed.retry_permitted);

    crate::comments::set_lock_in(
        &h.root(),
        "spec.md",
        &thread.id,
        true,
        &human("ada"),
        "2024-01-01T00:00:00Z",
    )
    .expect("lock");

    let app = h.app.handle().clone();
    assert_eq!(
        retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &failed.id).unwrap_err(),
        FAIL_THREAD_LOCKED,
    );
    // The offer is exactly where it was, so the author can take it again once
    // the thread is unlocked rather than losing the only route back to it.
    let entries = h.turns().recoverable_failures(None);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, failed.id);
}

#[test]
fn a_later_recoverable_failure_replaces_the_one_before_it() {
    // AGC-FR-31, AGC-FR-32.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS * 2]);
    h.create_agent("arch", "");
    h.create_agent("sec", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);

    let first = h
        .dispatch("arch", origin.clone(), &thread.comments[0].id)
        .expect("arch");
    let first = wait_for_terminal(&h, &first.id);
    let second = h
        .dispatch("sec", origin.clone(), &thread.comments[0].id)
        .expect("sec");
    let second = wait_for_terminal(&h, &second.id);

    let entries = h.turns().recoverable_failures(Some(&origin));
    assert_eq!(entries.len(), 1, "one entry per conversation");
    assert_eq!(entries[0].id, second.id);
    assert_eq!(entries[0].nickname, "sec");

    // The displaced turn is no longer retryable, the card exposing Retry for the
    // most recently failed eligible turn alone.
    let app = h.app.handle().clone();
    assert_eq!(
        retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &first.id).unwrap_err(),
        ERR_TURN_NOT_FOUND,
    );
}

#[test]
fn entries_in_two_conversations_are_independent() {
    // AGC-FR-32 (the second half) / AGC-FR-31.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS * 2]);
    h.create_agent("arch", "");
    let one = h.seed_artifact_thread("one.md", "Some artifact source here.");
    let two = h.seed_artifact_thread("two.md", "Some artifact source here.");
    let origin_one = ConversationOrigin::of(&one);
    let origin_two = ConversationOrigin::of(&two);

    for (origin, thread) in [(&origin_one, &one), (&origin_two, &two)] {
        let turn = h
            .dispatch("arch", origin.clone(), &thread.comments[0].id)
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);
    }

    assert_eq!(h.turns().recoverable_failures(Some(&origin_one)).len(), 1);
    assert_eq!(h.turns().recoverable_failures(Some(&origin_two)).len(), 1);
    assert_eq!(
        h.turns().recoverable_failures(None).len(),
        2,
        "there is no project-wide cap on entries",
    );
}

#[test]
fn a_human_comment_retires_the_offer_and_an_agents_answer_does_not() {
    // AGC-FR-30 / AGC-FR-31.
    let mut script: Vec<Result<String, &'static str>> = vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS];
    script.push(Ok("another agent's answer".into()));
    let h = Harness::new(script);
    h.create_agent("arch", "");
    h.create_agent("sec", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);

    let failed = h
        .dispatch("arch", origin.clone(), &thread.comments[0].id)
        .expect("arch");
    wait_for_terminal(&h, &failed.id);
    assert_eq!(h.turns().recoverable_failures(None).len(), 1);

    // An agent-authored comment retires nothing: an agent answering elsewhere in
    // the thread does not mean the author has stopped wanting the answer that
    // failed. Driven through the real agent write path rather than asserted of
    // it, so the two paths are told apart by what they actually do.
    let answered = h
        .dispatch("sec", origin.clone(), &thread.comments[0].id)
        .expect("sec");
    let answered = wait_for_terminal(&h, &answered.id);
    assert_eq!(answered.state, AgentTurnState::Delivered);
    assert_eq!(h.threads("spec.md")[0].comments.len(), 2);
    assert_eq!(
        h.turns().recoverable_failures(None).len(),
        1,
        "an agent's answer left the offer standing",
    );

    // A later human comment retires it.
    retire_recoverable_for_human_comment(&h.app.handle().clone(), &thread.id);
    assert!(h.turns().recoverable_failures(None).is_empty());
}

#[test]
fn closing_a_project_and_losing_an_agent_discard_the_offers_they_own() {
    // AGC-FR-31 (the in-session half) / AGC-FR-31, AGC-FR-23.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS]);
    let agent = h.create_agent("arch", "");
    run_one(&h, "arch");
    assert_eq!(h.turns().recoverable_failures(None).len(), 1);

    // AGR-FR-11: an agent that is gone is offered nothing.
    cancel_turns_for_agent(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &agent.id,
        None,
    );
    assert!(h.turns().recoverable_failures(None).is_empty());

    // And a closing project holds no conversation for an offer to be made in.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS]);
    h.create_agent("arch", "");
    run_one(&h, "arch");
    assert_eq!(h.turns().recoverable_failures(None).len(), 1);
    cancel_all_turns(&h.app.handle().clone(), &h.turns(), &h.progress());
    assert!(h.turns().recoverable_failures(None).is_empty());
}

#[test]
fn every_failed_attempt_is_reported_with_its_correlation_and_no_payload() {
    // CVL-FR-28, CVL-FR-21, AGC-FR-26, CVL-FR-26.
    let h = failing_then_answering(2, FAIL_UNREACHABLE, "answered");
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    let records = records_where("turnId", &terminal.id);
    let attempts: Vec<&LogRecord> = records
        .iter()
        .filter(|r| r.message == "model call failed")
        .collect();
    assert_eq!(attempts.len(), 2, "one record per failed physical attempt");
    for (index, record) in attempts.iter().enumerate() {
        // CVL-FR-28: `ai` and `backend` together on every record a turn emits,
        // so both queries of CVL-FR-28, CVL-FR-21 return the attempts as well as the
        // terminal record they explain. `remote` besides, an attempt at reaching
        // a provider being exactly what that domain is for.
        assert!(record.domains.contains(&Domain::Ai));
        assert!(record.domains.contains(&Domain::Backend));
        assert!(record.domains.contains(&Domain::Remote));
        assert_eq!(field(record, "turnId"), terminal.id);
        assert_eq!(field(record, "agent"), "arch");
        assert_eq!(field(record, "agentId"), terminal.agent_id);
        assert_eq!(field(record, "provider"), "openrouter");
        assert_eq!(field(record, "model"), "m");
        assert_eq!(field(record, "attempt"), (index + 1).to_string());
        assert_eq!(field(record, "maxAttempts"), MAX_ATTEMPTS.to_string());
        assert_eq!(field(record, "failure"), FAIL_UNREACHABLE);
        assert!(!field(record, "failureClass").is_empty());
        assert_eq!(field(record, "retrying"), "true");
        assert!(!field(record, "plannedBackoffMs").is_empty());
    }

    // The terminal record carries the same correlation plus the turn's cost, and
    // reports the two call counts separately.
    let ended = find(&records, "agent turn ended");
    assert_eq!(field(&ended, "agent"), "arch");
    assert_eq!(field(&ended, "agentId"), terminal.agent_id);
    assert_eq!(field(&ended, "state"), "delivered");
    assert_eq!(field(&ended, "modelCalls"), "1", "one logical invocation");
    assert_eq!(field(&ended, "physicalCalls"), "3", "three physical attempts");
    assert!(!field(&ended, "elapsedMs").is_empty());

    // AGC-FR-26 / CVL-FR-26: nothing any of them carries is a payload.
    for record in &records {
        let text = serde_json::to_string(&record.fields).expect("fields");
        for forbidden in ["Some artifact source", "answered", "sk-", "Authorization", "@arch what"] {
            assert!(
                !text.contains(forbidden),
                "{forbidden:?} reached a record: {text}",
            );
        }
    }
}

#[test]
fn a_transport_error_is_normalized_before_it_is_reported() {
    // CVL-FR-28, AGC-FR-26 / CVL-FR-21.
    let cases = [
        ("failed to lookup address information for api.example", class::DNS, FAIL_UNREACHABLE),
        ("tcp connect error: connection refused", class::CONNECT, FAIL_UNREACHABLE),
        ("connection reset by peer", class::RESET, FAIL_UNREACHABLE),
        ("request timed out after 30s", class::PROVIDER_TIMEOUT, FAIL_TIMED_OUT),
        ("HTTP 401 Unauthorized", class::AUTH, FAIL_REJECTED),
        ("server returned 503 for the request", class::HTTP_STATUS, FAIL_UNREACHABLE),
        ("the sprocket flange disagreed", class::TRANSPORT_OTHER, FAIL_UNREACHABLE),
    ];
    for (message, expected_class, expected_failure) in cases {
        let classified = classify_provider_error(message);
        assert_eq!(classified.class, expected_class, "class of {message:?}");
        assert_eq!(classified.failure, expected_failure, "failure of {message:?}");
        // CVL-FR-21: a safe code and a redacted category, never the payload.
        // The classes are a closed vocabulary of this module's own, so nothing
        // a provider wrote can reach a record through one.
        assert!(classified.class.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        assert!(
            [
                class::DNS,
                class::CONNECT,
                class::RESET,
                class::HTTP_STATUS,
                class::PROVIDER_TIMEOUT,
                class::EMPTY_REPLY,
                class::TRANSPORT_OTHER,
                class::AUTH,
            ]
            .contains(&classified.class),
            "{message:?} produced a class outside the vocabulary",
        );
    }
    // The status a provider returned is carried; nothing else of the message is.
    assert_eq!(
        classify_provider_error("server returned 503 for the request").status,
        Some(503),
    );
    assert_eq!(classify_provider_error("connection reset by peer").status, None);
    // An empty reply is the completion outcome rather than a transport one, and
    // is classified without a message at all.
    assert_eq!(
        CallFailure::from(FAIL_EMPTY_REPLY).class,
        class::EMPTY_REPLY,
    );
}

#[test]
fn nothing_an_author_can_reach_varies_the_retry_policy() {
    // CVL-FR-22.
    //
    // The commands are the surface an author reaches, and none of them names a
    // delay, an attempt count, or a deadline. Asserted against the registered
    // command list rather than against this module's own functions, because that
    // list is what the frontend — and so the author — can actually invoke.
    // Commands ask for work to happen again and say nothing about how:
    // `retry_agent_turn` repeats one turn, `retry_work_stream_update`
    // (WKS-FR-DPNM) runs one update again, and
    // `retry_draft_publication` (per
    // `../specifications/core/GHP-github-publication.md` GHP-FR-OWLB) continues
    // one publication attempt, and `retry_github_claim` (per
    // `../specifications/core/GPP-github-polling.md` GPP-FR-TOED) completes
    // one pending claim's local steps. None carries a delay, a count or a
    // deadline.
    const ASKS_AGAIN: [&str; 4] = [
        "retry_agent_turn",
        "retry_work_stream_update",
        "retry_draft_publication",
        "retry_github_claim",
    ];
    // One command spells a forbidden word while meaning something else:
    // `cancel_draft_publication_attempt` abandons the persisted publication
    // **attempt record** of `../specifications/core/GHP-github-publication.md`
    // GHP-FR-NAXT, which is a durable object rather than a try count.
    const NOT_A_POLICY: [&str; 1] = ["cancel_draft_publication_attempt"];
    // CVL-FR-22 / CVL-FR-16: the whole-turn timeout is the provider's or the
    // project's, so one command sets it (`AAP-ai-api-integrations.md`
    // AAP-FR-FGNK). It is the turn's bound and none of the policy's waits.
    const SETS_TURN_BOUND: [&str; 1] = ["set_ai_api_turn_timeout"];
    for command in crate::COMMAND_NAMES {
        for forbidden in ["retry", "backoff", "attempt", "jitter", "timeout"] {
            assert!(
                !command.contains(forbidden)
                    || ASKS_AGAIN.contains(command)
                    || NOT_A_POLICY.contains(command)
                    || (forbidden == "timeout" && SETS_TURN_BOUND.contains(command)),
                "{command} looks like a retry-policy control",
            );
        }
    }
    // And `retry_agent_turn` itself takes a turn id and nothing else: it asks
    // for the turn to repeat, never for how to repeat it.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS]);
    h.create_agent("arch", "");
    let (failed, _) = run_one(&h, "arch");
    let app = h.app.handle().clone();
    // The whole of its input.
    let _: Result<AgentTurn, String> = retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &failed.id);

    // An agent's stored definition carries no retry field either.
    let agents = agents::list_agents_impl(&h.store()).expect("agents");
    let stored = serde_json::to_value(agents.first().expect("an agent")).expect("serialise");
    let text = stored.to_string();
    for forbidden in ["retry", "backoff", "attempt", "jitter"] {
        assert!(!text.contains(forbidden), "an agent names {forbidden}");
    }
}
