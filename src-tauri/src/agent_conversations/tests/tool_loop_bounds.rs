//! The tool loop's bounds, and what it refuses.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

#[test]
fn a_turn_runs_inside_an_agent_session_that_ends_with_it() {
    // CVL-FR-24, per FSA-FR-29 and RFT-FR-05.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "spec.md" }),
        )),
        Ok(ScriptedReply::answer("Read it.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    assert_eq!(h.agent_sessions(), 0, "no session before a turn runs");

    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    // The read actually went through the session's instance: it returned the
    // file rather than the no-project refusal a missing session produces.
    let result = h.seam.exchanges()[1]
        .iter()
        .flat_map(|message| match message {
            rig::completion::Message::User { content } => content.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .find_map(|part| match part {
            rig::completion::message::UserContent::ToolResult(result) => match result.content.first() {
                rig::completion::message::ToolResultContent::Text(text) => Some(text.text.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("a result came back");
    assert_eq!(
        result, "Some artifact source here.",
        "read_file read through the turn's own session",
    );

    // CVL-FR-24: the session ended with the turn, taking its temp directory.
    assert_eq!(
        h.agent_sessions(),
        0,
        "the session was closed when the turn terminated",
    );
}

#[test]
fn cancelling_mid_loop_discards_the_tool_result_and_appends_nothing() {
    // CVL-FR-25, AGC-FR-20.
    // The first call asks for a tool; the second blocks until this test releases
    // it. Gated rather than slept at, so the cancellation cannot lose a race
    // with delivery and the test cannot go red for timing alone.
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let h = Harness::with_patient_seam(Box::new(ToolThenGated { gate: gate.clone() }));
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

    // The tool ran, so the loop is past its first round and inside the second
    // call — which is where the cancellation has to land to be "mid-loop".
    wait_for_record(&turn.id, "agent tool call completed");
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
        "a cancelled turn appends nothing",
    );
    // Polled rather than read once: `cancel_impl` removes the turn from the live
    // set and publishes the terminal event itself, so both `settle` and
    // `wait_for_terminal` return before the turn's own thread has finished
    // unwinding. Anything asserted about that thread's effects has to wait for
    // the thread, not for the registry.
    wait_for_sessions(&h, 0);
}

#[test]
fn tool_use_is_invisible_to_every_surface() {
    // CVL-FR-33 / CVL-FR-27, AGC-FR-21, AGC-FR-24.
    let busy = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))),
        Ok(ScriptedReply::calls("search_skills", serde_json::json!({ "query": "a" }))),
        Ok(ScriptedReply::calls("search_specifications", serde_json::json!({ "query": "b" }))),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    busy.mount();
    busy.create_agent("arch", "");
    let (busy_turn, _) = run_one(&busy, "arch");

    let quiet = Harness::scripted(vec![Ok(ScriptedReply::answer("Done."))]);
    quiet.mount();
    quiet.create_agent("arch", "");
    let (quiet_turn, _) = run_one(&quiet, "arch");

    // AGC-FR-21: exactly one terminal event each, whatever happened in between.
    for (h, turn) in [(&busy, &busy_turn), (&quiet, &quiet_turn)] {
        let events = h.terminal.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            events.iter().filter(|t| t.id == turn.id).count(),
            1,
            "one terminal event per turn, and none for a tool call",
        );
    }

    // CVL-FR-27: the two turns are indistinguishable to the surface.
    assert_eq!(busy_turn.state, quiet_turn.state);
    assert_eq!(busy_turn.failure, quiet_turn.failure);

    // …and the delivered comment names no tool and quotes no result.
    let body = busy.threads("spec.md")[0].comments[1].body.clone();
    assert_eq!(body, "Done.");
    for tool in EXPECTED_TOOLS {
        assert!(!body.contains(tool), "the comment names {tool}");
    }
}

#[test]
fn every_prompt_tells_the_agent_it_may_gather_without_naming_a_tool() {
    // CVL-FR-04, AGC-FR-11, AGC-FR-07 / CVL-FR-05.
    for (name, template) in [
        ("comment.md", COMMENT_PROMPT_TEMPLATE),
        ("discuss-artifact.md", DISCUSS_ARTIFACT_PROMPT_TEMPLATE),
        ("discuss-draft.md", DISCUSS_DRAFT_PROMPT_TEMPLATE),
        ("discuss-note.md", DISCUSS_NOTE_PROMPT_TEMPLATE),
    ] {
        let lower = template.to_lowercase();
        // CVL-FR-05's first clause: the role the agent is taking, which is
        // where its title stands. Exactly once, on the same reasoning the
        // instructions placeholder is counted below — a template carrying two
        // would ship the literal text to a model, and one carrying none would
        // drop the role silently while every prompt still compiled.
        assert_eq!(
            template.matches(AGENT_TITLE_PLACEHOLDER).count(),
            1,
            "{name} does not name the agent's role exactly once",
        );
        assert!(
            lower.contains("look things up"),
            "{name} does not tell the agent it may gather material",
        );
        // CVL-FR-05's second clause: what the agent has been given, and that the
        // material section holds the whole of its material unless it says
        // otherwise (AGC-FR-11). Without this an agent fetches the document it
        // is already holding, spending a round of the loop to arrive back where
        // it started and carrying the same text twice thereafter.
        assert!(
            lower.contains("## what you are given"),
            "{name} does not tell the agent what it has been given",
        );
        assert!(
            lower.contains("**in full**"),
            "{name} does not say its material section holds its material in full",
        );
        assert!(
            lower.contains("would say so"),
            "{name} does not say a partial section says so itself",
        );
        assert!(
            lower.contains("do not need to go and fetch it"),
            "{name} does not say the material it holds need not be fetched",
        );
        assert!(
            lower.contains("specification"),
            "{name} does not name the specifications as the authority",
        );
        // CVL-FR-05's third clause: ask rather than guess, one thing at a time.
        assert!(
            lower.contains("ask the author") || lower.contains("ask them"),
            "{name} does not tell the agent it may ask the author",
        );

        // The claim that everything reachable is read-only is no longer true of
        // every tool a turn carries, and a prompt asserting it would contradict
        // the definition of the one that posts (CVL-FR-09).
        assert!(
            !lower.contains("everything you can reach is read-only"),
            "{name} still claims every tool it carries is read-only",
        );
        // CVL-FR-05: no tool name, no parameter name, no limit. What a tool is
        // called is its own fixed definition, and a prompt restating it would be
        // a second place to keep correct.
        //
        // The two proposal tools are chained in deliberately: three of these
        // prompts tell an agent it may offer a rewrite, so they are the ones with
        // any reason to name a tool — and leaving them out of this loop would
        // exempt the very files the check exists for.
        for tool in EXPECTED_TOOLS
            .iter()
            .copied()
            .chain([DRAFT_ONLY_TOOL, ARTIFACT_ONLY_TOOL, DISCUSSION_ONLY_TOOL])
        {
            assert!(!lower.contains(tool), "{name} names the tool {tool}");
        }
        for parameter in [
            "`query`",
            "`limit`",
            "`offset`",
            "`path`",
            "`ecosystem`",
            "`question`",
            "`options`",
            // `propose_draft_changes`' own three (PDC-FR-04).
            "`content`",
            "`rationale`",
        ] {
            assert!(
                !lower.contains(parameter),
                "{name} describes the parameter {parameter}",
            );
        }
    }
    // CVL-FR-MZTK: the draft discussion prompt states the working method the
    // agent keeps to. Asserted on the sections it commits to rather than on a
    // stray word, on the same reasoning the rewrite section is asserted on
    // below: a bare substring is satisfied by the words turning up in an
    // unrelated sentence.
    let draft = DISCUSS_DRAFT_PROMPT_TEMPLATE.to_lowercase();
    for section in [
        "## how to work",
        "## what is worth a change",
        "## text that is settled",
        "## when the current comment is a decision",
    ] {
        assert!(
            draft.contains(section),
            "discuss-draft.md does not state its working method: {section} is missing",
        );
    }
    // The rules themselves, not just the headings over them. A heading asserts
    // that a section exists; these assert that it says the thing it exists for.
    for (rule, missing) in [
        // The stopping condition, without which a method of four steps is worthless.
        ("do not start a new pass", "to stop once a pass is decided"),
        // The materiality bar (CVL-FR-MZTK), which is what a turn with nothing
        // substantial to add answers with instead of a wording change.
        ("alters what the other agent will build", "what makes a change worth offering"),
        // The settled-text rule, which is what the history rendering of
        // AGC-FR-RVQP exists to make obeyable.
        ("do not offer a change to settled text a second time", "to leave settled text alone"),
        // The first of the three decision cases, which is the one that ends a pass.
        ("offer no changes", "that a decision is not a request for more changes"),
    ] {
        assert!(
            draft.contains(rule),
            "discuss-draft.md does not tell the agent {missing}",
        );
    }
    // And the trigger it replaces: an unbounded invitation to improve the text
    // is true of any document for ever, so a prompt carrying both says nothing.
    assert!(
        !draft.contains("can be improved by adding something"),
        "discuss-draft.md still carries the unbounded invitation to propose",
    );
    // CVL-FR-MZTK: the other three prompts carry no working method of their own,
    // the method being the draft discussion's alone.
    for (name, template) in [
        ("comment.md", COMMENT_PROMPT_TEMPLATE),
        ("discuss-artifact.md", DISCUSS_ARTIFACT_PROMPT_TEMPLATE),
        ("discuss-note.md", DISCUSS_NOTE_PROMPT_TEMPLATE),
    ] {
        assert!(
            !template.to_lowercase().contains("## how to work"),
            "{name} states a working method that is the draft discussion's alone",
        );
    }

    // CVL-FR-05's third clause differs by origin kind, exactly as the tool set
    // does (CVL-FR-08). A passage under review is one remark, so `comment.md`
    // says to ask one thing; a discussion is where material is decided and an
    // agent reading a whole document finds several open points, so the three
    // discussion prompts say to gather everything unsettled and put it as one
    // set, offering the likely answers where there are any.
    let lower = COMMENT_PROMPT_TEMPLATE.to_lowercase();
    assert!(
        lower.contains("one thing at a time"),
        "comment.md does not tell the agent to ask one thing at a time",
    );
    assert!(
        !lower.contains("one set"),
        "comment.md tells an anchored comment turn to ask a set it has no tool for",
    );
    for (name, template) in [
        ("discuss-artifact.md", DISCUSS_ARTIFACT_PROMPT_TEMPLATE),
        ("discuss-draft.md", DISCUSS_DRAFT_PROMPT_TEMPLATE),
        ("discuss-note.md", DISCUSS_NOTE_PROMPT_TEMPLATE),
    ] {
        let lower = template.to_lowercase();
        assert!(
            lower.contains("one set"),
            "{name} does not tell the agent to gather everything unsettled into one set",
        );
        assert!(
            !lower.contains("one thing at a time"),
            "{name} still tells a discussion turn to ask one thing at a time",
        );
        // And it says the set carries the likely answers, which is what tells
        // the agent this way of asking is available at all.
        assert!(
            lower.contains("two or three likely answers"),
            "{name} does not say each question carries the likely answers",
        );
    }

    // CVL-FR-04, CVL-FR-08 / CVL-FR-03: each of the three prompts whose origin kinds carry a
    // proposal tool tells the agent it may offer the author a rewrite — the
    // artifact origins and the draft origins each having one of their own — and
    // none of them names which store the rewrite would be recorded in.
    for (name, template) in [
        ("comment.md", COMMENT_PROMPT_TEMPLATE),
        ("discuss-artifact.md", DISCUSS_ARTIFACT_PROMPT_TEMPLATE),
        ("discuss-draft.md", DISCUSS_DRAFT_PROMPT_TEMPLATE),
    ] {
        let lower = template.to_lowercase();
        assert!(
            lower.contains("## offering a rewrite") || lower.contains("## making proposals"),
            "{name} does not tell the agent it may offer the author a rewrite",
        );
    }
    // …and the one that carries neither says nothing about offering one, there
    // being nothing about a note to rewrite (CVL-FR-08).
    let note = DISCUSS_NOTE_PROMPT_TEMPLATE.to_lowercase();
    assert!(
        !note.contains("## offering a rewrite") && !note.contains("## making proposals"),
        "discuss-note.md offers a rewrite it has no tool to make",
    );

    // CVL-FR-05, CVL-FR-04, AGC-FR-11 / AGC-FR-07: the one prompt whose subject is *not* the thing it
    // is filed against has to say so, or an agent reads the scope attributes as
    // an offer of the file and answers a question nobody asked.
    assert!(
        DISCUSS_NOTE_PROMPT_TEMPLATE
            .to_lowercase()
            .contains("is **not** given to you"),
        "discuss-note.md does not say the file its note is filed against is withheld",
    );
}

#[test]
fn a_turn_reports_how_many_model_calls_it_made_and_no_part_of_its_exchange() {
    // AGC-FR-26, CVL-FR-26 / CVL-FR-28.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!({ "path": "spec.md" }),
        )),
        Ok(ScriptedReply::answer("A distinctive answer nobody should log.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let records = records_where("turnId", &terminal.id);
    let ended = find(&records, "agent turn ended");
    assert_eq!(ended.level, LogLevel::Info);
    assert_eq!(require(&ended, "agent"), "arch");
    assert_eq!(require(&ended, "state"), "delivered");
    assert_eq!(require(&ended, "modelCalls"), "2");

    // CVL-FR-28 / CVL-FR-26: no part of the exchange anywhere in the turn's
    // records — not the material it was given, not what a tool returned, and not
    // the answer.
    let haystack = records
        .iter()
        .map(|r| format!("{} {:?}", r.message, r.fields))
        .collect::<Vec<_>>()
        .join("\n");
    for leak in [
        "A distinctive answer nobody should log.",
        "Some artifact source here.",
        "@arch what do you think?",
        "spec.md",
    ] {
        assert!(
            !haystack.contains(leak),
            "a record carries {leak:?}:\n{haystack}",
        );
    }
}

#[test]
fn each_answered_call_names_the_upstream_that_carried_it_and_what_it_presented() {
    // CVL-FR-40 / CVL-FR-28, CVL-FR-40. A routing provider reports no cached and
    // uncached split, so the terminal record of CVL-FR-28, CVL-FR-23, CVL-FR-26, AGC-FR-26 stays empty for it —
    // which makes the per-call record the only evidence that the caching of
    // CVL-FR-23 is wired at all. What it must show is the routing holding
    // steady across a turn and what each round presented.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))
            .routed_by("Anthropic", 4_000)),
        Ok(ScriptedReply::answer("Settled.").routed_by("Anthropic", 4_600)),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let answered: Vec<LogRecord> = records_where("turnId", &terminal.id)
        .into_iter()
        .filter(|record| record.message == "model answered")
        .collect();
    assert_eq!(answered.len(), 2, "one record per answered call");
    for (round, record) in answered.iter().enumerate() {
        assert_eq!(
            require(record, "servedUpstream"),
            "Anthropic",
            "round {round}: the record names the upstream that carried the call",
        );
    }
    assert_eq!(require(&answered[0], "promptTokens"), "4000");
    assert_eq!(
        require(&answered[1], "promptTokens"),
        "4600",
        "the exchange grew, and the record says by how much",
    );

    // CVL-FR-28: a provider that names neither is recorded as not having named
    // them, rather than as having named nothing or zero.
    let silent = Harness::scripted(vec![Ok(ScriptedReply::answer("Settled."))]);
    silent.mount();
    silent.create_agent("quiet", "");
    let (terminal, _) = run_one(&silent, "quiet");
    let record = wait_for_record(&terminal.id, "model answered");
    assert_eq!(require(&record, "servedUpstream"), "not reported");
    assert_eq!(require(&record, "promptTokens"), "not reported");

    // CVL-FR-26 / AGC-FR-26: a service name and a count, and nothing drawn from
    // the exchange or from a credential.
    let haystack = records_where("turnId", &terminal.id)
        .iter()
        .map(|r| format!("{} {:?}", r.message, r.fields))
        .collect::<Vec<_>>()
        .join("\n");
    for leak in ["Settled.", "Some artifact source here."] {
        assert!(
            !haystack.contains(leak),
            "a record carried {leak:?}: {haystack}",
        );
    }
}

#[test]
fn a_turn_reports_what_its_input_cost_split_by_what_the_cache_served() {
    // CVL-FR-26, AGC-FR-26 / CVL-FR-28, CVL-FR-23. Whether caching is landing has to be
    // readable from the log rather than inferred from an invoice a month later,
    // so the terminal record carries the split — summed over the turn, because
    // one number per turn is what a reader compares against another turn.
    let h = Harness::scripted(vec![
        // The opening call: nothing cached yet, the whole prefix written.
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({})).reporting(0, 4_000)),
        // The second: the prefix served from the cache, only what the loop
        // added since read afresh.
        Ok(ScriptedReply::answer("Settled.").reporting(4_000, 120)),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let records = records_where("turnId", &terminal.id);
    let ended = find(&records, "agent turn ended");
    assert_eq!(require(&ended, "modelCalls"), "2");
    assert_eq!(require(&ended, "cachedInputTokens"), "4000");
    assert_eq!(require(&ended, "uncachedInputTokens"), "4120");
    // CVL-FR-28 / CVL-FR-26 / AGC-FR-26: counts, and nothing drawn from the
    // exchange or from a credential.
    let haystack = records
        .iter()
        .map(|r| format!("{} {:?}", r.message, r.fields))
        .collect::<Vec<_>>()
        .join("\n");
    for leak in ["Settled.", "Some artifact source here."] {
        assert!(
            !haystack.contains(leak),
            "a record carries {leak:?}:\n{haystack}",
        );
    }
}

#[test]
fn a_provider_that_reports_no_split_leaves_the_record_naming_none() {
    // CVL-FR-23, CVL-FR-26, AGC-FR-26 (second clause) / CVL-FR-28. A total reported as wholly uncached
    // would be a confident claim that caching had not landed, which is the one
    // thing the record must not say when the provider would not tell it. Absent
    // rather than zero.
    let h = Harness::scripted(vec![Ok(ScriptedReply::answer("Settled."))]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let ended = find(&records_where("turnId", &terminal.id), "agent turn ended");
    assert!(
        !ended.fields.contains_key("cachedInputTokens"),
        "a provider that said nothing is not recorded as having cached nothing: {:?}",
        ended.fields,
    );
    assert!(
        !ended.fields.contains_key("uncachedInputTokens"),
        "{:?}",
        ended.fields,
    );
    // The turn is otherwise unaffected: the answer still landed.
    assert_eq!(require(&ended, "state"), "delivered");
}

#[test]
fn a_reply_the_loop_rejected_is_not_counted_as_input_the_turn_paid_for() {
    // CVL-FR-28 / CVL-FR-19. A reply carrying neither usable prose nor a tool
    // request is retried as the *same* logical call, so counting what it
    // reported would bill the turn twice for one round of input.
    let h = Harness::scripted(vec![
        // Rejected: no prose, no tool call. Reports a large figure that must not
        // reach the record.
        Ok(ScriptedReply::answer("").reporting(0, 9_999)),
        Ok(ScriptedReply::answer("Settled.").reporting(0, 10)),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let ended = find(&records_where("turnId", &terminal.id), "agent turn ended");
    assert_eq!(
        require(&ended, "uncachedInputTokens"),
        "10",
        "only the attempt the loop accepted is counted",
    );
}

#[test]
fn a_turn_whose_providers_only_sometimes_reported_still_names_what_it_knows() {
    // CVL-FR-28. The latch is on *whether anything reported*, not on whether the
    // totals came out non-zero — a turn where one call reported and another did
    // not still knows something worth recording, and collapsing that into
    // "report only if a total is non-zero" would lose a turn that read entirely
    // from the cache.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))),
        Ok(ScriptedReply::answer("Settled.").reporting(4_000, 0)),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);

    let ended = find(&records_where("turnId", &terminal.id), "agent turn ended");
    assert_eq!(require(&ended, "cachedInputTokens"), "4000");
    assert_eq!(
        require(&ended, "uncachedInputTokens"),
        "0",
        "a turn served entirely from the cache still reports the split",
    );
}

#[test]
fn a_failed_turn_reports_the_split_like_any_other() {
    // CVL-FR-23, CVL-FR-26, AGC-FR-26 / CVL-FR-28: the split sits alongside the terminal state, and
    // `log_turn_ended` serves every route — a turn that spent tokens and then
    // failed spent them just the same, and is exactly the turn whose cost a
    // reader wants to see.
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("list_skills", serde_json::json!({})).reporting(0, 3_000)),
        Err(FAIL_REJECTED),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Failed);

    let ended = find(&records_where("turnId", &terminal.id), "agent turn ended");
    assert_eq!(require(&ended, "state"), "failed");
    assert_eq!(require(&ended, "uncachedInputTokens"), "3000");
}

#[test]
fn a_zero_valued_usage_report_reads_as_no_report_at_all() {
    // CVL-FR-28. `rig` reduces a missing usage report to a zero-valued `Usage`
    // rather than an absent one, so the mapping has to read all-zero as "would
    // not say" — a call that genuinely consumed no input tokens does not exist,
    // every call carrying at least the compiled prompt.
    let mut silent = rig::completion::Usage::new();
    silent.input_tokens = 0;
    silent.cached_input_tokens = 0;
    assert_eq!(rig_seam::input_tokens_of(&silent), None);

    let mut reported = rig::completion::Usage::new();
    reported.input_tokens = 120;
    reported.cached_input_tokens = 4_000;
    assert_eq!(
        rig_seam::input_tokens_of(&reported),
        Some(InputTokens {
            cached: 4_000,
            uncached: 120,
        }),
    );

    // A call that read everything from the cache still reported something.
    let mut all_cached = rig::completion::Usage::new();
    all_cached.cached_input_tokens = 4_000;
    assert_eq!(
        rig_seam::input_tokens_of(&all_cached),
        Some(InputTokens {
            cached: 4_000,
            uncached: 0,
        }),
    );
}





#[test]
fn the_deadline_spans_the_whole_turn_rather_than_each_call_within_it() {
    // CVL-FR-17, CVL-FR-18, AGC-FR-18 (second half) / CVL-FR-16. A model that answers every call
    // promptly but always by asking for another tool: no single call ever
    // exceeds the bound, and the turn still has to end.
    //
    // This is the test that fails if the deadline is applied per call — under
    // that reading every call here is comfortably inside it and the turn runs to
    // the model-call bound instead, delivering or failing `empty_reply`.
    //
    // One clause of CVL-FR-16 is reached only indirectly here, and knowingly: "a
    // tool call in flight is abandoned and its result discarded". This test
    // establishes that the deadline ends a turn that is looping through tools and
    // that the turn appends nothing, but not *which* tool call was in flight when
    // it expired, because nothing here can hold a tool open. The gated seam beside
    // it (`ToolThenGated`) gates the model call that follows a tool rather than a
    // tool call itself, and the tool set comes from `conversation_tools`, which is
    // not injectable — deliberately, because CVL-FR-08 fixes the set by origin
    // kind and `an_artifact_turn_is_offered_the_six_and_a_draft_turn_the_seven`
    // pins it exactly. A test-only tool added to reach this clause would falsify
    // the requirement that test exists to protect, so the discard itself is left
    // to the route that can gate there: cancellation, in
    // `cancelling_mid_loop_discards_the_tool_result_and_appends_nothing`, which
    // lands mid-loop and asserts the turn appends nothing and releases its
    // session. What is untested is that the *deadline* reaches that same discard,
    // and closing it means making the loop's tool set injectable — a change to
    // the loop rather than to its tests.
    let per_call = Duration::from_millis(150);
    assert!(
        per_call < SHORT_TIMEOUT,
        "the point of this test is that no single call exceeds the deadline",
    );
    let script: Vec<Result<ScriptedReply, &'static str>> = (0..MAX_MODEL_CALLS)
        .map(|_| Ok(ScriptedReply::calls("list_skills", serde_json::json!({}))))
        .collect();
    let h = Harness::scripted_impatient(script, per_call);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");

    assert_eq!(terminal.state, AgentTurnState::Failed);
    assert_eq!(
        terminal.failure.as_deref(),
        Some(FAIL_TIMED_OUT),
        "the turn ran out of time rather than reaching the model-call bound",
    );
    assert!(
        h.seam.call_count() < MAX_MODEL_CALLS,
        "the deadline stopped the loop before the bound did: {} calls",
        h.seam.call_count(),
    );
    assert_eq!(
        h.threads("spec.md")[0].comments.len(),
        1,
        "a timed-out turn appends nothing",
    );
    assert_eq!(h.agent_sessions(), 0, "its session ended with it");
}
