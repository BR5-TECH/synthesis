//! What the provider ran for itself, and how the turn reports it.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// CVL-FR-34, CVL-FR-12 / CVL-FR-13, CVL-FR-20 — what the provider ran for itself (CVL-FR-31)
// ---------------------------------------------------------------------------




#[test]
fn the_helpers_that_look_for_provider_traffic_find_it_when_it_is_there() {
    // Five tests below assert only that these two helpers find *nothing*. An
    // assertion of absence is worth what the finder is worth, so the finder is
    // shown working against the exact shape it exists to catch: the assistant
    // tool call and the tool result the loop used to build, which is the shape
    // a provider refuses.
    use rig::completion::message::AssistantContent;
    let exchange = vec![
        rig::completion::Message::user("the input"),
        rig::completion::Message::Assistant {
            id: None,
            content: rig::OneOrMany::one(AssistantContent::tool_call(
                "call_abc".to_string(),
                "openrouter:web_search".to_string(),
                serde_json::json!({ "query": "a" }),
            )),
        },
        rig::completion::Message::tool_result("call_abc".to_string(), "A result".to_string()),
    ];

    let (calls, results) = native_traffic(&exchange);
    assert_eq!(
        calls,
        vec![(
            "openrouter:web_search".to_string(),
            serde_json::json!({ "query": "a" }).to_string(),
        )],
    );
    assert_eq!(results, vec![("call_abc".to_string(), "A result".to_string())]);
    assert!(mentions_a_provider_entry(&exchange));

    // And it says nothing of an exchange that carries only the input.
    let clean = vec![rig::completion::Message::user("the input")];
    assert_eq!(native_traffic(&clean), (Vec::new(), Vec::new()));
    assert!(!mentions_a_provider_entry(&clean));
}

#[test]
fn a_provider_run_search_and_fetch_are_finished_before_the_reply_that_reports_them() {
    // CVL-FR-31, CVL-FR-34, CVL-FR-12 / WST-FR-07, WST-FR-08, WST-FR-14 / WFT-FR-07, WFT-FR-08, WFT-FR-09, WFT-FR-14: OpenRouter executed both inside the one
    // model call, gave both results to the model, and returned the reply the
    // model composed with them already read. So the loop dispatches neither,
    // appends neither, and owes the turn no further round — and the exchange the
    // next request would carry holds nothing of either.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("The current release settles this.")
        .and_native(
            "openrouter:web_search",
            r#"{"query":"rig portable tools"}"#,
            "1. Rig docs — https://example.invalid/rig — portable tools are…",
        )
        .and_native(
            "openrouter:web_fetch",
            r#"{"url":"https://example.invalid/rig"}"#,
            "Rig — the whole page text, as the provider extracted it.",
        ))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 1, "one call carried both and the answer");

    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges[0].len(), 1, "the call carries the input alone");
    let (calls, results) = native_traffic(&exchanges[0]);
    assert!(calls.is_empty() && results.is_empty());

    // Nothing of this application's ran for either: the provider had already
    // carried them out (CVL-FR-31).
    assert!(
        !all_records()
            .into_iter()
            .any(|record| field(&record, "turnId") == terminal.id
                && record.message == "agent tool call completed"),
        "no tool of this application was dispatched for provider traffic",
    );

    // CVL-FR-34: each is recorded, named, and measured — in the order the
    // provider reported them.
    let records = provider_tool_records(&terminal.id);
    assert_eq!(
        records
            .iter()
            .map(|record| field(record, "tool"))
            .collect::<Vec<_>>(),
        vec!["openrouter:web_search", "openrouter:web_fetch"],
    );
    assert_eq!(require(&records[0], "callIndex"), "0");
    assert_eq!(require(&records[1], "callIndex"), "1");
    // Literal counts rather than a recomputation of the expression under test:
    // a count derived the same way the production code derives it cannot fail.
    // The search result carries an em dash and an ellipsis, so the literal also
    // pins characters rather than bytes.
    assert_eq!(require(&records[0], "argumentChars"), "30");
    assert_eq!(require(&records[0], "resultChars"), "63", "63 characters, 69 bytes");
    assert_eq!(require(&records[0], "emptyResult"), "false");
    assert_eq!(require(&records[1], "argumentChars"), "37");
    assert_eq!(require(&records[1], "resultChars"), "56", "56 characters, 58 bytes");
    assert_eq!(require(&records[1], "emptyResult"), "false");
    // The correlation fields every record of this turn carries, so a record can
    // be tied to the round and the endpoint that produced it.
    for record in &records {
        assert_eq!(require(record, "modelCall"), "1");
        assert_eq!(require(record, "provider"), "openrouter");
        assert!(!field(record, "agentId").is_empty());
    }

    // CVL-FR-26: the comment carries the answer alone — no call, no result.
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments.len(), 2);
    assert_eq!(threads[0].comments[1].body, "The current release settles this.");
}

#[test]
fn several_provider_run_calls_in_one_reply_cost_the_turn_no_round_of_its_own() {
    // CVL-FR-31, CVL-FR-13, CVL-FR-20 / WST-FR-11 / WFT-FR-11: several searches and several fetches,
    // mixed, all carried inside one model call — which is the whole of what the
    // provider's own budget bounds. The turn spends one logical call on all of
    // them and every one of them is reported and recorded.
    let mut reply = ScriptedReply::answer("Six of them, then an answer.");
    for query in ["a", "b", "c"] {
        reply = reply.and_native(
            "openrouter:web_search",
            &format!("{{\"query\":\"{query}\"}}"),
            "A result",
        );
    }
    for url in ["d", "e", "f"] {
        reply = reply.and_native(
            "openrouter:web_fetch",
            &format!("{{\"url\":\"{url}\"}}"),
            "A page",
        );
    }
    let h = Harness::scripted(vec![Ok(reply.counting(Some(6), Some(6), Some(3)))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 1, "one round carried all six");
    assert_eq!(provider_tool_records(&terminal.id).len(), 6);

    // CVL-FR-34: the provider's own counts, beside the calls it reported.
    let answered = all_records()
        .into_iter()
        .find(|record| {
            record.message == "model answered" && field(record, "turnId") == terminal.id
        })
        .expect("the round that answered is recorded");
    assert_eq!(require(&answered, "nativeToolCalls"), "6");
    assert_eq!(require(&answered, "nativeToolCallsRequested"), "6");
    assert_eq!(require(&answered, "nativeToolCallsExecuted"), "6");
    assert_eq!(require(&answered, "nativeWebSearches"), "3");

    // And the record of the counts themselves, which is the quieter of the two
    // branches: the provider ran everything it was asked for.
    let usage = all_records()
        .into_iter()
        .find(|record| {
            field(record, "turnId") == terminal.id
                && record.message == "provider tool usage reported"
        })
        .expect("the counts are recorded");
    assert_eq!(usage.level, logging::LogLevel::Debug);
    assert_eq!(require(&usage, "reportedCalls"), "6");
    assert_eq!(require(&usage, "toolCallsRequested"), "6");
    assert_eq!(require(&usage, "toolCallsExecuted"), "6");
    assert_eq!(require(&usage, "webSearchRequests"), "3");
    assert!(
        !all_records().into_iter().any(|record| {
            field(&record, "turnId") == terminal.id
                && record.message == "provider did not execute every tool call it was asked for"
        }),
        "the two branches are exclusive",
    );
}

#[test]
fn a_call_the_provider_did_not_execute_is_recorded_as_such() {
    // CVL-FR-34: a request the provider counted and did not carry out leaves no
    // call to report, so the counts are the only place it appears. It is worth
    // a `WARN`: it is the shape an exhausted or refused server tool takes, and
    // nothing else in the turn says it happened.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered from memory.")
        .and_native("openrouter:web_search", "{\"query\":\"a\"}", "A")
        .counting(Some(3), Some(1), Some(3)))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let record = all_records()
        .into_iter()
        .find(|record| {
            field(record, "turnId") == terminal.id
                && record.message == "provider did not execute every tool call it was asked for"
        })
        .expect("the shortfall is recorded");
    assert_eq!(record.level, logging::LogLevel::Warn);
    assert_eq!(require(&record, "toolCallsRequested"), "3");
    assert_eq!(require(&record, "toolCallsExecuted"), "1");
    assert_eq!(require(&record, "reportedCalls"), "1");
    assert!(
        !all_records().into_iter().any(|record| {
            field(&record, "turnId") == terminal.id
                && record.message == "provider tool usage reported"
        }),
        "the two branches are exclusive",
    );
}

#[test]
fn a_call_the_provider_counted_and_never_ran_is_recorded_with_no_call_to_report() {
    // CVL-FR-34, the case the counts exist for: the provider counted two calls
    // and executed none, so it reported no call at all. Nothing else in the turn
    // says a search was ever attempted — a record keyed on there being a call to
    // report would be silent here, which is the one place it must not be.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered from memory.")
        .counting(Some(2), Some(0), Some(2)))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert!(
        provider_tool_records(&terminal.id).is_empty(),
        "the provider reported no call, so there is none to record",
    );
    let record = all_records()
        .into_iter()
        .find(|record| {
            field(record, "turnId") == terminal.id
                && record.message == "provider did not execute every tool call it was asked for"
        })
        .expect("the counts are recorded even with no call beside them");
    assert_eq!(record.level, logging::LogLevel::Warn);
    assert_eq!(require(&record, "toolCallsRequested"), "2");
    assert_eq!(require(&record, "toolCallsExecuted"), "0");
    assert_eq!(require(&record, "reportedCalls"), "0");
}

#[test]
fn one_reply_records_a_search_at_debug_and_an_empty_fetch_at_warn() {
    // CVL-FR-34: the two levels, side by side in one reply, so the
    // rule that decides between them is pinned rather than each branch being
    // seen alone. The fetch's result is whitespace and nothing else — the one
    // case where "the provider produced nothing" and "the result is zero
    // characters long" disagree, and the record must say both truthfully.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Teardown runs last.")
        .and_native("openrouter:web_search", "{\"q\":\"a\"}", "1. Teardown — a result")
        .and_native("openrouter:web_fetch", "{\"url\":\"u\"}", "   \n\t "))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let records = provider_tool_records(&terminal.id);
    assert_eq!(records.len(), 2);

    assert_eq!(records[0].message, "provider ran its own tool");
    assert_eq!(records[0].level, logging::LogLevel::Debug);
    assert_eq!(require(&records[0], "tool"), "openrouter:web_search");
    assert_eq!(require(&records[0], "callIndex"), "0");
    assert!(!require(&records[0], "callId").is_empty());
    assert_eq!(require(&records[0], "argumentChars"), "9");
    assert_eq!(require(&records[0], "resultChars"), "22", "an em dash is one character");
    assert_eq!(require(&records[0], "emptyResult"), "false");

    assert_eq!(records[1].message, "provider tool produced no result");
    assert_eq!(records[1].level, logging::LogLevel::Warn);
    assert_eq!(require(&records[1], "tool"), "openrouter:web_fetch");
    assert_eq!(require(&records[1], "callIndex"), "1");
    assert!(!require(&records[1], "callId").is_empty());
    assert_eq!(
        require(&records[1], "emptyResult"),
        "true",
        "whitespace alone is nothing the model can read",
    );
    assert_eq!(
        require(&records[1], "resultChars"),
        "6",
        "and the measurement is of what arrived, not of what is left after trimming",
    );
}

#[test]
fn counts_the_provider_only_partly_reported_are_named_as_far_as_they_go() {
    // CVL-FR-34: a provider that says how many calls were asked for and not how
    // many ran has reported no shortfall — only an unknown. The record says what
    // it was told and takes the quieter level, because a `WARN` here would fire
    // on every provider that counts one field of the two.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered.")
        .and_native("openrouter:web_search", "{}", "A")
        .counting(Some(3), None, None))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let record = all_records()
        .into_iter()
        .find(|record| {
            field(record, "turnId") == terminal.id
                && record.message == "provider tool usage reported"
        })
        .expect("what the provider did report is recorded");
    assert_eq!(record.level, logging::LogLevel::Debug);
    assert_eq!(require(&record, "toolCallsRequested"), "3");
    assert_eq!(
        require(&record, "toolCallsExecuted"),
        "null",
        "not reported, which is not the same as zero",
    );
    assert_eq!(require(&record, "webSearchRequests"), "null");
}

#[test]
fn a_provider_that_counts_nothing_names_no_count_rather_than_zero() {
    // CVL-FR-34, second clause: a confident zero would say the provider ran
    // nothing, which is the one thing the record does not know.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered.").and_native(
        "openrouter:web_search",
        "{}",
        "A",
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert!(
        !all_records().into_iter().any(|record| {
            field(&record, "turnId") == terminal.id
                && (record.message == "provider tool usage reported"
                    || record.message
                        == "provider did not execute every tool call it was asked for")
        }),
        "a provider that counted nothing contributes no record of counts",
    );
    // And the round's own record says "not reported" rather than zero.
    let answered = all_records()
        .into_iter()
        .find(|record| {
            record.message == "model answered" && field(record, "turnId") == terminal.id
        })
        .expect("the round that answered is recorded");
    assert_eq!(require(&answered, "nativeToolCalls"), "1");
    assert_eq!(require(&answered, "nativeToolCallsRequested"), "null");
    assert_eq!(require(&answered, "nativeToolCallsExecuted"), "null");
    assert_eq!(require(&answered, "nativeWebSearches"), "null");
}

#[test]
fn a_rejected_reply_records_what_the_provider_ran_and_reports_none_of_it() {
    // CVL-FR-34 / CVL-FR-13: a reply the loop discards for carrying nothing had
    // a provider run a search all the same, and a turn that spent its attempts
    // on such replies must not look like a turn that never reached the web. The
    // call is *recorded* once per attempt and *reported* never — no surface
    // shows activity from a reply the turn threw away.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::native("openrouter:web_search", "{\"q\":\"a\"}", "A")),
        Ok(ScriptedReply::answer("Answered on the second attempt.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2, "one discarded reply and one answer");
    let records = provider_tool_records(&terminal.id);
    assert_eq!(records.len(), 1, "recorded once, for the attempt that ran it");
    assert_eq!(require(&records[0], "tool"), "openrouter:web_search");

    let events = h.events.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events
            .iter()
            .filter(|event| event.id == terminal.id)
            .all(|event| event.active_tool_calls.is_empty()),
        "a discarded reply's call is never active anywhere",
    );
}

#[test]
fn derived_call_ids_stay_distinct_across_the_rounds_of_one_turn() {
    // CVL-FR-34: the id is what ties a record to the provider's own trace, so
    // two calls of one turn sharing one id would make two records nobody can
    // tell apart. A provider that names none of them is the case that tests it:
    // every id here is derived.
    let h = Harness::scripted(vec![
        Ok(
            ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))
                .and_native("openrouter:web_search", "{}", "A")
                .and_native("openrouter:web_fetch", "{}", "B"),
        ),
        Ok(ScriptedReply::answer("Four of them.")
            .and_native("openrouter:web_search", "{}", "C")
            .and_native("openrouter:web_fetch", "{}", "D")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let records = provider_tool_records(&terminal.id);
    assert_eq!(records.len(), 4, "two rounds of two");
    assert_eq!(
        records
            .iter()
            .map(|record| require(record, "modelCall"))
            .collect::<Vec<_>>(),
        vec!["1", "1", "2", "2"],
        "each record names the round it belongs to",
    );
    let ids: Vec<String> = records
        .iter()
        .map(|record| require(record, "callId"))
        .collect();
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "no two calls of one turn share a derived id: {ids:?}",
    );
}

#[test]
fn a_reply_at_the_model_call_bound_still_records_what_the_provider_ran() {
    // CVL-FR-13 / CVL-FR-34: at the bound the turn delivers whatever prose the
    // reply carried and dispatches no tool it asked for. What the provider had
    // *already* run is not undispatched work — it happened — so it is recorded
    // there exactly as on any other round.
    let mut replies: Vec<_> = (0..7)
        .map(|_| Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))))
        .collect();
    replies.push(Ok(ScriptedReply::calls(
        "read_file",
        serde_json::json!({ "path": "spec.md" }),
    )
    .with_text("What I had concluded.")
    .and_native("openrouter:web_search", "{}", "A")));
    let h = Harness::scripted(replies);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let threads = h.threads("spec.md");
    assert_eq!(threads[0].comments[1].body, "What I had concluded.");
    let bound = wait_for_record(&terminal.id, "agent turn reached its model-call bound");
    assert_eq!(require(&bound, "undispatchedToolCalls"), "1");
    let records = provider_tool_records(&terminal.id);
    assert_eq!(records.len(), 1, "the search the provider had already run");
    assert_eq!(require(&records[0], "modelCall"), "8");
}

#[test]
fn provider_counts_are_reported_only_where_the_provider_reported_something() {
    // CVL-FR-34, at the seam the scripted harness overwrites: `ScriptedReply`
    // sets `native_usage` wholesale, so the mapping from the provider's own wire
    // shape is unreachable from a scripted turn and is pinned here directly.
    assert_eq!(NativeToolUsage::reported(None, None, None), None);
    assert_eq!(
        NativeToolUsage::reported(None, None, Some(1)),
        Some(NativeToolUsage {
            requested: None,
            executed: None,
            web_searches: Some(1),
        }),
        "one count reported is a report, and the other two stay unreported",
    );
    assert_eq!(
        NativeToolUsage::reported(Some(0), Some(0), Some(0)),
        Some(NativeToolUsage {
            requested: Some(0),
            executed: Some(0),
            web_searches: Some(0),
        }),
        "a reported zero is not the same as nothing reported",
    );

    // And the wire shape the mapping reads, which is the SDK's to declare and
    // the provider's to change. Pinned here because it is the whole of what
    // stands between a live response and these counts: the SDK carries the block
    // under `server_tool_use_details`, which is what a live chat completion
    // returns, and declares no alias — so this test is what will say so on the
    // day either of them moves.
    fn details(
        usage: &str,
    ) -> Option<openrouter_rs::types::completion::ServerToolUseDetails> {
        serde_json::from_str::<openrouter_rs::types::completion::ResponseUsage>(usage)
            .expect("a usage blob parses")
            .server_tool_use_details
    }
    let base = r#""prompt_tokens":1,"completion_tokens":2,"total_tokens":3"#;
    assert!(details(&format!("{{{base}}}")).is_none(), "absent");
    let full = details(&format!(
        "{{{base},\"server_tool_use_details\":{{\"tool_calls_requested\":3,\"tool_calls_executed\":1,\"web_search_requests\":2}}}}"
    ))
    .expect("a populated block maps");
    assert_eq!(full.tool_calls_requested, Some(3));
    assert_eq!(full.tool_calls_executed, Some(1));
    assert_eq!(full.web_search_requests, Some(2));
    // The spelling the server-tools guide shows for its own `usage` block, which
    // belongs to the Messages API rather than to this one. The SDK does not
    // accept it, and a chat completion does not send it.
    assert!(
        details(&format!(
            "{{{base},\"server_tool_use\":{{\"web_search_requests\":2}}}}"
        ))
        .is_none(),
        "the SDK declares no alias for this spelling",
    );
}

#[test]
fn a_provider_tool_named_something_unexpected_is_recorded_as_the_provider_named_it() {
    // The name is the provider's, not this application's: nothing here declares
    // it, and `native_calls_in` carries whatever the wire named (WST-FR-07,
    // WFT-FR-07). A build that quietly rewrote it to one of the two entry types
    // would make a record say a search ran when the provider said otherwise, so
    // the pass-through is pinned rather than assumed.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered.").and_native(
        "openrouter:something_else",
        "{}",
        "A",
    ))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let records = provider_tool_records(&terminal.id);
    assert_eq!(records.len(), 1);
    assert_eq!(require(&records[0], "tool"), "openrouter:something_else");
}

#[test]
fn a_structured_provider_refusal_keeps_the_code_the_id_and_the_reason() {
    // CVL-FR-35: this client answers a refused request with a structured error,
    // and reducing it to a string only to scan the string for a status throws
    // away the three fields that make a refusal actionable. Pinned at the
    // mapping, the SDK's error type being the thing under test.
    use openrouter_rs::error::{ApiErrorContext, ApiErrorKind, OpenRouterError};

    let api = |status: u16, message: &str| {
        OpenRouterError::Api(Box::new(ApiErrorContext {
            status: http::StatusCode::from_u16(status).unwrap(),
            api_code: Some(status as i64),
            message: message.to_string(),
            request_id: Some("req_abc123".to_string()),
            metadata: None,
            kind: ApiErrorKind::Generic,
        }))
    };

    let refused = openrouter_failure(&api(
        400,
        "Item 'rs_1' of type 'reasoning' was provided without its required following item.",
    ));
    assert_eq!(refused.failure, FAIL_UNREACHABLE);
    assert_eq!(refused.class, class::HTTP_STATUS);
    assert_eq!(refused.status, Some(400));
    assert_eq!(refused.provider_code, Some(400));
    assert_eq!(refused.provider_request_id.as_deref(), Some("req_abc123"));
    assert_eq!(
        refused.provider_message.as_deref(),
        Some("Item 'rs_1' of type 'reasoning' was provided without its required following item."),
        "the reason the provider gave, which is the whole point of carrying it",
    );

    // AAP-FR-05: a refused key stays distinguishable from an unwell endpoint.
    for status in [401, 403] {
        let rejected = openrouter_failure(&api(status, "no"));
        assert_eq!(rejected.failure, FAIL_REJECTED);
        assert_eq!(rejected.class, class::AUTH);
    }
    assert_eq!(openrouter_failure(&api(503, "busy")).class, class::HTTP_STATUS);

    // CVL-FR-35: a provider free to answer with a page of text does not get to
    // put a page of text in a record, and the cut is by character.
    let long = "п".repeat(PROVIDER_MESSAGE_LIMIT + 50);
    let bounded = openrouter_failure(&api(400, &long));
    let carried = bounded.provider_message.expect("a message is carried");
    assert_eq!(carried.chars().count(), PROVIDER_MESSAGE_LIMIT + 1, "and one mark");
    assert!(carried.ends_with('…'));

    // CVL-FR-21: a *transport* failure carries none of it. Its string is the
    // driver's, and a driver renders the request it made.
    let transport = openrouter_failure(&OpenRouterError::HttpRequest(
        openrouter_rs::error::HttpRequestError::new("connection refused to https://x/y?key=SECRET"),
    ));
    assert_eq!(transport.provider_message, None);
    assert_eq!(transport.provider_code, None);
    assert_eq!(transport.provider_request_id, None);
    assert_eq!(transport.class, class::CONNECT);
}

#[test]
fn a_search_the_provider_named_no_call_for_is_still_reported_as_activity() {
    // CVL-FR-33: a provider that serves search natively reports no call for it —
    // its reply carries reasoning blocks naming no tool — and the count is the
    // only thing that says a search happened. Reporting nothing there leaves a
    // conversation blank while its agent searches, which is the whole of what
    // this reporting is for.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Rust 1.97.1, per the blog.")
        .counting(None, None, Some(2)))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let events = h.events.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let active: Vec<Vec<&str>> = events
        .iter()
        .filter(|event| event.id == terminal.id)
        .map(|event| {
            event
                .active_tool_calls
                .iter()
                .map(|call| call.tool.as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        active,
        vec![
            Vec::<&str>::new(),
            vec!["openrouter:web_search"],
            vec!["openrouter:web_search", "openrouter:web_search"],
            vec!["openrouter:web_search"],
            Vec::<&str>::new(),
            Vec::<&str>::new(),
        ],
        "one report per search the provider counted, both active together",
    );

    // CVL-FR-34: and the record says the report came from the count rather than
    // from a call, so the two numbers are not read as disagreeing.
    let usage = all_records()
        .into_iter()
        .find(|record| {
            field(record, "turnId") == terminal.id
                && record.message == "provider tool usage reported"
        })
        .expect("the counts are recorded");
    assert_eq!(require(&usage, "webSearchRequests"), "2");
    assert_eq!(require(&usage, "reportedCalls"), "0");
    assert_eq!(require(&usage, "reportedFromCounts"), "true");
    assert!(
        provider_tool_records(&terminal.id).is_empty(),
        "there is no per-call record to write for a call the provider never named",
    );
}

#[test]
fn a_provider_that_named_its_calls_is_not_also_reported_from_its_counts() {
    // CVL-FR-33: the count is a stand-in for calls the provider did not name,
    // never an addition to the ones it did — a provider reporting both would
    // otherwise have every search reported twice.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered.")
        .and_native("openrouter:web_search", "{}", "A")
        .counting(Some(1), Some(1), Some(1)))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let events = h.events.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let reported = events
        .iter()
        .filter(|event| event.id == terminal.id)
        .map(|event| event.active_tool_calls.len())
        .max()
        .expect("the turn emitted events");
    assert_eq!(reported, 1, "one report for one call, not two");
    let usage = all_records()
        .into_iter()
        .find(|record| {
            field(record, "turnId") == terminal.id
                && record.message == "provider tool usage reported"
        })
        .expect("the counts are recorded");
    assert_eq!(require(&usage, "reportedFromCounts"), "false");
}

#[test]
fn a_count_beyond_what_the_provider_could_have_run_is_capped() {
    // CVL-FR-33: the number of events a turn emits is this application's to
    // decide. A provider's count is a field, and a field is not a promise.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Answered.")
        .counting(None, None, Some(MAX_COUNTED_NATIVE_REPORTS + 500)))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let events = h.events.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let peak = events
        .iter()
        .filter(|event| event.id == terminal.id)
        .map(|event| event.active_tool_calls.len())
        .max()
        .expect("the turn emitted events");
    assert_eq!(peak as u32, MAX_COUNTED_NATIVE_REPORTS);
    // And the count the provider actually gave is still recorded in full, so the
    // cap is on what is reported and never on what is known.
    let usage = all_records()
        .into_iter()
        .find(|record| {
            field(record, "turnId") == terminal.id
                && record.message == "provider tool usage reported"
        })
        .expect("the counts are recorded");
    assert_eq!(
        require(&usage, "webSearchRequests"),
        (MAX_COUNTED_NATIVE_REPORTS + 500).to_string(),
    );
}

#[test]
fn a_refusal_naming_a_server_tool_narrows_the_entries_and_a_plain_400_does_not() {
    // CVL-FR-36: the provider refuses a whole request when it cannot serve one
    // of the server tools that request offers, before the model sees anything,
    // so CVL-FR-32's graceful path never opens. Narrowing is what keeps one
    // unservable entry from costing a turn the capability that works — and it is
    // told from every other 400 by the provider's own message, because narrowing
    // on any 400 would strip an agent's tools over a malformed message.
    let refusal = |message: &str| CallFailure {
        failure: FAIL_UNREACHABLE,
        class: class::HTTP_STATUS,
        status: Some(400),
        provider_code: Some(400),
        provider_request_id: Some("req_1".into()),
        provider_message: Some(message.into()),
    };
    assert!(refuses_a_server_tool(&refusal("Server tool request failed")));
    assert!(
        refuses_a_server_tool(&refusal("SERVER TOOL request failed")),
        "the provider's capitalisation is not part of the contract",
    );
    assert!(
        !refuses_a_server_tool(&refusal("Invalid value for 'messages[2].content'")),
        "a malformed request is not a reason to take an agent's tools away",
    );
    assert!(
        !refuses_a_server_tool(&CallFailure {
            status: Some(500),
            ..refusal("Server tool request failed")
        }),
        "a provider that is unwell is not a provider that refuses an entry",
    );
    assert!(
        !refuses_a_server_tool(&CallFailure::unreachable(class::CONNECT)),
        "a transport failure names no message at all",
    );
}

#[test]
fn a_local_tool_and_provider_traffic_travel_one_reply() {
    // CVL-FR-31 / CVL-FR-12: a reply carrying both is the ordinary case — the
    // provider had already searched by the time the model asked this
    // application for a file. The file is dispatched and its result travels; the
    // search is already answered and nothing of it travels.
    let h = Harness::scripted(vec![
        Ok(
            ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))
                .and_native("openrouter:web_search", "{\"query\":\"a\"}", "A"),
        ),
        Ok(ScriptedReply::answer("Both read.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.call_count(), 2);
    let exchanges = h.seam.exchanges();
    assert_eq!(
        exchanges[1].len(),
        3,
        "the input, the assistant message, and the one tool result — and nothing else",
    );
    let (calls, results) = native_traffic(&exchanges[1]);
    assert!(
        calls.is_empty() && results.is_empty(),
        "the provider's own call is not replayed into the next request",
    );
    assert!(
        !mentions_a_provider_entry(&exchanges[1]),
        "and no part of it reaches the next request in any other shape",
    );
    // And the local tool ran as it always does, its result in the exchange.
    let local_results = exchanges[1]
        .iter()
        .filter(|message| matches!(
            message,
            rig::completion::Message::User { content }
                if content.iter().any(|part| matches!(
                    part,
                    rig::completion::message::UserContent::ToolResult(_)
                ))
        ))
        .count();
    assert_eq!(local_results, 1, "the file the model asked this application for");
}
