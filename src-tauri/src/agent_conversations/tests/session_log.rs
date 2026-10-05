//! What a turn writes to the session log, and what it must never write
//! there.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// Session logging (LGC-logging.md, AGC-FR-26)
// ---------------------------------------------------------------------------
//
// A turn runs on a thread of its own and leaves nothing behind but the comment
// it appended, so the session log is the only account of what it did. These
// assert that the account exists, that it is attributable, and — the part that
// cannot be recovered once it is wrong — that it carries neither a key nor a
// word of the conversation.
//
// The buffer is process-wide and every test in this file emits into it, so a
// turn's records are located by the two ids that are unique for the run: the
// `discussionId` `crate::notes::new_note_id` mints, and the `turnId` `NEXT_TURN`
// hands out. Neither restarts per test, which is what lets these run alongside
// the rest of the file rather than against a buffer of their own.

use crate::logging::{self, LogLevel, LogRecord};








#[test]
fn a_delivered_turn_reports_every_stage_it_passed_through() {
    // Dispatch, context, endpoint, call, answer, end — the sequence a reader
    // needs to tell a model that was never called from one that answered with
    // nothing.
    let h = Harness::new(vec![Ok("An answer.".into())]);
    h.create_agent("scribe", "Argue about structure.");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");

    let turn = h
        .dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    h.settle();
    wait_for_terminal(&h, &turn.id);

    let by_thread = records_where("discussionId", &thread.id);
    let dispatched = find(&by_thread, "agent turn dispatched");
    assert_eq!(require(&dispatched, "agent"), "scribe");
    assert_eq!(require(&dispatched, "provider"), "openrouter");
    assert_eq!(require(&dispatched, "model"), "m");
    assert_eq!(require(&dispatched, "reasoning"), "effort:high");
    assert_eq!(require(&dispatched, "originKind"), "artifact_comment");
    assert_eq!(dispatched.domains, vec![logging::Domain::Ai]);

    let ended = find(&by_thread, "agent turn ended");
    assert_eq!(require(&ended, "state"), "delivered");
    assert_eq!(require(&ended, "failure"), "none");
    assert_eq!(ended.level, LogLevel::Info, "a delivery is not an error");

    let by_turn = records_where("turnId", &turn.id);
    let context = find(&by_turn, "agent turn context assembled");
    assert_eq!(require(&context, "sections"), "3");
    assert_eq!(require(&context, "truncated"), "false");
    assert!(
        require(&context, "tags").contains(TAG_CURRENT_COMMENT),
        "the tags say which sections were assembled: {}",
        require(&context, "tags")
    );

    let resolved = find(&by_turn, "ai endpoint resolved");
    assert_eq!(require(&resolved, "baseUrl"), "https://openrouter.example/api/v1");
    assert_eq!(require(&resolved, "keyPresent"), "true", "presence, never the key");

    let calling = find(&by_turn, "calling model");
    assert_eq!(
        calling.domains,
        vec![logging::Domain::Ai, logging::Domain::Remote],
        "a call made on the author's behalf is about both",
    );
    assert!(
        require(&calling, "promptChars").parse::<u64>().unwrap_or(0) > 0,
        "the size of the prompt, so a request that carried nothing is visible",
    );

    let answered = find(&by_turn, "model answered");
    assert_eq!(require(&answered, "replyChars"), "10", "\"An answer.\" is ten characters");
    assert!(
        answered.fields.contains_key("durationMs"),
        "how long the model took is the first thing anyone asks",
    );
    assert_eq!(require(&answered, "agent"), "scribe", "the middle of a turn is attributable too");

    // The stages are in the order they happened. `sequence` is append order
    // (LGC-FR-05), so a panel read top to bottom is a turn read start to finish.
    let sequence_of = |message: &str| find(&by_turn, message).sequence;
    let ordered = [
        dispatched.sequence,
        sequence_of("agent turn context assembled"),
        sequence_of("ai endpoint resolved"),
        sequence_of("calling model"),
        sequence_of("model answered"),
        ended.sequence,
    ];
    assert!(
        ordered.windows(2).all(|w| w[0] < w[1]),
        "the stages are out of order: {ordered:?}",
    );

    // AGC-FR-21 has one terminal event per turn, and a reader counting outcomes
    // needs one terminal record to match. Two emit sites can produce it —
    // `finish` and `cancel_impl` — and only one of them may ever win.
    assert_eq!(
        by_turn.iter().filter(|r| r.message == "agent turn ended").count(),
        1,
        "exactly one terminal record per turn",
    );

    // LGC-FR-04: every field is flat, so the panel renders each as one row.
    for record in &by_turn {
        for (key, value) in &record.fields {
            assert!(
                !value.is_object() && !value.is_array(),
                "{:?} carries a structured {key}: {value}",
                record.message,
            );
        }
    }
}

#[test]
fn no_record_a_turn_produces_carries_a_key_or_a_word_of_the_conversation() {
    // AGC-FR-26, extended to what a conversation carries. The logging facility
    // redacts nothing (LGC-FR-16), so this is enforceable at the emit site
    // alone — and a record travels into the panel, onto the clipboard, and into
    // whatever file the author attaches to a bug report.
    let h = Harness::new(vec![Ok("The model's own words, verbatim.".into())]);
    h.create_agent("scribe", "Instructions the author configured.");
    let thread = h.seed_artifact_thread(
        "spec.md",
        "Confidential prose that belongs in the artifact and nowhere else.",
    );

    let turn = h
        .dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    h.settle();
    wait_for_terminal(&h, &turn.id);

    // The precondition without which every assertion below is vacuous: this
    // turn's records are actually in the buffer. `crate::project::close_project`
    // clears it (LGC-FR-15) and other tests in this binary reach that path, so
    // an emptied buffer would otherwise pass this test while proving nothing.
    let snapshot = all_records();
    let mine: Vec<&LogRecord> = snapshot
        .iter()
        .filter(|r| field(r, "turnId") == turn.id)
        .collect();
    assert!(
        mine.len() >= 4,
        "the turn's records are missing; nothing below would be evidence: {:?}",
        mine.iter().map(|r| r.message.as_str()).collect::<Vec<_>>(),
    );

    // The same snapshot the precondition was taken from, so nothing can be
    // appended or cleared between proving the records exist and reading them.
    let text = serde_json::to_string(&snapshot).expect("records serialise");
    // The key `FixedSecrets` hands every resolution.
    assert!(
        // A substring of the key, as AGC-FR-26 puts it — a record carrying half
        // a credential has disclosed half a credential.
        !text.contains("sk-secret"),
        "a key reached the session log",
    );
    for content in [
        // The material under discussion, the comment that addressed the agent,
        // the agent's own instructions, and the reply it produced.
        "Confidential prose",
        "what do you think?",
        "Instructions the author configured",
        "The model's own words",
    ] {
        assert!(
            !text.contains(content),
            "the log carries conversation content: {content:?}",
        );
    }
}

#[test]
fn a_failed_call_records_the_typed_failure_at_error() {
    // AGC-FR-15: the failure vocabulary is what tells an author whether to fix a
    // host, a credential, or nothing at all — so it is a field rather than
    // prose, and the record is at the level that says the turn produced nothing.
    let h = Harness::new(vec![Err(FAIL_REJECTED)]);
    h.create_agent("scribe", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");

    let turn = h
        .dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    h.settle();
    wait_for_terminal(&h, &turn.id);

    let failed = find(&records_where("turnId", &turn.id), "model call failed");
    assert_eq!(require(&failed, "failure"), FAIL_REJECTED);
    assert_eq!(failed.level, LogLevel::Error);
    assert!(failed.fields.contains_key("durationMs"));

    let ended = find(&records_where("discussionId", &thread.id), "agent turn ended");
    assert_eq!(require(&ended, "state"), "failed");
    assert_eq!(require(&ended, "failure"), FAIL_REJECTED);
    assert_eq!(ended.level, LogLevel::Error);
}

#[test]
fn a_dispatch_refused_before_registration_is_still_recorded() {
    // AGC-FR-02: a refusal registers no turn, so it emits no state change
    // either — without this record an `@mention` that went nowhere leaves no
    // trace at all.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");

    let refused = h.dispatch(
        "nobody",
        ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    );
    assert_eq!(refused.unwrap_err(), FAIL_AGENT_NOT_FOUND);

    let record = find(&records_where("discussionId", &thread.id), "agent turn refused");
    assert_eq!(require(&record, "agent"), "nobody");
    assert_eq!(require(&record, "failure"), FAIL_AGENT_NOT_FOUND);
    assert_eq!(require(&record, "originKind"), "artifact_comment");
    assert_eq!(record.level, LogLevel::Warn);

    // The other two refusals knowable without a call (AGC-FR-02), each naming
    // the correction it calls for: unlock the conversation, or restore the
    // provider the agent was configured against.
    h.create_agent("scribe", "");
    crate::comments::set_lock_in(
        &h.root(),
        "spec.md",
        &thread.id,
        true,
        &human("ada"),
        "2026-01-01T00:00:03Z",
    )
    .expect("lock");
    assert_eq!(
        h.dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .unwrap_err(),
        FAIL_THREAD_LOCKED,
    );
    let locked = find(&records_where("discussionId", &thread.id), "agent turn refused");
    assert_eq!(require(&locked, "failure"), FAIL_THREAD_LOCKED);

    crate::comments::set_lock_in(
        &h.root(),
        "spec.md",
        &thread.id,
        false,
        &human("ada"),
        "2026-01-01T00:00:04Z",
    )
    .expect("unlock");
    h.store().save_ai_api_registry(vec![], None).expect("clear");
    assert_eq!(
        h.dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .unwrap_err(),
        FAIL_AGENT_UNAVAILABLE,
    );
    let unavailable = find(&records_where("discussionId", &thread.id), "agent turn refused");
    assert_eq!(require(&unavailable, "failure"), FAIL_AGENT_UNAVAILABLE);
    assert_eq!(require(&unavailable, "agent"), "scribe");
}

#[test]
fn a_cancelled_turn_ends_at_info_rather_than_error() {
    // The distinction the level is carrying: a cancellation is a decision the
    // author made, so a panel filtered to ERROR shows the failures and not the
    // turns somebody changed their mind about. Cancellation terminates through
    // `cancel_impl` rather than `finish`, which is a second emit site for the
    // same record and the reason this is asserted separately.
    let h = Harness::with(vec![Ok("too late".into())], Duration::from_millis(400), 4);
    h.create_agent("scribe", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");

    // Cancel while the model call is actually in flight, rather than while the
    // worker is still resolving: that is the race AGC-FR-20 is about, and it is
    // the only path on which the worker reaches its *own* exit — the second of
    // the two sites that can emit a terminal record. The seam records a request
    // as it enters, so this waits on the call rather than on a clock.
    for _ in 0..1000 {
        if h.seam.requests_len() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(h.seam.requests().len(), 1, "the model call never started");
    cancel_impl(&h.app.handle().clone(), &h.turns(), &h.progress(), &turn.id)
        .expect("cancel");

    let ended = find(&records_where("turnId", &turn.id), "agent turn ended");
    assert_eq!(require(&ended, "state"), "cancelled");
    assert_eq!(require(&ended, "failure"), "none");
    assert_eq!(ended.level, LogLevel::Info, "a cancellation is not a fault");

    // `settle()` is not enough to assert the count below: `terminate` removes
    // the turn from the in-flight set, so it returns while the worker is still
    // inside the model call. The second emit site — `finish`, on the worker's
    // own exit — has not run yet, and counting here would pass against a turn
    // that goes on to emit a second terminal record a moment later. So wait for
    // the worker to reach its own end, which it announces by discarding the
    // answer it can no longer deliver (AGC-FR-20).
    let discarded = wait_for_record(&turn.id, "agent answer discarded: the turn was cancelled");
    assert!(
        discarded.sequence > ended.sequence,
        "the worker exits after the cancellation, which is what makes the count below a test",
    );
    h.settle();

    assert_eq!(
        records_where("turnId", &turn.id)
            .iter()
            .filter(|r| r.message == "agent turn ended")
            .count(),
        1,
        "the run's own exit must not emit a second terminal record",
    );
}

#[test]
fn an_answer_that_cannot_be_appended_says_so_at_error() {
    // The most expensive failure there is: the model answered, the tokens were
    // spent, and the conversation gained nothing (AGC-FR-18). Without this
    // record the turn reads as though it never reached a model at all.
    let h = Harness::with(vec![Ok("too late".into())], Duration::from_millis(500), 4);
    h.create_agent("scribe", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    // Locked while the model is still thinking, so the answer arrives at a
    // thread that will not take it (AGC-FR-19).
    crate::comments::set_lock_in(
        &h.root(),
        "spec.md",
        &thread.id,
        true,
        &human("ada"),
        "2026-01-01T00:00:05Z",
    )
    .expect("lock");
    wait_for_terminal(&h, &turn.id);

    let by_turn = records_where("turnId", &turn.id);
    // The model was reached and did answer — that is what makes the failure
    // expensive, and it is why both records have to be there.
    find(&by_turn, "model answered");
    let discarded = find(&by_turn, "agent answer could not be appended");
    assert_eq!(require(&discarded, "failure"), FAIL_THREAD_LOCKED);
    assert_eq!(require(&discarded, "originKind"), "artifact_comment");
    assert_eq!(discarded.level, LogLevel::Error);
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1, "and nothing was appended");
}

#[test]
fn an_endpoint_that_will_not_resolve_names_what_the_author_has_to_correct() {
    // The turn's own vocabulary collapses several refusals into one value, so
    // the record carries `crate::ai_api`'s typed reason beside it: a provider
    // that was never verified, a model it no longer serves, and a keychain that
    // will not open are three different corrections and one `failure`.
    //
    // Driven through the real resolution path with a keychain that refuses,
    // rather than by scripting the seam — the failure this covers happens
    // before a model is reached at all.
    let h = Harness::with_locked_keychain(vec![Ok("never sent".into())]);
    h.create_agent("scribe", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "scribe",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("a locked keychain does not stop a dispatch (AAP-FR-33)");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_KEYCHAIN_UNAVAILABLE));

    let by_turn = records_where("turnId", &turn.id);
    let record = find(&by_turn, "ai endpoint did not resolve");
    assert_eq!(require(&record, "failure"), FAIL_KEYCHAIN_UNAVAILABLE);
    assert_eq!(
        require(&record, "reason"),
        crate::ai_api::ERR_KEYCHAIN_UNAVAILABLE,
        "the reason is one of the resolver's typed refusals, never a payload",
    );
    assert_eq!(require(&record, "provider"), "openrouter");
    assert_eq!(record.level, LogLevel::Warn);
    // No call was made, so nothing claims one was.
    assert!(
        !by_turn.iter().any(|r| r.message == "calling model"),
        "a turn that never reached a model must not report calling one",
    );
    assert!(h.seam.requests().is_empty());
}
