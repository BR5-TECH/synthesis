//! Dispatching a turn: what it answers with, which endpoint carries it,
//! how it fails, how it is delivered, and what it reports while it runs.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// AGC-FR-01, AGC-FR-02, AGC-FR-21 / AGC-FR-03, AGC-FR-22 / AGC-FR-04, AGC-FR-18: dispatch, concurrency, independence
// ---------------------------------------------------------------------------

#[test]
fn a_dispatch_returns_running_at_once_and_publishes_its_registration() {
    // AGC-FR-01, AGC-FR-02, AGC-FR-21.
    let h = Harness::with(vec![Ok("An answer.".into())], Duration::from_millis(300), 4);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let trigger = thread.comments[0].id.clone();

    let (tx, rx) = mpsc::channel();
    h.app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
        let _ = tx.send(event.payload().to_string());
    });

    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &trigger,
        )
        .expect("dispatch");

    // "dispatch must not wait for the model" is carried by these two lines: the
    // turn is still running and has not ended, so dispatch returned while the
    // work was in flight.
    //
    // There is deliberately no wall-clock assertion beside them. One stood here
    // at 2s, and it could not fail for the reason it gave — the seam's delay is
    // 300ms, so a dispatch that *did* wait for the model still came in far
    // under it. The only bound that would have discriminated is one below 300ms,
    // which on a machine running 2000-odd tests beside this one measures the
    // filesystem rather than the code. A bound that cannot fail is not evidence,
    // and widening it to keep it quiet would only have documented that at
    // length.
    assert_eq!(turn.state, AgentTurnState::Running);
    assert!(turn.ended_at.is_none());
    let first = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("registration event");
    assert!(first.contains("\"running\""));
    h.settle();
}

#[test]
fn any_number_of_turns_may_be_in_flight_and_none_is_refused() {
    // AGC-FR-03, AGC-FR-22.
    let h = Harness::with(
        vec![Ok("one".into()), Ok("two".into()), Ok("three".into())],
        Duration::from_millis(400),
        4,
    );
    h.create_agent("arch", "");
    h.create_agent("sec", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let trigger = thread.comments[0].id.clone();

    let a = h.dispatch("arch", origin.clone(), &trigger).expect("first");
    let b = h.dispatch("arch", origin.clone(), &trigger).expect("second");
    let c = h.dispatch("sec", origin.clone(), &trigger).expect("third");

    let live = h.turns().in_flight(Some(&origin));
    assert_eq!(live.len(), 3, "no dispatch was refused for another running");
    let ids: Vec<&str> = live.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec![c.id.as_str(), b.id.as_str(), a.id.as_str()]);
    h.settle();
}

#[test]
fn one_agents_failure_never_suppresses_anothers_answer() {
    // AGC-FR-04, AGC-FR-18.
    // CVL-FR-19: `arch`'s call is recoverable, so it is made three times before
    // the turn is failed for it. The script is a queue of *physical* attempts,
    // so `sec`'s answer sits behind all three of them.
    let h = Harness::new(vec![
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Err(FAIL_UNREACHABLE),
        Ok("sec's answer".into()),
    ]);
    h.create_agent("arch", "");
    h.create_agent("sec", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let trigger = thread.comments[0].id.clone();

    h.dispatch("arch", origin.clone(), &trigger).expect("arch");
    // Serialised so the scripted replies land on the agent this test means.
    h.settle();
    h.dispatch("sec", origin, &trigger).expect("sec");
    h.settle();

    let threads = h.threads("spec.md");
    let comments = &threads[0].comments;
    assert_eq!(comments.len(), 2, "exactly one agent appended");
    assert_eq!(comments[1].body, "sec's answer");
    match &comments[1].author {
        Participant::Agent { handle, .. } => assert_eq!(handle, "sec"),
        other => panic!("expected an agent author, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// AGC-FR-14, AAP-FR-34 / CVL-FR-10: which endpoint carries the call
// ---------------------------------------------------------------------------

#[test]
fn the_call_carries_the_agents_own_model_and_reasoning_not_the_providers() {
    // AGC-FR-14, per AAP-FR-34. The provider's record selects
    // `provider-default-model` at `low`; the agent names `m` at `high`.
    let h = Harness::new(vec![Ok("ok".into())]);
    h.create_agent("arch", "");
    // The project overrides to Anthropic, which must not redirect the agent.
    h.store()
        .save_ai_api_override(PROJECT_KEY, "anthropic")
        .expect("override");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    h.dispatch(
        "arch",
        ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    )
    .expect("dispatch");
    h.settle();

    let (_, endpoint) = h.seam.requests().into_iter().next().expect("one call");
    assert_eq!(endpoint.provider, "openrouter");
    assert_eq!(endpoint.model_id.as_deref(), Some("m"));
    assert_eq!(
        endpoint.reasoning,
        Some(ReasoningChoice::Effort {
            effort: "high".into()
        }),
    );
}

#[test]
fn the_assembled_request_is_the_same_shape_whichever_client_would_carry_it() {
    // CVL-FR-10, CVL-FR-HBNW, CVL-FR-KXTQ. Which client carries the call is
    // `RigCompletion::complete`'s match on the provider; asserted here is the
    // claim that makes that match safe — the request `rig` assembles is
    // identical whichever provider it was assembled for.
    //
    // CVL-FR-37 / CVL-FR-01, CVL-FR-39: driven over both stable heads, because
    // naming one is a fact about how a call is **carried** and must reach
    // nothing the request presents. A build where it did would show up here as
    // two shapes rather than one.
    for stable_head_sections in [0usize, 1] {
    let request = AgentRequest {
        instructions: compile_prompt(OriginKind::ArtifactComment, "Argue about structure.", ""),
        input: vec![
            InputSection {
                tag: TAG_ARTIFACT.into(),
                attributes: vec![("path".into(), "spec.md".into())],
                body: "Body.".into(),
                truncated: false,
                parts: Vec::new(),
            },
            InputSection {
                tag: TAG_CURRENT_COMMENT.into(),
                attributes: Vec::new(),
                body: "What do you think?".into(),
                truncated: false,
                parts: Vec::new(),
            },
        ],
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections,
    };
    let shapes: Vec<_> = ["openrouter", "anthropic", "openai", "custom"]
        .iter()
        .map(|provider| {
            build_completion_request(
                &request,
                &opening_exchange(&request, false),
                &AiApiCall {
                    turn_timeout_ms: None,
                    provider: (*provider).into(),
                    base_url: "https://example/v1".into(),
                    api_key: Some("k".into()),
                    model_id: Some("m".into()),
                    reasoning: Some(ReasoningChoice::Effort {
                        effort: "high".into(),
                    }),
                    accepts_image_input: false,
                    model_mode: None,
                },
            )
        })
        .collect();
    // The prompt occupies the instruction position and the input the single user
    // message, for every provider alike (CVL-FR-10).
    for (provider, built) in ["openrouter", "anthropic", "openai", "custom"].iter().zip(&shapes) {
        assert_eq!(built.preamble.as_deref(), Some(request.instructions.as_str()));
        assert_eq!(built.model.as_deref(), Some("m"));
        assert_eq!(built.chat_history.len(), 1, "one user message, never more");
        assert_eq!(built.chat_history.len(), shapes[0].chat_history.len());
        assert!(built.tools.is_empty(), "an agent here answers, never acts");
        // CVL-FR-HBNW: the reasoning object is not a parameter of the Anthropic
        // Messages API, so only that provider receives none.
        let expected = (*provider != "anthropic")
            .then(|| serde_json::json!({ "reasoning": { "effort": "high" } }));
        assert_eq!(built.additional_params, expected, "{provider}");
        // CVL-FR-KXTQ: the Anthropic limit is set on that provider's model, so
        // the shared request carries no limit for any provider.
        assert_eq!(built.max_tokens, None, "{provider}");
    }
    // CVL-FR-01, CVL-FR-37, CVL-FR-39: and the whole assembled request is the same one whichever head
    // was named, so the split reaches nothing the request presents.
    let text: Vec<String> = shapes
        .iter()
        .map(|built| render_input(&request) + &format!("{:?}", built.chat_history))
        .collect();
    assert!(text.windows(2).all(|pair| pair[0] == pair[1]));
    }
}

// ---------------------------------------------------------------------------
// AGC-FR-15, AGC-FR-18, CVL-FR-11, CVL-FR-18, CVL-FR-19 / AGC-FR-02, AGC-FR-19: the failure vocabulary
// ---------------------------------------------------------------------------

#[test]
fn every_scripted_failure_terminates_its_turn_and_appends_nothing() {
    // AGC-FR-15, AGC-FR-18, CVL-FR-11, CVL-FR-18, CVL-FR-19.
    for failure in [
        FAIL_UNREACHABLE,
        FAIL_REJECTED,
        FAIL_KEYCHAIN_UNAVAILABLE,
        FAIL_EMPTY_REPLY,
        FAIL_TIMED_OUT,
    ] {
        // Scripted for the whole budget whether or not it will be spent: a
        // non-recoverable failure leaves the rest of the queue untouched, which
        // is exactly what the attempt count below asserts.
        let h = Harness::new(vec![Err(failure); MAX_ATTEMPTS]);
        h.create_agent("arch", "");
        let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
        let turn = h
            .dispatch(
                "arch",
                ConversationOrigin::of(&thread),
                &thread.comments[0].id,
            )
            .expect("dispatch");
        let terminal = wait_for_terminal(&h, &turn.id);
        assert_eq!(terminal.state, AgentTurnState::Failed);
        assert_eq!(terminal.failure.as_deref(), Some(failure));
        // CVL-FR-18: the three recoverable failures are repeated and the rest
        // are not, because calling again changes nothing about a refused
        // credential or a keychain that would not open.
        let recoverable = is_recoverable(failure);
        assert_eq!(
            h.seam.call_count(),
            if recoverable { MAX_ATTEMPTS } else { 1 },
            "attempts made for {failure}",
        );
        // AGC-FR-31: and only a recoverable one leaves an offer to ask again.
        assert_eq!(terminal.retry_permitted, recoverable, "retry offered for {failure}");
        assert_eq!(
            h.turns().recoverable_failures(None).len(),
            usize::from(recoverable),
            "registry entry for {failure}",
        );
        assert_eq!(
            h.threads("spec.md")[0].comments.len(),
            1,
            "the thread gained no line for {failure}",
        );
    }
}

#[test]
fn an_unresolvable_nickname_an_unavailable_agent_and_a_locked_thread_refuse_before_registering() {
    // AGC-FR-02, AGC-FR-15, AGC-FR-19.
    let h = Harness::new(vec![Ok("ok".into())]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let trigger = thread.comments[0].id.clone();

    assert_eq!(
        h.dispatch("nobody", origin.clone(), &trigger).unwrap_err(),
        FAIL_AGENT_NOT_FOUND,
    );
    assert!(
        h.turns().in_flight(None).is_empty(),
        "nothing was registered",
    );

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
        h.dispatch("arch", origin.clone(), &trigger).unwrap_err(),
        FAIL_THREAD_LOCKED,
    );
    crate::comments::set_lock_in(
        &h.root(),
        "spec.md",
        &thread.id,
        false,
        &human("ada"),
        "2026-01-01T00:00:04Z",
    )
    .expect("unlock");

    // An agent whose provider has been cleared.
    h.store().save_ai_api_registry(vec![], None).expect("clear");
    assert_eq!(
        h.dispatch("arch", origin, &trigger).unwrap_err(),
        FAIL_AGENT_UNAVAILABLE,
    );
}

#[test]
fn a_thread_locked_while_the_turn_was_in_flight_fails_the_turn_and_appends_nothing() {
    // AGC-FR-02, AGC-FR-15's third clause / AGC-FR-19.
    let h = Harness::with(vec![Ok("too late".into())], Duration::from_millis(250), 4);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    crate::comments::set_lock_in(
        &h.root(),
        "spec.md",
        &thread.id,
        true,
        &human("ada"),
        "2026-01-01T00:00:05Z",
    )
    .expect("lock");

    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_THREAD_LOCKED));
    // CVL-FR-18: a conversation that takes no further contribution is not one
    // calling again would change, so no offer to retry is left behind.
    assert!(!terminal.retry_permitted);
    assert!(h.turns().recoverable_failures(None).is_empty());
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1);
}

// ---------------------------------------------------------------------------
// AGC-FR-16, CMS-FR-41, CMS-FR-65: delivery
// ---------------------------------------------------------------------------

#[test]
fn a_delivered_turn_appends_an_ordinary_comment_authored_by_the_agent() {
    // CMS-FR-65 / AGC-FR-16, per CMS-FR-41.
    let h = Harness::new(vec![Ok("Two specs, I think.".into())]);
    let agent = h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert!(terminal.failure.is_none());

    let threads = h.threads("spec.md");
    let comments = &threads[0].comments;
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[1].body, "Two specs, I think.");
    match &comments[1].author {
        Participant::Agent {
            agent_id,
            handle,
            model,
            ..
        } => {
            assert_eq!(agent_id, &agent.id);
            assert_eq!(handle, "arch");
            assert_eq!(model.as_deref(), Some("m"));
        }
        other => panic!("expected an agent participant, got {other:?}"),
    }
}

#[test]
fn a_delivered_comment_carries_the_title_the_agent_answered_under() {
    // CVL-FR-04 / AGC-FR-16, per CMS-FR-65. The snapshot is taken at dispatch
    // and never resolved again, which is what makes an old comment readable as
    // the conversation it was rather than as the registry it outlived.
    let h = Harness::new(vec![
        Ok("As a developer, two.".into()),
        Ok("As an architect, still two.".into()),
        Ok("Untitled now.".into()),
    ]);
    let agent = h.create_titled_agent("arch", "", "Developer");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);

    let answer = |h: &Harness| {
        let turn = h
            .dispatch("arch", origin.clone(), &thread.comments[0].id)
            .expect("dispatch");
        assert_eq!(wait_for_terminal(h, &turn.id).state, AgentTurnState::Delivered);
    };
    let titles = |h: &Harness| -> Vec<Option<String>> {
        h.threads("spec.md")[0]
            .comments
            .iter()
            .skip(1)
            .map(|c| match &c.author {
                Participant::Agent { title, .. } => title.clone(),
                other => panic!("expected an agent participant, got {other:?}"),
            })
            .collect()
    };

    answer(&h);

    // Retitled between the two turns: the earlier comment does not move with it.
    let mut retitled = AgentDraft {
        nickname: "arch".into(),
        title: "Architect".into(),
        model_id: "m".into(),
        instructions: String::new(),
        reasoning: Some(ReasoningChoice::Effort {
            effort: "high".into(),
        }),
    };
    agents::update_agent_impl(&h.store(), "", &agent.id, &retitled).expect("retitle");
    answer(&h);

    // Cleared: the snapshot is the empty string that was stored, and never the
    // `Not defined` a *prompt* would have carried for the same agent.
    retitled.title = String::new();
    agents::update_agent_impl(&h.store(), "", &agent.id, &retitled).expect("clear the title");
    answer(&h);

    assert_eq!(
        titles(&h),
        vec![
            Some("Developer".to_string()),
            Some("Architect".to_string()),
            Some(String::new()),
        ],
    );
}

// ---------------------------------------------------------------------------
// AGC-FR-21, AGC-FR-22: events and listing
// ---------------------------------------------------------------------------

#[test]
fn exactly_two_events_are_delivered_per_turn_and_the_listing_agrees_with_them() {
    // AGC-FR-21, AGC-FR-22.
    let h = Harness::new(vec![Ok("done".into())]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");

    let (tx, rx) = mpsc::channel();
    h.app.listen(AGENT_TURN_STATE_CHANGED, move |event| {
        let _ = tx.send(event.payload().to_string());
    });

    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");
    // `settle()` is not enough to start draining: `terminate` empties the
    // in-flight set **before** the terminal event is published, so settling
    // returns with the second of the two events still to come and the drain
    // below can start on an empty-but-unfinished channel. The assertion is an
    // exact count, so that reads as a hard failure rather than a slow test.
    // Waiting for the terminal event itself is what makes the count a count.
    wait_for_terminal(&h, &turn.id);

    let mut payloads = Vec::new();
    while let Ok(payload) = rx.recv_timeout(Duration::from_millis(300)) {
        payloads.push(payload);
    }
    let mine: Vec<&String> = payloads
        .iter()
        .filter(|p| p.contains(&format!("\"{}\"", turn.id)))
        .collect();
    assert_eq!(mine.len(), 2, "one registration and one terminal event");
    assert!(mine[0].contains("\"running\""));
    assert!(mine[1].contains("\"delivered\""));
    assert!(h.turns().in_flight(None).is_empty());
}

#[test]
fn a_null_origin_returns_every_turn_in_flight_and_a_named_one_returns_only_its_own() {
    // AGC-FR-22.
    let h = Harness::with(
        vec![Ok("a".into()), Ok("b".into())],
        Duration::from_millis(400),
        4,
    );
    h.create_agent("arch", "");
    let one = h.seed_artifact_thread("one.md", "First artifact source.");
    let two = h.seed_artifact_thread("two.md", "Second artifact source.");
    let origin_one = ConversationOrigin::of(&one);
    let origin_two = ConversationOrigin::of(&two);
    h.dispatch("arch", origin_one.clone(), &one.comments[0].id)
        .expect("one");
    h.dispatch("arch", origin_two.clone(), &two.comments[0].id)
        .expect("two");

    assert_eq!(h.turns().in_flight(None).len(), 2);
    assert_eq!(h.turns().in_flight(Some(&origin_one)).len(), 1);
    assert_eq!(h.turns().in_flight(Some(&origin_two)).len(), 1);
    h.settle();
    assert!(h.turns().in_flight(Some(&origin_one)).is_empty());
}

// ---------------------------------------------------------------------------
// AGC-FR-24, PRG-FR-02, PRG-FR-08 / AGC-FR-25, AGC-FR-03: progress and the concurrency bound
// ---------------------------------------------------------------------------

/// The agent operations in flight now.
fn agent_operations(h: &Harness) -> Vec<crate::progress::Operation> {
    h.progress()
        .in_flight()
        .into_iter()
        .filter(|op| op.kind == "agent")
        .collect()
}

/// Wait for an agent operation to be in flight. A turn registers it on its own
/// thread, once it holds a call slot.
fn wait_for_agent_operation(h: &Harness) -> crate::progress::Operation {
    for _ in 0..500 {
        if let Some(op) = agent_operations(h).into_iter().next() {
            return op;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no agent operation was reported");
}

#[test]
fn a_running_turn_is_reported_as_an_agent_operation_and_leaves_the_set_when_it_ends() {
    // PRG-FR-02, PRG-FR-08 / AGC-FR-24, PRG-FR-KXQW.
    let h = Harness::with(vec![Ok("ok".into())], Duration::from_millis(300), 4);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");

    let mine = wait_for_agent_operation(&h);
    // AGC-FR-24: the producer's label names the agent and the artifact path.
    assert_eq!(mine.label, "@arch is thinking about spec.md…");
    // AGC-FR-24, PRG-FR-KXQW: the destination holds the discussion id.
    assert_eq!(
        mine.activation,
        Some(crate::progress::Activation::Discussion {
            discussion_id: thread.id.clone()
        })
    );

    wait_for_terminal(&h, &turn.id);
    for _ in 0..200 {
        if !h.progress().in_flight().iter().any(|op| op.kind == "agent") {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the agent operation was left in flight");
}

#[test]
fn a_turn_waiting_for_a_call_slot_is_not_reported_until_it_holds_one() {
    // AGC-FR-24, AGC-FR-25: queued work is not in-flight progress. With one
    // slot, two turns are both `running` as turns, and only the one holding the
    // slot is an operation; the other appears when the slot frees.
    let h = Harness::with(
        vec![Ok("1".into()), Ok("2".into())],
        Duration::from_millis(400),
        1,
    );
    h.create_agent("arch", "");
    let one = h.seed_artifact_thread("one.md", "First artifact source.");
    let two = h.seed_artifact_thread("two.md", "Second artifact source.");
    h.dispatch("arch", ConversationOrigin::of(&one), &one.comments[0].id)
        .expect("one");
    let first = wait_for_agent_operation(&h);
    h.dispatch("arch", ConversationOrigin::of(&two), &two.comments[0].id)
        .expect("two");

    assert_eq!(h.turns().in_flight(None).len(), 2, "both turns are registered");
    let reported = agent_operations(&h);
    assert_eq!(reported.len(), 1, "the waiting turn is not reported: {reported:?}");
    assert_eq!(reported[0].id, first.id);

    // The slot frees, and the second turn is reported on its own terms.
    for _ in 0..500 {
        let now = agent_operations(&h);
        if now.iter().any(|op| op.label.contains("two.md")) {
            // The first turn gives up its slot a moment before it ends its own
            // operation, so both can be in flight for that moment. Both are
            // gone once the turns have ended.
            h.settle();
            for _ in 0..200 {
                if agent_operations(&h).is_empty() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("an agent operation was left in flight");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the second turn was never reported");
}

#[test]
fn a_turn_cancelled_while_it_waits_for_a_slot_never_registers_an_operation() {
    // AGC-FR-24: a turn that ends before it holds a slot registers no operation,
    // and the turn holding the slot is unaffected.
    let h = Harness::with(
        vec![Ok("1".into()), Ok("2".into())],
        Duration::from_millis(300),
        1,
    );
    let arch = h.create_agent("arch", "");
    let sec = h.create_agent("sec", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let trigger = thread.comments[0].id.clone();
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    {
        let seen = seen.clone();
        h.app.listen(crate::progress::OPERATION_PROGRESS, move |event| {
            seen.lock().unwrap().push(event.payload().to_string());
        });
    }

    h.dispatch("arch", origin.clone(), &trigger).expect("arch");
    wait_for_agent_operation(&h);
    h.dispatch("sec", origin, &trigger).expect("sec");
    assert_eq!(h.turns().in_flight(None).len(), 2);

    // The second turn is still waiting for the only slot, so cancelling it
    // cancels a turn that has no operation yet.
    let handle = h.app.handle().clone();
    cancel_turns_for_agent(&handle, &h.turns(), &h.progress(), &sec.id, None);
    assert_eq!(h.turns().in_flight(None).len(), 1, "only the waiting turn ended");
    assert_eq!(agent_operations(&h).len(), 1, "the running turn is still reported");
    assert!(agent_operations(&h)[0].label.contains("@arch"));

    // The slot frees and the cancelled turn wakes: it still registers nothing.
    h.settle();
    assert!(agent_operations(&h).is_empty());
    let events = seen.lock().unwrap();
    assert!(
        !events.iter().any(|payload| payload.contains("@sec")),
        "no operation was ever registered for the cancelled turn: {events:?}"
    );
    let _ = arch;
}

#[test]
fn a_progress_label_names_the_agent_and_the_draft_name_the_artifact_path_or_nothing_for_a_note() {
    // AGC-FR-24, PRG-FR-HDQS: the producer supplies the subject.
    let h = Harness::new(vec![]);
    let root = h.root();
    let roots = Roots::same(&root);
    let created = crate::drafts::create_draft_at_root(&root, Some("Push button")).expect("draft");

    let draft = ConversationOrigin::stub_draft("d1", created.draft.id.clone(), false);
    assert_eq!(
        progress_label_of(roots, &draft, "Helga"),
        "@Helga is thinking about Push button…"
    );
    // A remark about a fragment of the draft names the draft as well.
    let draft_comment = ConversationOrigin::stub_draft("d1", created.draft.id.clone(), true);
    assert_eq!(
        progress_label_of(roots, &draft_comment, "Helga"),
        "@Helga is thinking about Push button…"
    );
    // A draft whose record cannot be read is named by its id.
    let missing = ConversationOrigin::stub_draft("d2", "draft-gone", false);
    assert_eq!(
        progress_label_of(roots, &missing, "Helga"),
        "@Helga is thinking about draft-gone…"
    );
    let artifact = ConversationOrigin::stub_artifact("d3", "docs/spec.md", false);
    assert_eq!(
        progress_label_of(roots, &artifact, "Helga"),
        "@Helga is thinking about docs/spec.md…"
    );
    let note = ConversationOrigin::stub_note("d4", "note-1");
    assert_eq!(progress_label_of(roots, &note, "Helga"), "@Helga is thinking…");
}

#[test]
fn more_turns_than_the_bound_are_all_registered_and_none_is_refused() {
    // AGC-FR-25, AGC-FR-03.
    let h = Harness::with(
        vec![
            Ok("1".into()),
            Ok("2".into()),
            Ok("3".into()),
            Ok("4".into()),
        ],
        Duration::from_millis(120),
        1,
    );
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let origin = ConversationOrigin::of(&thread);
    let trigger = thread.comments[0].id.clone();

    for _ in 0..4 {
        let turn = h
            .dispatch("arch", origin.clone(), &trigger)
            .expect("never refused for load");
        assert_eq!(turn.state, AgentTurnState::Running);
    }
    assert_eq!(h.turns().in_flight(None).len(), 4, "all registered at once");
    h.settle();
    assert_eq!(h.seam.requests().len(), 4, "every turn eventually ran");
    // The bound itself, measured from inside the seam rather than by sampling
    // from outside: a count taken here could only ever say what had *started*,
    // which is a race rather than an assertion.
    assert_eq!(
        h.seam.peak_concurrency(),
        1,
        "no more calls were in flight at once than the bound permits",
    );
}
