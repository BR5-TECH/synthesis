//! A provider failure, and what of it reaches a surface.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// CVL-FR-32, CVL-FR-14, CVL-FR-33 / CVL-FR-27, CVL-FR-26, CVL-FR-34 — a provider failure, and what reaches a surface
// ---------------------------------------------------------------------------

#[test]
fn a_provider_tool_that_failed_is_a_result_rather_than_a_turn_failure() {
    // CVL-FR-32, CVL-FR-14, CVL-FR-33 / WST-FR-09, WST-FR-10, WST-FR-14 / WFT-FR-09, WFT-FR-10, WFT-FR-14: a failure the provider reports for one
    // of its own tools is that call's result. The model read it inside the same
    // call and answered from what it had, the turn delivers, and nothing was
    // retried on its account.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer(
        "I could not look it up, so: from memory.",
    )
    .and_native(
        "openrouter:web_search",
        "{\"query\":\"a\"}",
        "The search could not be completed.",
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert!(terminal.failure.is_none(), "a tool's failure is not the turn's");
    assert_eq!(h.seam.call_count(), 1, "answered in one call, and never retried");

    // CVL-FR-33: the registration, the terminal state, and the one call begun
    // and finished around the reporting of it — and nothing that names the
    // query, the result, or any other part of the exchange.
    let events = h.events.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mine: Vec<_> = events.iter().filter(|e| e.id == terminal.id).collect();
    assert_eq!(mine.len(), 4, "one activity event per begin and per finish");
    assert_eq!(
        mine.iter()
            .map(|e| e
                .active_tool_calls
                .iter()
                .map(|c| c.tool.as_str())
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            Vec::<&str>::new(),
            vec!["openrouter:web_search"],
            Vec::<&str>::new(),
            Vec::<&str>::new(),
        ],
        "the provider's call was active only while it was being reported",
    );
}

#[test]
fn provider_tool_traffic_reaches_no_surface_and_no_record() {
    // CVL-FR-32, CVL-FR-27, CVL-FR-26, CVL-FR-34 / WST-FR-13, WST-FR-14 / WFT-FR-13, WFT-FR-14 / TLC-FR-26: the query, the address, and
    // the page reach the model and nothing else. The comment carries the answer
    // alone, and no record carries any part of them — the records name the tool
    // and measure what it carried, which is a name fixed in the binary and two
    // lengths (CVL-FR-34).
    const QUERY: &str = "how-does-teardown-work";
    const ADDRESS: &str = "https://example.invalid/teardown-page";
    const PAGE: &str = "The page text nobody but the model should ever read.";
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::answer("Teardown runs last.")
            .and_native("openrouter:web_search", QUERY, "1. Teardown")
            .and_native("openrouter:web_fetch", ADDRESS, PAGE)),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 2);
    // The answer alone: naming a tool, quoting a result, or listing anything the
    // agent consulted would each make this something other than what it is.
    assert_eq!(&threads[0].comments[1].body, "Teardown runs last.");

    // TLC-FR-26 / CVL-FR-28 / CVL-FR-34: the counts and the entry types are
    // there to be read, so they are asserted present as well as harmless — a
    // field deleted tomorrow turns this red rather than turning nothing red.
    let calling = wait_for_record(&terminal.id, "calling model");
    assert_eq!(require(&calling, "nativeTools"), "1");
    assert_eq!(require(&calling, "nativeToolTypes"), "openrouter:web_search");
    let answered = all_records()
        .into_iter()
        .filter(|record| record.message == "model answered")
        .find(|record| field(record, "turnId") == terminal.id && field(record, "modelCall") == "1")
        .expect("a record for the round the provider ran its tools on");
    assert_eq!(require(&answered, "nativeToolCalls"), "2");
    assert_eq!(
        provider_tool_records(&terminal.id)
            .iter()
            .map(|record| field(record, "tool"))
            .collect::<Vec<_>>(),
        vec!["openrouter:web_search", "openrouter:web_fetch"],
    );

    // Every record of this turn, under both domains the loop emits with.
    for domain in [logging::Domain::Ai, logging::Domain::Backend] {
        let records = TEST_BUFFER
            .query(
                &LogFilter {
                    domains: vec![domain],
                    ..Default::default()
                },
                None,
                logging::BUFFER_CAPACITY,
            )
            .expect("the buffer answers a domain filter")
            .records;
        assert!(!records.is_empty(), "the turn reported itself");
        for record in records {
            let rendered = serde_json::to_string(&record).expect("serialisable");
            for leaked in [QUERY, ADDRESS, PAGE] {
                assert!(
                    !rendered.contains(leaked),
                    "no record carries a query, an address, or any part of a page: {rendered}",
                );
            }
        }
    }
}

#[test]
fn provider_reported_blocks_become_one_call_per_tool_the_provider_ran() {
    // CVL-FR-31 / WST-FR-08 / WFT-FR-08: the extraction is what stands between
    // the provider's report and the model's next round, so it is pinned
    // directly. A block naming no tool is the model's own reasoning and is not
    // one of these; a call and a result reported as two blocks under one id are
    // the one call they were; and a block with no id still travels, since the
    // model asked for it and the result has to reach it.
    fn detail(
        tool_name: Option<&str>,
        arguments: Option<&str>,
        result: Option<&str>,
        id: Option<&str>,
    ) -> openrouter_rs::types::ReasoningDetail {
        let mut value = serde_json::json!({ "type": "reasoning.server_tool_call" });
        let object = value.as_object_mut().expect("an object");
        for (key, held) in [
            ("tool_name", tool_name),
            ("arguments", arguments),
            ("result", result),
            ("tool_call_id", id),
        ] {
            if let Some(held) = held {
                object.insert(key.into(), serde_json::json!(held));
            }
        }
        serde_json::from_value(value).expect("a reasoning block")
    }

    let details = vec![
        // The model's own reasoning: no tool, so nothing to carry.
        detail(None, None, None, None),
        // One tool reported as a call and then as its result, under one id.
        detail(
            Some("openrouter:web_search"),
            Some(r#"{"query":"a"}"#),
            None,
            Some("call_1"),
        ),
        detail(Some("openrouter:web_search"), None, Some("A"), Some("call_1")),
        // Another reported whole, and a third naming no id at all.
        detail(
            Some("openrouter:web_fetch"),
            Some(r#"{"url":"u"}"#),
            Some("PAGE"),
            Some("call_2"),
        ),
        detail(Some("openrouter:web_search"), Some("{}"), Some("B"), None),
        // A second tool reported under an id another tool already used: two
        // calls, not one, and the later one keeps an id of its own rather than
        // disappearing into the first.
        detail(
            Some("openrouter:web_fetch"),
            Some(r#"{"url":"v"}"#),
            Some("OTHER"),
            Some("call_1"),
        ),
        // A call the provider never reported a result for.
        detail(
            Some("openrouter:web_search"),
            Some(r#"{"query":"c"}"#),
            None,
            Some("call_3"),
        ),
    ];
    let calls = openrouter_bridge::native_calls_in(details.iter(), 7);
    assert_eq!(calls.len(), 5, "one entry per tool the provider actually ran");
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[0].arguments, r#"{"query":"a"}"#);
    assert_eq!(calls[0].result, "A", "the two blocks are the one call they were");
    assert_eq!(calls[1].name, "openrouter:web_fetch");
    assert_eq!(calls[1].result, "PAGE");
    assert_eq!(
        calls[2].id, "provider-tool-7-2",
        "a block naming no id still travels, under an id unique to this round",
    );
    assert_eq!(calls[2].result, "B");
    assert_eq!(calls[3].name, "openrouter:web_fetch");
    assert_eq!(calls[3].result, "OTHER", "a different tool is a different call");
    assert_ne!(calls[3].id, calls[0].id, "and it does not collide with the first");
    assert_eq!(calls[4].id, "call_3");
    assert_eq!(
        calls[4].result, "",
        "a call the provider reported no result for still travels, carrying none",
    );
    // Every id is distinct, which is the whole basis on which a result is
    // matched to the call it answers.
    let ids: std::collections::HashSet<&String> = calls.iter().map(|call| &call.id).collect();
    assert_eq!(ids.len(), calls.len());
}

#[test]
fn a_provider_call_with_no_result_is_recorded_as_one_that_produced_nothing() {
    // CVL-FR-34: a call the provider reported nothing for is the shape a
    // refused, exhausted, or unserved server tool takes. It is not the turn's
    // failure (CVL-FR-32) — the model read it and carried on — but it is
    // recorded at `WARN`, so it is found in a log rather than by reproducing
    // the turn.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Nothing came back.")
        .and_native("openrouter:web_fetch", "{}", ""))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let records = provider_tool_records(&terminal.id);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].message, "provider tool produced no result");
    assert_eq!(records[0].level, logging::LogLevel::Warn);
    assert_eq!(require(&records[0], "tool"), "openrouter:web_fetch");
    assert_eq!(require(&records[0], "resultChars"), "0");
    assert_eq!(require(&records[0], "emptyResult"), "true");
}

#[test]
fn provider_call_arguments_of_every_shape_are_measured_and_never_parsed() {
    // CVL-FR-31 / CVL-FR-34: what a model composed and a provider already acted
    // on is not this application's to validate. Nothing parses it, nothing
    // travels on it, and the record says how long it was and no more — so a
    // string that is not JSON at all fails nothing.
    // The expected count is a literal rather than a recomputation of the
    // expression under test: `arguments.chars().count()` would agree with a
    // regression to `len()` on every ASCII case here. The last entry is the one
    // that tells them apart — 17 characters, 22 bytes.
    for (arguments, expected) in [
        ("not json at all", "15"),
        ("123", "3"),
        ("null", "4"),
        ("[1,2]", "5"),
        ("", "0"),
        ("{\"query\":\"a\"}", "13"),
        ("{\"query\":\"поиск\"}", "17"),
    ] {
        let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered.").and_native(
            "openrouter:web_search",
            arguments,
            "A",
        ))]);
        h.mount();
        h.create_agent("arch", "");
        let (terminal, _) = run_one(&h, "arch");
        assert_eq!(
            terminal.state,
            AgentTurnState::Delivered,
            "arguments {arguments:?} must not fail a turn",
        );
        let records = provider_tool_records(&terminal.id);
        assert_eq!(records.len(), 1, "for arguments {arguments:?}");
        assert_eq!(
            require(&records[0], "argumentChars"),
            expected,
            "for arguments {arguments:?}",
        );
    }
}

#[test]
fn provider_traffic_without_prose_is_an_empty_reply() {
    // CVL-FR-13 / CVL-FR-31: OpenRouter gives the model every result inside the
    // call that ran the tools, so a reply that searched and then said nothing
    // has said nothing. There is no round that follows in which it would say
    // what it made of the result — calling again would carry an unchanged
    // exchange and ask the same question twice — so it is empty on the terms
    // every other contentless reply is.
    // Exactly three, which is what one logical call is allowed: a longer script
    // would leave replies unconsumed and misstate what the test drives.
    let h = Harness::scripted(
        (0..3)
            .map(|index| {
                Ok(ScriptedReply::native(
                    "openrouter:web_search",
                    &format!("{{\"query\":\"q{index}\"}}"),
                    "A",
                ))
            })
            .collect(),
    );
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(terminal.failure.as_deref(), Some("empty_reply"));
    // CVL-FR-19: one logical call, attempted three times before the turn was
    // failed for carrying nothing to deliver.
    assert_eq!(h.seam.call_count(), 3);
    let exchanges = h.seam.exchanges();
    assert!(
        exchanges.iter().all(|exchange| exchange.len() == 1),
        "every attempt carried the input alone (CVL-FR-20)",
    );
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 1, "no line for a turn that never answered");

    // CVL-FR-34: the searches still happened, so the turn does not read as one
    // that never reached the web — one record per attempt, and no activity from
    // a reply the turn discarded.
    assert_eq!(provider_tool_records(&terminal.id).len(), 3);
    let events = h.events.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events
            .iter()
            .filter(|event| event.id == terminal.id)
            .all(|event| event.active_tool_calls.is_empty()),
        "no discarded reply's call was ever active",
    );
}

#[test]
fn prose_carried_alongside_provider_traffic_is_the_turn_s_answer() {
    // CVL-FR-31 / CVL-FR-12: prose beside provider tool calls is the ordinary
    // OpenRouter shape — the provider runs its tools and the model composes the
    // reply, all inside one call — so the turn ends here.
    //
    // Deliberately unlike a reply carrying prose beside a *portable* tool call,
    // which is not delivered below the bound: that call has not run yet, and
    // this one has.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Found it.").and_native(
        "openrouter:web_search",
        "{\"query\":\"a\"}",
        "A",
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 1, "the reply answered; no further round");
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 2);
    assert_eq!(threads[0].comments[1].body, "Found it.");
}

#[test]
fn a_provider_supplied_call_id_is_what_the_record_names() {
    // CVL-FR-34: OpenRouter names the call, and that name is what a record
    // carries, so a record can be matched to a provider's own trace. Ids are
    // unique within the reply that carried them, and a call the provider named
    // no id for still gets one — a record with no id would be a record nobody
    // can correlate.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Four of them.")
        .and_native_with_id("call_abc", "openrouter:web_search", "{}", "A")
        .and_native("openrouter:web_fetch", "{}", "B"))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let ids: Vec<String> = provider_tool_records(&terminal.id)
        .iter()
        .map(|record| field(record, "callId"))
        .collect();
    assert_eq!(ids.len(), 2, "one record per call the provider ran");
    assert_eq!(ids[0], "call_abc", "the provider's own id travels");
    assert!(!ids[1].is_empty(), "and one is derived where it named none");
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "no two calls share an id: {ids:?}");
}

#[test]
fn a_retried_call_repeats_an_exchange_that_carries_no_provider_traffic() {
    // CVL-FR-20 / CVL-FR-31: a retry repeats the failed call against the
    // exchange byte-for-byte as it stood before it. Provider traffic never
    // entered that exchange, so there is nothing there for a second attempt to
    // carry twice.
    let h = Harness::scripted(vec![
        Err(FAIL_UNREACHABLE),
        Ok(ScriptedReply::answer("Answered on the second attempt.")
            .and_native("openrouter:web_search", "{}", "A")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2, "one failed attempt and one answer");
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges[0], exchanges[1], "the same exchange, twice");
    for exchange in &exchanges {
        // Exactly the input. A partial revert that appended a provider result
        // without the assistant call that names it would be a malformed request
        // that both helpers below are blind to; the length is not.
        assert_eq!(exchange.len(), 1);
        let (calls, results) = native_traffic(exchange);
        assert!(calls.is_empty() && results.is_empty());
        assert!(!mentions_a_provider_entry(exchange));
    }
    // And the one call the answering attempt reported is recorded once.
    assert_eq!(provider_tool_records(&terminal.id).len(), 1);
}

#[test]
fn cancelling_after_a_reply_that_reported_provider_traffic_appends_nothing() {
    // CVL-FR-25 / AGC-FR-20: cancellation reaches the loop wherever it stands,
    // and a round whose reply reported provider traffic is no exception — the
    // turn ends `cancelled`, nothing is appended, and no further call is made.
    // The second round exists because the reply also asked for a tool of this
    // application's; the provider's own traffic finishes where it is reported
    // (CVL-FR-31) and would owe no round of its own.
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(Mutex::new(0usize));
    let h = Harness::with_patient_seam(Box::new(NativeThenGated {
        gate: gate.clone(),
        calls: Arc::clone(&calls),
    }));
    h.mount();
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::of(&thread),
            &thread.comments[0].id,
        )
        .expect("dispatch");

    // Wait until the second round — the one after the provider's traffic — is in
    // flight, then cancel it there.
    for _ in 0..600 {
        if *calls.lock().unwrap_or_else(|e| e.into_inner()) >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let handle = h.app.handle().clone();
    let _ = cancel_impl(&handle, &h.turns(), &h.progress(), &turn.id);
    {
        let (lock, cvar) = &*gate;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cvar.notify_all();
    }
    h.settle();
    let terminal = wait_for_terminal(&h, &turn.id);

    assert_eq!(terminal.state, AgentTurnState::Cancelled);
    assert_eq!(
        h.threads("spec.md")[0].comments.len(),
        1,
        "a cancelled turn appends nothing, whatever the provider had already run",
    );
    assert_eq!(
        *calls.lock().unwrap_or_else(|e| e.into_inner()),
        2,
        "and no round follows the one that was cancelled",
    );
}

/// A seam whose first round reports provider traffic beside a tool call and
/// whose second blocks, so a cancellation can land after the traffic and before
/// any answer. The tool call is what makes a second round happen at all: the
/// provider's own traffic is finished when it is reported (CVL-FR-31).
struct NativeThenGated {
    gate: Arc<(Mutex<bool>, Condvar)>,
    calls: Arc<Mutex<usize>>,
}

impl CompletionSeam for NativeThenGated {
    fn complete(
        &self,
        _request: &AgentRequest,
        exchange: &[rig::completion::Message],
        _endpoint: &AiApiCall,
        _timeout: Duration,
    ) -> Result<ModelReply, CallFailure> {
        *self.calls.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        if exchange.len() == 1 {
            return Ok(ModelReply {
                text: String::new(),
                tool_calls: vec![rig::completion::message::ToolCall::new(
                    "call-0".to_string(),
                    rig::completion::message::ToolFunction {
                        name: crate::tools::skill_list::NAME.into(),
                        arguments: serde_json::json!({}),
                    },
                )],
                native_calls: vec![NativeToolCall {
                    id: "call_1".into(),
                    name: "openrouter:web_search".into(),
                    arguments: "{}".into(),
                    result: "A".into(),
                }],
                native_usage: None,
            native_entries_dropped: 0,
            reply_repairs: Default::default(),
            served_by: None,
            prompt_tokens: None,
            output_tokens: None,
                input_tokens: None,
            });
        }
        let (lock, cvar) = &*self.gate;
        let mut released = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !*released {
            released = cvar.wait(released).unwrap_or_else(|e| e.into_inner());
        }
        Ok(ModelReply {
            text: "Should never be delivered.".into(),
            tool_calls: Vec::new(),
            native_calls: Vec::new(),
            native_usage: None,
            native_entries_dropped: 0,
            reply_repairs: Default::default(),
            served_by: None,
            prompt_tokens: None,
            output_tokens: None,
            input_tokens: None,
        })
    }
}
