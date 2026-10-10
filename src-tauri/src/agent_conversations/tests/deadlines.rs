//! The provider-call deadline, told apart from the turn's own, and the
//! entries a provider decides.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// CVL-FR-16, CVL-FR-17, CVL-FR-18, AGC-FR-18 / CVL-FR-17: the provider-call deadline, told apart from the turn's
// ---------------------------------------------------------------------------

/// A never-answering seam with its call counter and the deadlines it was given.
pub(super) fn counting_never_answers() -> (
    Box<dyn CompletionSeam>,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<Duration>>>,
) {
    let calls = Arc::new(AtomicUsize::new(0));
    let deadlines = Arc::new(Mutex::new(Vec::new()));
    (
        Box::new(CountingNeverAnswers {
            calls: calls.clone(),
            deadlines: deadlines.clone(),
        }),
        calls,
        deadlines,
    )
}

#[test]
fn a_provider_call_that_expires_is_retried_at_its_own_deadline() {
    // CVL-FR-16, AGC-FR-18 (first half) / CVL-FR-17, CVL-FR-18, CVL-FR-19.
    //
    // The real `block_on_with_timeout` abandons each call, so what is exercised
    // is the production expiry rather than a scripted `timed_out` string — the
    // whole point of CVL-FR-17 being that a per-call deadline is a thing that
    // happens rather than a value a seam reports.
    let (seam, calls, deadlines) = counting_never_answers();
    let h = Harness::with_seam_and_retries(
        seam,
        RetryPolicy {
            call_timeout: Duration::from_millis(60),
            first_base: Duration::from_millis(1),
            ..RetryPolicy::default()
        },
        // Comfortably beyond three of those, so the turn's own deadline is not
        // what any of them hits.
        Duration::from_secs(30),
    );
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(calls.load(Ordering::SeqCst), MAX_ATTEMPTS, "three attempts");
    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    // The provider-call deadline, which is recoverable — unlike the whole-turn
    // one it shares a spelling with.
    assert!(terminal.retry_permitted);
    assert_eq!(h.turns().recoverable_failures(None).len(), 1);
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1, "no line was gained");

    // Each call was given the per-call deadline rather than what the turn had
    // left, the turn having far more.
    let given = deadlines.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(given.len(), MAX_ATTEMPTS);
    for deadline in given {
        assert_eq!(deadline, Duration::from_millis(60));
    }

    // CVL-FR-21: and the class says which expiry it was.
    let failed = records_where("turnId", &terminal.id)
        .into_iter()
        .filter(|r| r.message == "model call failed")
        .collect::<Vec<_>>();
    assert_eq!(failed.len(), MAX_ATTEMPTS);
    for record in &failed {
        assert_eq!(field(record, "failureClass"), class::PROVIDER_TIMEOUT);
    }
}

#[test]
fn the_whole_turn_deadline_caps_the_call_and_is_not_retried() {
    // CVL-FR-17 (second half) / CVL-FR-17, CVL-FR-16, CVL-FR-18.
    //
    // Five minutes is the constant, and the turn has a fraction of it — so the
    // call is given what the turn has left, and its expiry is the *turn's*
    // deadline rather than the call's. No retry follows: a budget spent by
    // definition is not one repeating the call would help.
    let (seam, calls, deadlines) = counting_never_answers();
    let h = Harness::with_seam_and_retries(
        seam,
        RetryPolicy::default(),
        Duration::from_millis(150),
    );
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(calls.load(Ordering::SeqCst), 1, "no retry began after the deadline");
    let given = deadlines.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(given.len(), 1);
    assert!(
        given[0] <= Duration::from_millis(150),
        "the call was capped by what the turn had left, got {:?}",
        given[0],
    );
    assert!(
        given[0] < PROVIDER_CALL_TIMEOUT,
        "five minutes did not outlive the turn it belongs to",
    );

    assert_eq!(terminal.failure.as_deref(), Some(FAIL_TIMED_OUT));
    assert!(!terminal.retry_permitted, "the whole-turn deadline offers nothing");
    assert!(h.turns().recoverable_failures(None).is_empty());
    let record = find(
        &records_where("turnId", &terminal.id),
        "agent turn ran out of time",
    );
    assert_eq!(field(&record, "deadline"), "whole_turn");
}

#[test]
fn a_thread_locked_during_a_backoff_wait_stops_the_retry() {
    // CVL-FR-25 (second half) / CVL-FR-19, AGC-FR-19, AGC-FR-31.
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

    // Lock it while the turn is sitting in its first backoff.
    for _ in 0..500 {
        if h.seam.call_count() >= 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    // Asserted rather than assumed: a budget that ran out locks a thread against
    // a turn that has not attempted anything yet, which is a different test than
    // this one — one that would still pass.
    assert!(h.seam.call_count() >= 1, "the first attempt never began");
    crate::comments::set_lock_in(
        &h.root(),
        "spec.md",
        &thread.id,
        true,
        &human("ada"),
        "2024-01-01T00:00:00Z",
    )
    .expect("lock");

    let terminal = wait_for_terminal(&h, &turn.id);
    assert_eq!(terminal.state, AgentTurnState::Failed);
    // A conversation that takes no further contribution has nowhere to deliver
    // whatever a retry produced, so the turn ends on the lock rather than on
    // what the provider last did.
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_THREAD_LOCKED));
    assert!(!terminal.retry_permitted);
    assert!(h.turns().recoverable_failures(None).is_empty());
    assert_eq!(h.seam.call_count(), 1, "the retry the wait was for was never made");
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1);
}

#[test]
fn a_panicked_turn_is_not_offered_a_retry() {
    // CVL-FR-18: the recoverable three are the failures where repeating the same
    // call could plausibly succeed. A defect in this module repeats
    // deterministically, so offering the author a Retry against one would invite
    // them to press it until they gave up.
    let h = Harness::with_patient_seam(Box::new(PanickingCompletion));
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some(FAIL_UNREACHABLE));
    assert!(!terminal.retry_permitted);
    assert!(h.turns().recoverable_failures(None).is_empty());
    assert_eq!(h.threads("spec.md")[0].comments.len(), 1);
}

#[test]
fn deleting_a_note_takes_its_conversations_offer_with_it() {
    // AGC-FR-31 (third clause) / AGC-FR-31, AGC-FR-30.
    //
    // An offer to retry in a conversation that is gone with its note is an offer
    // nothing could take: the thread it names resolves to nothing, so the retry
    // would be refused before it registered anyway (AGC-FR-30) and the surface
    // would be left holding a control that could only ever fail.
    let h = Harness::new(vec![Err(FAIL_UNREACHABLE); MAX_ATTEMPTS]);
    h.create_agent("arch", "");
    let (note_id, origin) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "about to go",
        "@arch thoughts?",
    );
    let trigger = crate::comments::note_discussion_of(&h.root(), &note_id)
        .unwrap()
        .comments[0]
        .id
        .clone();

    let turn = h.dispatch("arch", origin, &trigger).expect("dispatch");
    let terminal = wait_for_terminal(&h, &turn.id);
    assert!(terminal.retry_permitted);
    assert_eq!(h.turns().recoverable_failures(None).len(), 1);

    crate::notes::delete_note_in(&h.root(), &h.root(), &note_id).unwrap();
    cancel_turns_for_note(
        &h.app.handle().clone(),
        &h.turns(),
        &h.progress(),
        &note_id,
    );

    assert!(
        h.turns().recoverable_failures(None).is_empty(),
        "the offer went with the conversation",
    );
    let app = h.app.handle().clone();
    assert_eq!(
        retry_impl(&app, Roots::same(&h.root()), PROJECT_KEY, &terminal.id).unwrap_err(),
        ERR_TURN_NOT_FOUND,
    );
}

/// CVL-FR-29, CVL-FR-04, CVL-FR-02: no authoring comment of a prompt template reaches a turn, and a
/// placeholder written inside one is never substituted.
///
/// The templates carry no comment today. The rule binds whichever of them grows
/// one, so the substitution half of it is exercised against a template built
/// here rather than against a file that happens not to test it.
#[test]
fn no_authoring_comment_reaches_a_turn() {
    use super::{
        compile_prompt, prompt_instruction, prompt_template, AGENT_INSTRUCTIONS_PLACEHOLDER,
        AGENT_TITLE_PLACEHOLDER,
    };

    for kind in [
        OriginKind::ArtifactComment,
        OriginKind::DraftComment,
        OriginKind::ArtifactDiscussion,
        OriginKind::DraftDiscussion,
        OriginKind::NoteDiscussion,
    ] {
        let compiled = compile_prompt(kind, "Argue about structure.", "Architect");
        assert!(
            !compiled.contains("<!--") && !compiled.contains("-->"),
            "{kind:?} sends a comment delimiter to a model",
        );
        assert_eq!(
            compiled, compiled.trim_start(),
            "{kind:?} begins with blank space",
        );
        // A template carrying no comment contributes itself unchanged.
        let raw = prompt_template(kind);
        if !raw.contains("<!--") {
            assert_eq!(prompt_instruction(kind), raw, "{kind:?} was altered");
        }
    }

    // CVL-FR-29: the removal happens **before** the substitution, and it is
    // asserted through the composition itself rather than beside it. A build
    // that substituted first and stripped afterwards would put the agent's
    // instructions into the note's placeholder and then delete the note along
    // with them — so the whole instruction goes missing, which is what the
    // second assertion below catches.
    let template = format!(
        "<!-- a note holding {AGENT_INSTRUCTIONS_PLACEHOLDER} -->\n\nYou are {AGENT_TITLE_PLACEHOLDER}. {AGENT_INSTRUCTIONS_PLACEHOLDER}\n",
    );
    let compiled = super::compile_from_template(&template, "Argue about structure.", "Architect");
    assert_eq!(compiled, "You are Architect. Argue about structure.\n");
    assert!(
        !compiled.contains(AGENT_INSTRUCTIONS_PLACEHOLDER),
        "the placeholder inside the comment survived the removal",
    );
    assert!(!compiled.contains("<!--") && !compiled.contains("-->"));
    assert!(
        !compiled.contains("a note holding"),
        "the comment's own words reached the model",
    );
}

// ---------------------------------------------------------------------------
// CVL-FR-08 — the entries, and the provider that decides them (CVL-FR-30)
// ---------------------------------------------------------------------------

#[test]
fn an_openrouter_conversation_carries_the_search_entry_and_withholds_the_fetch() {
    // CVL-FR-30, CVL-FR-08 / WST-FR-02, WST-FR-03 / WFT-FR-02, WFT-FR-03: both entries stand on every
    // conversational request OpenRouter carries, beside exactly the portable
    // tools the origin kind decides — the entries being decided by the provider
    // and by nothing else.
    //
    // Two agents and two projects here. The other dimensions of the claim —
    // every origin kind, and every selected model — are discharged by the
    // invariant `ScriptedCompletion::complete` asserts on *every* dispatch this
    // suite makes, which is a wider sample of both than any one test could
    // enumerate. What this test adds is the comparison between them: the
    // entries are byte-identical across agents and projects, and the portable
    // set beside them is untouched by their presence.
    let mut seen: Vec<Vec<crate::tools::ProviderNativeTool>> = Vec::new();
    for nickname in ["arch", "sec"] {
        for project in [PROJECT_KEY, "other-project"] {
            let h = Harness::new(vec![Ok("Answered.".into())]);
            h.mount();
            let agent = h.create_agent(nickname, "");
            agents::enrol_project_agent_impl(&h.store(), project, &agent.id).expect("enrol");
            let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
            let turn = h
                .dispatch_in(
                    project,
                    nickname,
                    ConversationOrigin::of(&thread),
                    &thread.comments[0].id,
                )
                .expect("dispatch");
            wait_for_terminal(&h, &turn.id);
            for (request, endpoint) in h.seam.requests() {
                assert_eq!(endpoint.provider, "openrouter");
                // The portable set is still exactly what the origin kind
                // decides: the entries neither add nor remove one of these.
                let portable: Vec<&str> =
                    request.tools.iter().map(|t| t.name.as_str()).collect();
                assert_eq!(portable.len(), 12, "the eleven and one proposal tool");
                assert!(
                    !portable
                        .iter()
                        .any(|name| name.contains("web_search") || name.contains("web_fetch")),
                    "a provider-native capability is no portable tool",
                );
                seen.push(request.native_tools.clone());
            }
        }
    }
    assert_eq!(seen.len(), 4, "one request per dispatch");
    for entries in &seen {
        assert_eq!(
            entries,
            &vec![crate::tools::web_search::entry()],
            "the search entry alone (CVL-FR-30)",
        );
        assert_eq!(
            serde_json::to_value(entries).expect("serialisable"),
            serde_json::json!([{ "type": "openrouter:web_search" }]),
            "the entry is its type and nothing else",
        );
        // WFT-FR-16: withheld rather than removed. The entry is still the fixed
        // text it always was, and the only thing that changed is that nobody
        // offers it — so restoring it is a decision rather than a rebuild.
        assert!(
            !entries.contains(&crate::tools::web_fetch::entry()),
            "the fetch entry is offered to no turn while it is withheld",
        );
    }
    assert_eq!(
        crate::tools::web_fetch::entry().tool_type,
        "openrouter:web_fetch",
        "and it is unchanged, waiting to be offered again (WFT-FR-02)",
    );
    // Byte-identical across agents and projects: nothing varied them.
    assert!(seen.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn a_conversation_on_another_provider_carries_neither_entry() {
    // CVL-FR-30, CVL-FR-08 / WST-FR-04 / WFT-FR-04: a request carried by a provider this
    // application has no native contract for carries neither entry and nothing
    // standing in for them, while the portable tools it offers are exactly the
    // ones its origin kind decides.
    for provider in ["anthropic", "openai", "custom"] {
        let h = Harness::new(vec![Ok("Answered.".into())]);
        h.mount();
        h.seed_second_provider(provider);
        h.create_plain_agent("arch");
        let (terminal, _) = run_one(&h, "arch");
        assert_eq!(terminal.state, AgentTurnState::Delivered);
        let requests = h.seam.requests();
        assert_eq!(requests.len(), 1);
        let (request, endpoint) = &requests[0];
        assert_eq!(&endpoint.provider, provider);
        assert!(
            request.native_tools.is_empty(),
            "{provider} carries no provider-native entry (CVL-FR-30)",
        );
        // CVL-FR-34: present and empty rather than absent, so a log query that
        // filters on it does not silently drop every turn on these providers.
        let calling = wait_for_record(&terminal.id, "calling model");
        assert_eq!(require(&calling, "nativeTools"), "0");
        assert_eq!(require(&calling, "nativeToolTypes"), "");
        let portable: Vec<&str> = request.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(portable.len(), 12, "the same portable set as anywhere else");
        assert!(
            !portable
                .iter()
                .any(|name| name.contains("web_search") || name.contains("web_fetch")),
            "and nothing local stands in for the absent capability",
        );
    }
}

/// One image part, as a section's material carries it.
fn image_part(name: &str) -> InputPart {
    InputPart::Image(InputImage {
        data: "AAAA".into(),
        media_type: "image/png".into(),
        context: format!("![{name}]({name}.png)"),
        metadata: format!("image/png {name}.png"),
    })
}

fn openrouter_request_for(model: &str) -> serde_json::Value {
    openrouter_request_over(model, plain_material(), false)
}

/// The material a conversation turn opens with: the artifact, then what the
/// conversation has grown around it.
fn plain_material() -> Vec<InputSection> {
    vec![
        InputSection {
            tag: TAG_ARTIFACT.into(),
            attributes: vec![("path".into(), "spec.md".into())],
            body: "The material.".into(),
            truncated: false,
            parts: Vec::new(),
        },
        InputSection {
            tag: TAG_DISCUSSION_HISTORY.into(),
            attributes: Vec::new(),
            body: "What was said.".into(),
            truncated: false,
            parts: Vec::new(),
        },
    ]
}

fn openrouter_request_over(
    model: &str,
    input: Vec<InputSection>,
    with_images: bool,
) -> serde_json::Value {
    let request = AgentRequest {
        instructions: compile_prompt(OriginKind::ArtifactComment, "Argue.", ""),
        stable_head_sections: stable_head_of(&input),
        input,
        tools: Vec::new(),
        native_tools: Vec::new(),
    };
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "openrouter".into(),
        base_url: "https://example.invalid".into(),
        api_key: None,
        model_id: Some(model.into()),
        reasoning: None,
        accepts_image_input: false,
                    model_mode: None,
    };
    let exchange = opening_exchange(&request, with_images);
    let built = openrouter_request(&request, &exchange, &endpoint).expect("assembled");
    serde_json::to_value(&built).expect("serialisable")
}

/// The first user message's content parts, each paired with its mark, as they
/// reach the wire.
fn wire_input_parts(wire: &serde_json::Value) -> Vec<(String, Option<String>)> {
    match &wire["messages"][1]["content"] {
        serde_json::Value::String(text) => vec![(text.clone(), None)],
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|part| {
                (
                    part["text"].as_str().unwrap_or_default().to_string(),
                    part.get("cache_control")
                        .and_then(|control| control["type"].as_str())
                        .map(str::to_string),
                )
            })
            .collect(),
        other => panic!("expected text content, got {other:?}"),
    }
}

#[test]
fn the_openrouter_call_asks_the_client_to_advance_its_own_boundary() {
    // CVL-FR-39 / CVL-FR-23. What grows is held by a boundary the client moves,
    // asked for at the request level — which is what lets it hold a block this
    // module never has to reshape.
    let wire = openrouter_request_for("cache-boundary-test-model");
    assert_eq!(
        wire.get("cache_control"),
        Some(&serde_json::json!({ "type": "ephemeral" })),
        "the call asks for an advancing boundary at the short lifetime: {wire}",
    );
    // CVL-FR-39: and the two fixed marks stand where the material stops.
    let parts = wire["messages"][1]["content"]
        .as_array()
        .expect("the input travels as parts");
    assert_eq!(parts.len(), 2, "the head and the tail: {parts:?}");
    assert_eq!(parts[0]["cache_control"]["type"], "ephemeral");
    assert!(
        parts[1].get("cache_control").is_none(),
        "what the conversation grows carries no mark of its own",
    );
    assert_eq!(
        wire["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral",
        "the compiled prompt is marked",
    );
}

#[test]
fn an_input_carrying_a_picture_splits_at_the_same_boundary() {
    // CVL-FR-23 / CVL-FR-39, CVL-FR-37. The head is rendered through whichever
    // renderer produced the message, so a turn carrying a picture is split where
    // a turn without one is. Driven all the way to the serialized request rather
    // than stopping at the head, because the message a multimodal turn builds is
    // a part list and the split has to survive the crossing into this SDK.
    let mut material = plain_material();
    material[0].parts = vec![
        InputPart::Text("Before.\n".into()),
        image_part("a"),
        InputPart::Text("After.".into()),
    ];
    let wire = openrouter_request_over("cache-multimodal-test-model", material, true);
    let parts = wire_input_parts(&wire);
    assert_eq!(parts.len(), 2, "the head and the tail: {parts:?}");
    assert_eq!(
        parts[0].1.as_deref(),
        Some("ephemeral"),
        "the head carries the mark",
    );
    assert_eq!(parts[1].1, None, "what the conversation grows carries none");
    assert!(
        parts[0].0.contains("Before.") && parts[0].0.contains("After."),
        "the head is the material, prose either side of its picture: {:?}",
        parts[0].0,
    );
    assert!(
        !parts[0].0.contains("What was said."),
        "the head stops before what the conversation grows",
    );
    assert!(
        parts[1].0.contains("What was said."),
        "and the tail carries it: {:?}",
        parts[1].0,
    );
}

#[test]
fn a_head_of_prose_splits_the_same_when_a_later_section_carries_a_picture() {
    // CVL-FR-23 / CVL-FR-39. The head itself holds no picture while the input
    // does, so the head is rendered by one renderer and the message it must be a
    // prefix of by the other. The two agree today because a section with no
    // image part renders its prose the same way either way — this is what keeps
    // that true rather than leaving it to coincidence.
    let mut material = plain_material();
    material[1].parts = vec![
        InputPart::Text("What was said.\n".into()),
        image_part("b"),
    ];
    let wire = openrouter_request_over("cache-late-picture-test-model", material, true);
    let parts = wire_input_parts(&wire);
    assert_eq!(parts.len(), 2, "the head is still split off: {parts:?}");
    assert_eq!(parts[0].1.as_deref(), Some("ephemeral"));
    assert!(parts[0].0.contains("The material."));
    assert!(!parts[0].0.contains("What was said."));
}

#[test]
fn the_boundary_and_the_marks_stand_on_every_round() {
    // CVL-FR-39 / CVL-FR-23. Not the first call alone: the exchange grows every
    // round, and a boundary asked for only while it was short would leave the
    // rounds that cost the most uncovered.
    let request = AgentRequest {
        instructions: compile_prompt(OriginKind::ArtifactComment, "Argue.", ""),
        stable_head_sections: 1,
        input: plain_material(),
        tools: Vec::new(),
        native_tools: Vec::new(),
    };
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "openrouter".into(),
        base_url: "https://example.invalid".into(),
        api_key: None,
        model_id: Some("cache-rounds-test-model".into()),
        reasoning: None,
        accepts_image_input: false,
                    model_mode: None,
    };
    let mut exchange = opening_exchange(&request, false);
    for round in 0..4 {
        let built = openrouter_request(&request, &exchange, &endpoint).expect("assembled");
        let wire = serde_json::to_value(&built).expect("serialisable");
        assert_eq!(
            wire.get("cache_control"),
            Some(&serde_json::json!({ "type": "ephemeral" })),
            "round {round}: the call asks for an advancing boundary",
        );
        let parts = wire_input_parts(&wire);
        assert_eq!(parts.len(), 2, "round {round}: the head is split off");
        assert_eq!(parts[0].1.as_deref(), Some("ephemeral"));
        assert_eq!(parts[1].1, None);

        let id = format!("call-{round}");
        let call = rig_tool_call(&id, "list_skills", serde_json::json!({}));
        exchange.push(rig::completion::Message::Assistant(
            rig::completion::message::AssistantMessage::new(vec![
                rig::completion::message::AssistantContent::ToolCall(call.clone()),
            ]),
        ));
        exchange.push(rig::completion::Message::tool_result(
            call.id.clone(),
            call.function.name.clone(),
            "RESULT",
        ));
    }
}

#[test]
fn a_repeated_call_of_one_round_presents_byte_identical_marks_and_pin() {
    // CVL-FR-01, CVL-FR-10 / CVL-FR-20, CVL-FR-23. A retry repeats the failed call and
    // nothing else, and the marking is derived from the exchange rather than
    // chosen per attempt — so two builds of one round agree byte for byte, the
    // pin among them.
    let model = "cache-retry-test-model";
    remember_upstream(model, "Anthropic");
    let first = openrouter_request_over(model, plain_material(), false);
    let second = openrouter_request_over(model, plain_material(), false);
    assert_eq!(first, second, "a retry presents what its attempt presented");
    assert!(
        first.get("provider").is_some() && first.get("cache_control").is_some(),
        "and there was something to agree about: {first}",
    );
    forget_upstream(model);
}

/// One OpenRouter answer, built the way `crate::ai_openrouter`'s fixtures are:
/// through deserialisation, because the SDK's response types are
/// `#[non_exhaustive]` — which has the side benefit of pinning the **wire**
/// shape this module reads rather than an in-memory one.
fn openrouter_answer(provider: Option<&str>, prompt_tokens: u32) -> serde_json::Value {
    let mut answer = serde_json::json!({
        "id": "gen-1",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "Settled." },
            "finish_reason": "stop",
        }],
        "created": 0,
        "model": "m",
        "object": "chat.completion",
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": 2,
            "total_tokens": prompt_tokens + 2,
        },
    });
    if let Some(provider) = provider {
        answer["provider"] = serde_json::json!(provider);
    }
    answer
}

fn openrouter_reply(model: &str, provider: Option<&str>, prompt_tokens: u32) -> ModelReply {
    let response: openrouter_rs::types::completion::CompletionsResponse =
        serde_json::from_value(openrouter_answer(provider, prompt_tokens))
            .expect("the fixture parses as an answer");
    reply_from_openrouter(&response, model, 2)
}

#[test]
fn an_answer_carries_the_upstream_it_names_and_is_remembered_by_it() {
    // CVL-FR-40, CVL-FR-28. The whole chain the routing rests on —
    // what the client reported, what the reply carries, and what the next call
    // of that model will ask for — driven without a request leaving the machine.
    let model = "cache-answer-test-model";
    let reply = openrouter_reply(model, Some("Anthropic"), 4_000);
    assert_eq!(reply.served_by.as_deref(), Some("Anthropic"));
    assert_eq!(reply.prompt_tokens, Some(4_000));
    // CVL-FR-28: the provider's own identifiers for the response, the model it
    // says answered, and why the reply ended. This client reports no
    // cache-write or reasoning count and no request id of its own.
    assert_eq!(reply.response_id.as_deref(), Some("gen-1"));
    assert_eq!(reply.response_model.as_deref(), Some("m"));
    assert_eq!(reply.finish_reason.as_deref(), Some("stop"));
    assert_eq!(reply.cache_write_tokens, None);
    assert_eq!(reply.reasoning_tokens, None);
    assert_eq!(reply.provider_request_id, None);
    assert_eq!(
        upstream_for(model).as_deref(),
        Some("Anthropic"),
        "an answered call is what the next call of this model asks for",
    );

    // A client that named no upstream leaves the pin standing rather than
    // clearing it, and reports none of its own.
    let reply = openrouter_reply(model, None, 4_600);
    assert_eq!(reply.served_by, None);
    assert_eq!(upstream_for(model).as_deref(), Some("Anthropic"));

    // An upstream named as nothing at all is the same as none named.
    let reply = openrouter_reply(model, Some(""), 4_600);
    assert_eq!(reply.served_by, None);
    assert_eq!(upstream_for(model).as_deref(), Some("Anthropic"));

    // A fallback re-pins to wherever the call actually landed.
    let reply = openrouter_reply(model, Some("Amazon Bedrock"), 4_600);
    assert_eq!(reply.served_by.as_deref(), Some("Amazon Bedrock"));
    assert_eq!(upstream_for(model).as_deref(), Some("Amazon Bedrock"));

    // CVL-FR-28: a provider that counted nothing is not recorded as having
    // counted zero — no call presents nothing, every one of them carrying at
    // least the compiled prompt.
    assert_eq!(openrouter_reply(model, Some("Anthropic"), 0).prompt_tokens, None);
    forget_upstream(model);
}

#[test]
fn a_call_asks_for_the_upstream_that_served_the_last_one() {
    // CVL-FR-28 / CVL-FR-40. A cache belongs to the upstream that wrote it, so
    // the call asks for the one that answered last — and keeps fallback allowed,
    // an upstream that has gone away costing the call its cache and never the
    // call itself. A model id of this test's own, because the affinity is
    // process-wide and a shared one would make two tests each other's business.
    let model = "cache-affinity-test-model";
    assert!(
        openrouter_request_for(model).get("provider").is_none(),
        "a call made before anything is known of a model asks for no upstream",
    );

    remember_upstream(model, "Anthropic");
    let wire = openrouter_request_for(model);
    assert_eq!(
        wire.get("provider"),
        Some(&serde_json::json!({ "order": ["Anthropic"], "allow_fallbacks": true })),
        "the call asks for the upstream that answered, with fallback allowed: {wire}",
    );

    // A call the client reports as served elsewhere re-pins to where it landed,
    // so an affinity that outlived its upstream heals itself.
    remember_upstream(model, "Amazon Bedrock");
    assert_eq!(
        openrouter_request_for(model).get("provider"),
        Some(&serde_json::json!({ "order": ["Amazon Bedrock"], "allow_fallbacks": true })),
    );

    // An upstream the client did not name leaves the affinity as it stood
    // rather than clearing it or asking for an empty one.
    remember_upstream(model, "");
    assert_eq!(
        openrouter_request_for(model).get("provider"),
        Some(&serde_json::json!({ "order": ["Amazon Bedrock"], "allow_fallbacks": true })),
    );

    // CVL-FR-40: remembered per **model**. Two models of one provider are served
    // by different upstreams, so a pin learned for one must not reach the other
    // — a single process-wide slot would answer every assertion above and this
    // is what tells the two apart.
    assert!(
        openrouter_request_for("cache-affinity-other-model")
            .get("provider")
            .is_none(),
        "a model nothing is known of asks for no upstream, whatever another model pinned",
    );
    remember_upstream("cache-affinity-other-model", "Google AI Studio");
    assert_eq!(
        openrouter_request_for(model).get("provider"),
        Some(&serde_json::json!({ "order": ["Amazon Bedrock"], "allow_fallbacks": true })),
        "and pinning the other model leaves this one where it was",
    );
    forget_upstream(model);
    forget_upstream("cache-affinity-other-model");
}

#[test]
fn the_openrouter_call_presents_the_search_entry_in_its_tool_array() {
    // WST-FR-06 / CVL-FR-30: the entry reaches the wire as its own member of the
    // same `tools` array the function definitions travel in. Assembled from the
    // `AgentRequest` a dispatch produces — the entries are read off it here
    // exactly as the carrier reads them — so this covers the whole way from what
    // a turn decided to what a call presents, and pins it without a network
    // call.
    let request = AgentRequest {
        instructions: compile_prompt(OriginKind::ArtifactComment, "Argue.", ""),
        input: vec![InputSection {
            tag: TAG_ARTIFACT.into(),
            attributes: vec![("path".into(), "spec.md".into())],
            body: "Body.".into(),
            truncated: false,
            parts: Vec::new(),
        }],
        tools: vec![rig::completion::ToolDefinition {
            name: rig::completion::message::ToolName::new("read_file").expect("a tool name"),
            description: "Read a file.".into(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        }],
        native_tools: conversation_native_tools("openrouter"),
        stable_head_sections: 0,
    };
    let endpoint = AiApiCall {
        turn_timeout_ms: None,
        provider: "openrouter".into(),
        base_url: "https://example.invalid".into(),
        api_key: None,
        model_id: Some("m".into()),
        reasoning: None,
        accepts_image_input: false,
                    model_mode: None,
    };
    let exchange = opening_exchange(&request, false);
    let built = openrouter_request(&request, &exchange, &endpoint).expect("assembled");
    let wire = serde_json::to_value(&built).expect("serialisable");
    let tools = wire
        .get("tools")
        .and_then(|t| t.as_array())
        .expect("a tools array");
    assert_eq!(tools.len(), 2, "one portable definition and one entry");
    assert!(
        tools.contains(&serde_json::json!({ "type": "openrouter:web_search" })),
        "the search entry is a member of the tools array: {tools:?}",
    );
    // WFT-FR-16: withheld, so it reaches no wire.
    assert!(
        !tools.contains(&serde_json::json!({ "type": "openrouter:web_fetch" })),
        "the fetch entry reaches no request while it is withheld: {tools:?}",
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool.get("function").and_then(|f| f.get("name"))
                == Some(&serde_json::json!("read_file"))),
        "beside the portable definitions rather than instead of them",
    );
    // WST-FR-06: no member offers both capabilities. Still asserted while the
    // fetch is withheld, because what must never happen is the two being merged
    // behind one name — which is exactly the shape a hurried restoration would
    // reach for.
    assert!(
        !tools.iter().any(|tool| {
            let rendered = tool.to_string();
            rendered.contains("web_search") && rendered.contains("web_fetch")
        }),
        "search and fetch are never one member offering both",
    );

    // And a turn on any other provider presents neither (CVL-FR-30). Assembled
    // from the same request with the entries its own provider decides.
    let bare = AgentRequest {
        native_tools: conversation_native_tools("anthropic"),
        stable_head_sections: 0,
        ..request.clone()
    };
    let built = openrouter_request(&bare, &exchange, &endpoint).expect("assembled");
    let wire = serde_json::to_value(&built).expect("serialisable");
    let tools = wire
        .get("tools")
        .and_then(|t| t.as_array())
        .expect("a tools array");
    assert_eq!(tools.len(), 1, "the portable definition alone");
    assert!(
        !tools
            .iter()
            .any(|tool| tool.get("type") == Some(&serde_json::json!("openrouter:web_search"))
                || tool.get("type") == Some(&serde_json::json!("openrouter:web_fetch"))),
        "nothing attaches an entry the request did not carry",
    );
}
