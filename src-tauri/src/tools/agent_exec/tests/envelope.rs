//! EAC-FR-18 … EAC-FR-20 — envelope validation, escalations, and metadata.

use super::*;

// ---------------------------------------------------------------------------
// EAC-FR-18, EAC-FR-19 … EAC-FR-32, EAC-FR-20, CCP-FR-07 — envelope validation
// ---------------------------------------------------------------------------

/// EAC-FR-18, EAC-FR-36, EAC-FR-22 — an escalation reaches the caller whole, in the order it was
/// written, with every option intact.
#[test]
fn an_escalation_reaches_the_caller_in_the_order_it_was_written() {
    // EAC-FR-18 (EAC-FR-18, EAC-FR-36, EAC-FR-22).
    let counts = [3usize, 2, 1, 0, 3, 2, 1, 0];
    let questions: Vec<serde_json::Value> = counts
        .iter()
        .enumerate()
        .map(|(index, count)| {
            let options: Vec<serde_json::Value> = (0..*count)
                .map(|option| {
                    json!({
                        "answer": format!("answer-{index}-{option}"),
                        "summary": format!("Choice {option}"),
                        "description": "What choosing it means.",
                    })
                })
                .collect();
            json!({ "question": format!("question {index}?"), "options": options })
        })
        .collect();
    let document = json!({
        "protocol_version": 1,
        "outcome": "escalation_required",
        "summary": "stopped to ask",
        "escalation": { "reason": "eight things", "questions": questions },
    });
    // Through the runtime rather than the decoder alone, so the scenario's
    // "the result is `completed` carrying all eight" is what is asserted
    // (EAC-FR-22) rather than only that the bytes parse.
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&claude_stdout(&document.to_string()));
    let outcome = run(&harness, runtime, task("go")).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    let escalation = outcome
        .response
        .expect("envelope")
        .escalation
        .expect("the escalation");
    assert_eq!(escalation.questions.len(), 8);
    for (index, count) in counts.iter().enumerate() {
        assert_eq!(escalation.questions[index].question, format!("question {index}?"));
        assert_eq!(escalation.questions[index].options.len(), *count);
        if *count == 0 {
            // An empty list rather than a null one: the caller reads "no fixed
            // response suits" without having to tell absence from emptiness.
            assert!(escalation.questions[index].options.is_empty());
        } else {
            assert_eq!(
                escalation.questions[index].options[0].answer,
                format!("answer-{index}-0"),
            );
            assert_eq!(
                escalation.questions[index].options[0].summary,
                "Choice 0",
                "and every option's label, not only its value",
            );
            assert_eq!(
                escalation.questions[index].options[*count - 1].summary,
                format!("Choice {}", *count - 1),
                "including the last, so a list read only at its head is caught",
            );
            assert_eq!(
                escalation.questions[index].options[0].description,
                "What choosing it means.",
            );
        }
    }

    // One question alone is accepted on identical terms.
    let one = json!({
        "protocol_version": 1,
        "outcome": "escalation_required",
        "summary": "stopped to ask",
        "escalation": { "reason": "one thing", "questions": [{ "question": "which?" }] },
    });
    let escalation = decode_envelope(&one.to_string())
        .expect("one question is an escalation")
        .escalation
        .expect("the escalation");
    assert_eq!(escalation.questions.len(), 1);
    assert!(escalation.questions[0].options.is_empty());
}

/// EAC-FR-18, EAC-FR-36 — a malformed escalation refuses the envelope entire, and no
/// partially decoded question list exists anywhere.
#[test]
fn a_malformed_escalation_refuses_the_whole_envelope() {
    // EAC-FR-18 (EAC-FR-18, EAC-FR-36).
    let good = json!({ "question": "which?" });
    let violations = [
        // A singular `question` at the escalation's own level.
        json!({ "reason": "r", "question": "which?" }),
        // A singular `options` beside `questions`.
        json!({ "reason": "r", "questions": [good], "options": [] }),
        // Nine questions, and zero.
        json!({ "reason": "r", "questions": (0..9)
            .map(|index| json!({ "question": format!("q{index}?") }))
            .collect::<Vec<_>>() }),
        json!({ "reason": "r", "questions": [] }),
        // A whitespace-only question text.
        json!({ "reason": "r", "questions": [{ "question": "   " }] }),
        // A blank reason.
        json!({ "reason": "  ", "questions": [good] }),
        // Four options on one question.
        json!({ "reason": "r", "questions": [{ "question": "which?", "options": (0..4)
            .map(|index| json!({
                "answer": format!("a{index}"),
                "summary": "A choice",
                "description": "What it means.",
            }))
            .collect::<Vec<_>>() }] }),
        // Each of the three response formats, one at a time.
        json!({ "reason": "r", "questions": [{ "question": "which?", "options": [
            { "answer": "  ", "summary": "A choice", "description": "What it means." }] }] }),
        json!({ "reason": "r", "questions": [{ "question": "which?", "options": [
            { "answer": "a", "summary": "one two three four five six",
              "description": "What it means." }] }] }),
        json!({ "reason": "r", "questions": [{ "question": "which?", "options": [
            { "answer": "a", "summary": "Two. Sentences.",
              "description": "What it means." }] }] }),
        json!({ "reason": "r", "questions": [{ "question": "which?", "options": [
            { "answer": "a", "summary": "A choice",
              "description": "One. Two. Three." }] }] }),
        json!({ "reason": "r", "questions": [{ "question": "which?", "options": [
            { "answer": "a", "summary": "A choice",
              "description": "one two three four five six seven eight nine ten eleven twelve thirteen" }] }] }),
    ];

    for escalation in violations {
        let document = json!({
            "protocol_version": 1,
            "outcome": "escalation_required",
            "summary": "stopped to ask",
            "escalation": escalation,
        });
        let runtime = RecordingRuntime::replying(&claude_stdout(&document.to_string()));
        let harness = harness_for("claude_code");
        let outcome = run(&harness, runtime, task("go")).expect("the process still ran");
        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::InvalidStructuredOutput,
            "accepted {escalation}",
        );
        assert!(
            outcome.response.is_none(),
            "no escalation reaches the caller, and no partial question list exists",
        );
    }
}

/// EAC-FR-08 — the escalation's own byte bounds, each named and each refusing.
#[test]
fn every_escalation_byte_bound_refuses_the_envelope() {
    let over = |limit: usize| "x".repeat(limit + 1);
    let cases = [
        json!({ "reason": over(LIMIT_ESCALATION_REASON), "questions": [{ "question": "q?" }] }),
        json!({ "reason": "r", "questions": [{ "question": over(LIMIT_ESCALATION_QUESTION) }] }),
        json!({ "reason": "r", "questions": [{ "question": "q?", "options": [
            { "answer": over(LIMIT_OPTION_ANSWER), "summary": "A choice",
              "description": "What it means." }] }] }),
        json!({ "reason": "r", "questions": [{ "question": "q?", "options": [
            { "answer": "a", "summary": over(LIMIT_OPTION_SUMMARY),
              "description": "What it means." }] }] }),
        json!({ "reason": "r", "questions": [{ "question": "q?", "options": [
            { "answer": "a", "summary": "A choice",
              "description": over(LIMIT_OPTION_DESCRIPTION) }] }] }),
    ];
    for escalation in cases {
        let document = json!({
            "protocol_version": 1,
            "outcome": "escalation_required",
            "summary": "stopped to ask",
            "escalation": escalation,
        });
        assert_eq!(
            decode_envelope(&document.to_string()).unwrap_err(),
            EnvelopeInvalid::TooLarge,
        );
    }
}

/// EAC-FR-08 / EAC-FR-18 — a value **exactly at** its limit is accepted.
///
/// Every bound above is tested at `limit + 1`; without this, changing any `>` to
/// `>=` would turn nothing red and an agent that answered exactly to the stated
/// contract would be refused for having done so — the one failure EAC-FR-35 is
/// about.
#[test]
fn a_value_exactly_at_its_limit_is_accepted() {
    let at = |limit: usize| "x".repeat(limit);
    let document = json!({
        "protocol_version": 1,
        "outcome": "escalation_required",
        "summary": "stopped to ask",
        "escalation": {
            "reason": at(LIMIT_ESCALATION_REASON),
            "questions": (0..MAX_ESCALATION_QUESTIONS)
                .map(|index| json!({
                    "question": at(LIMIT_ESCALATION_QUESTION),
                    // Exactly the greatest number of options a question may
                    // carry, on the first question alone so the whole escalation
                    // stays inside its own serialized bound.
                    "options": if index == 0 {
                        (0..MAX_QUESTION_OPTIONS)
                            .map(|option| json!({
                                "answer": at(LIMIT_OPTION_ANSWER),
                                // Exactly five words, and exactly two sentences
                                // of exactly twelve words.
                                "summary": "one two three four five",
                                "description": "one two three four five six. \
                                                seven eight nine ten eleven twelve.",
                                "_": option,
                            }))
                            .map(|mut value| {
                                value.as_object_mut().expect("an object").remove("_");
                                value
                            })
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    },
                }))
                .collect::<Vec<_>>(),
        },
    });
    let escalation = decode_envelope(&document.to_string())
        .expect("a value exactly at its limit is inside it")
        .escalation
        .expect("the escalation");
    assert_eq!(escalation.questions.len(), MAX_ESCALATION_QUESTIONS);
    assert_eq!(escalation.questions[0].options.len(), MAX_QUESTION_OPTIONS);
    assert_eq!(escalation.reason.len(), LIMIT_ESCALATION_REASON);
    assert_eq!(
        word_count(&escalation.questions[0].options[0].summary),
        MAX_SUMMARY_WORDS,
    );
    assert_eq!(
        sentence_count(&escalation.questions[0].options[0].description),
        MAX_DESCRIPTION_SENTENCES,
    );
    assert_eq!(
        word_count(&escalation.questions[0].options[0].description),
        MAX_DESCRIPTION_WORDS,
    );
}

/// The two readable shapes, exercised where they are defined rather than only
/// through an envelope (EAC-FR-18).
#[test]
fn the_readable_shapes_count_words_and_sentences_as_a_reader_would() {
    assert_eq!(word_count("one two three"), 3);
    assert_eq!(word_count("  one   two  "), 2);
    // Stray punctuation between words is not itself a word.
    assert_eq!(word_count("one - two"), 2);
    assert_eq!(word_count(""), 0);

    assert_eq!(sentence_count("One sentence"), 1);
    assert_eq!(sentence_count("One sentence."), 1);
    assert_eq!(sentence_count("One. Two."), 2);
    assert_eq!(sentence_count("One. Two"), 2);
    // A run of marks is one ending rather than several.
    assert_eq!(sentence_count("What?!"), 1);
    assert_eq!(sentence_count("   "), 0);

    assert!(summary_shape_ok("Fail the run"));
    assert!(summary_shape_ok("Fail the run."));
    assert!(!summary_shape_ok("one two three four five six"));
    assert!(!summary_shape_ok("One. Two."));
    assert!(!summary_shape_ok("   "));

    assert!(description_shape_ok("Nothing is published until then."));
    assert!(description_shape_ok("One thing. Then another."));
    assert!(!description_shape_ok("One. Two. Three."));
    assert!(!description_shape_ok(
        "one two three four five six seven eight nine ten eleven twelve thirteen"
    ));
}

/// EAC-FR-18, EAC-FR-19 — every exclusivity and schema violation is invalid output, never
/// an agent-reported outcome.
#[test]
fn a_violating_envelope_is_never_read_as_an_outcome() {
    let harness = harness_for("claude_code");

    let violations = [
        // success carrying a failure object
        json!({"protocol_version":1,"outcome":"success","summary":"s",
               "failure":{"code":"c","message":"m","retryable":false}}),
        // failure with a null failure object
        json!({"protocol_version":1,"outcome":"failure","summary":"s"}),
        // escalation carrying a result
        json!({"protocol_version":1,"outcome":"escalation_required","summary":"s",
               "escalation":{"reason":"r","questions":[{"question":"q"}]},
               "result":{"a":1}}),
        // empty summary
        json!({"protocol_version":1,"outcome":"success","summary":"   "}),
        // wrong protocol version
        json!({"protocol_version":2,"outcome":"success","summary":"s"}),
        // an undefined field
        json!({"protocol_version":1,"outcome":"success","summary":"s","extra":true}),
        // failure carrying a result as well
        json!({"protocol_version":1,"outcome":"failure","summary":"s",
               "failure":{"code":"c","message":"m","retryable":true},"result":{"a":1}}),
    ];

    for envelope in violations {
        let runtime = RecordingRuntime::replying(&claude_stdout(&envelope.to_string()));
        let outcome = run(&harness, runtime, task("go")).expect("the process still ran");
        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::InvalidStructuredOutput,
            "accepted {envelope}"
        );
        assert!(outcome.response.is_none());
    }
}

/// EAC-FR-19, EAC-FR-23 — prose, partial JSON, two documents, and non-UTF-8.
#[test]
fn unparseable_output_is_invalid_and_the_bytes_are_preserved() {
    let harness = harness_for("claude_code");
    let valid = envelope_json("success");

    // Prose where a JSON document was required.
    let prose = RecordingRuntime::replying("I have finished the task.\n");
    let outcome = run(&harness, prose, task("go")).expect("ran");
    assert_eq!(
        outcome.process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );
    assert_eq!(outcome.stdout.bytes, b"I have finished the task.\n");

    // A truncated document.
    let partial = RecordingRuntime::replying(&claude_stdout(&valid[..valid.len() / 2]));
    assert_eq!(
        run(&harness, partial, task("go"))
            .expect("ran")
            .process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );

    // Two complete envelopes back to back: two answers about one turn, and
    // picking either would be a guess.
    let doubled = RecordingRuntime::replying(&claude_stdout(&format!("{valid}{valid}")));
    assert_eq!(
        run(&harness, doubled, task("go"))
            .expect("ran")
            .process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );

    // Bytes that are not valid UTF-8 where JSON was required.
    let raw = Arc::new(RecordingRuntime {
        script: Script::ReplyRaw {
            stdout: vec![0xff, 0xfe, 0x00],
            exit_code: 0,
        },
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails: false,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
    });
    assert_eq!(
        run(&harness, raw, task("go")).expect("ran").process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );
}

/// EAC-FR-32, EAC-FR-20, CCP-FR-07 — the v1 metadata allowlist is empty, and an unreviewed key is
/// discarded rather than allowed to end the turn.
#[test]
fn metadata_admits_nothing_in_v1() {
    let harness = harness_for("claude_code");

    for metadata in [json!(null), json!({})] {
        let envelope = json!({
            "protocol_version": 1, "outcome": "success", "summary": "s",
            "metadata": metadata
        });
        let runtime = RecordingRuntime::replying(&claude_stdout(&envelope.to_string()));
        let execution = run(&harness, runtime, task("go")).expect("ran");
        assert_eq!(execution.process_outcome, ProcessOutcome::Completed);
        assert!(execution.response.expect("envelope").metadata.is_none());
    }

    // Any key at all — including one that looks harmless — is unreviewed. It is
    // removed, which is what keeps transcript data out of every caller: a key
    // nobody receives can carry nothing. The turn itself stands, because the
    // agent's work is not what the field was wrong about.
    //
    // Both pinned vendors, and the second is the one that matters most: Codex
    // validates against no schema (CDX-FR-12), so for that vendor this discard
    // is not a backstop behind the CLI's own validation — it is the whole of
    // the rule.
    for vendor in ["claude_code", "codex"] {
        let harness = harness_for(vendor);
        for metadata in [
            json!({"transcript": "…the whole conversation…"}),
            json!({"files_written": ["a.md"]}),
            json!({"turns": 3}),
        ] {
            let envelope = json!({
                "protocol_version": 1, "outcome": "success", "summary": "s",
                "result": {"paths": ["a.md"]},
                "metadata": metadata
            });
            let stdout = match vendor {
                "codex" => codex_stdout(&envelope.to_string()),
                _ => claude_stdout(&envelope.to_string()),
            };
            let runtime = RecordingRuntime::replying(&stdout);
            let execution = run(&harness, runtime, task("go")).expect("ran");
            assert_eq!(
                execution.process_outcome,
                ProcessOutcome::Completed,
                "{vendor} rejected the turn over {metadata}"
            );
            let response = execution.response.expect("envelope");
            assert!(
                response.metadata.is_none(),
                "{vendor} kept an unreviewed key from {metadata}"
            );
            // Everything else the agent reported survives the removal — the
            // field is dropped, not the envelope around it.
            assert_eq!(response.outcome, AgentOutcome::Success);
            assert_eq!(response.summary, "s");
            assert!(response.result.is_some());
        }
    }
}

/// EAC-FR-32, EAC-FR-20, CCP-FR-07 — an unreviewed key never changes *why* an envelope is refused.
///
/// The discard runs before validation, so every rule that ends a turn has to
/// end it for its own reason with the field present. Asserted on the exact
/// variant rather than on `InvalidStructuredOutput`, because the failure this
/// guards against — somebody reordering the discard after `validate` — would
/// still produce an invalid envelope, just the wrong kind of one.
#[test]
fn a_populated_metadata_never_changes_why_an_envelope_is_refused() {
    let smuggled = json!({"transcript": "…the whole conversation…"});
    let with_metadata = |mut envelope: serde_json::Value| {
        envelope["metadata"] = smuggled.clone();
        envelope.to_string()
    };

    let cases = [
        (
            "success carrying a failure object",
            json!({"protocol_version": 1, "outcome": "success", "summary": "s",
                   "failure": {"code": "c", "message": "m", "retryable": true}}),
            EnvelopeInvalid::Exclusivity,
        ),
        (
            "an empty summary",
            json!({"protocol_version": 1, "outcome": "success", "summary": "  "}),
            EnvelopeInvalid::SummaryEmpty,
        ),
        (
            "a protocol version this build does not speak",
            json!({"protocol_version": 2, "outcome": "success", "summary": "s"}),
            EnvelopeInvalid::ProtocolVersion,
        ),
        (
            "a result past its bound",
            json!({"protocol_version": 1, "outcome": "success", "summary": "s",
                   "result": {"paths": "x".repeat(LIMIT_RESULT + 1)}}),
            EnvelopeInvalid::TooLarge,
        ),
        (
            "a session field past its bound",
            json!({"protocol_version": 1, "outcome": "success", "summary": "s",
                   "session": {"session_id": "x".repeat(LIMIT_SESSION_FIELD + 1)}}),
            EnvelopeInvalid::TooLarge,
        ),
    ];

    for (what, envelope, expected) in cases {
        assert_eq!(
            decode_envelope(&with_metadata(envelope)).unwrap_err(),
            expected,
            "{what} was not refused for its own reason"
        );
    }
}

/// EAC-FR-08 / EAC-FR-20 — the envelope bound outranks the discard, by
/// decision.
///
/// Removing a field means parsing the document that holds it, and parsing an
/// unbounded document to rescue one turn is the worse trade. So `metadata` fat
/// enough to carry the envelope past its bound is still `TooLarge` — the one
/// case where an unreviewed key does end a turn, recorded here so that it is a
/// decision somebody made rather than a gap somebody finds.
#[test]
fn metadata_large_enough_to_burst_the_envelope_bound_still_refuses() {
    let envelope = json!({
        "protocol_version": 1, "outcome": "success", "summary": "s",
        "metadata": {"transcript": "x".repeat(LIMIT_ENVELOPE)}
    });
    assert_eq!(
        decode_envelope(&envelope.to_string()).unwrap_err(),
        EnvelopeInvalid::TooLarge
    );
}

/// EAC-FR-32, EAC-FR-20 / CCP-FR-07 — the schema the CLI validates against states the
/// empty allowlist, so a populated `metadata` is corrected inside the run
/// rather than discarded after it.
#[test]
fn the_vendor_schema_states_the_empty_metadata_allowlist() {
    let schema: serde_json::Value =
        serde_json::from_str(descriptor::claude_code::ENVELOPE_SCHEMA).expect("valid JSON Schema");
    assert_eq!(schema["properties"]["metadata"]["maxProperties"], 0);
    // Still nullable, and that matters: `null` is the overwhelmingly common
    // answer, and a tightening that made it invalid would refuse almost every
    // turn at the CLI instead of almost none.
    assert_eq!(
        schema["properties"]["metadata"]["type"],
        json!(["object", "null"])
    );
}

/// CCP-FR-07 — the schema closes every field set the decoder closes.
///
/// `deny_unknown_fields` sits on the envelope and on each of its objects. A
/// schema that did not say so would let an agent's extra key pass the CLI's
/// validation and be refused by the decoder afterwards — the same loss the
/// `metadata` bound prevents, at a different field. `result` is exempt because
/// it is free-form by contract, and `metadata` because it is bounded to no keys
/// at all.
#[test]
fn the_vendor_schema_closes_every_field_set_the_decoder_closes() {
    let schema: serde_json::Value =
        serde_json::from_str(descriptor::claude_code::ENVELOPE_SCHEMA).expect("valid JSON Schema");

    assert_eq!(schema["additionalProperties"], json!(false), "the envelope");
    for object in ["failure", "escalation", "session"] {
        assert_eq!(
            schema["properties"][object]["additionalProperties"],
            json!(false),
            "{object} is left open"
        );
    }
    for free_form in ["result", "metadata"] {
        assert!(
            schema["properties"][free_form]["additionalProperties"].is_null(),
            "{free_form} is not a closed field set and must not claim to be"
        );
    }

    // And it demands exactly what the decoder demands. Stricter is not the safe
    // direction to err in: a field the decoder defaults but the schema requires
    // makes the CLI re-prompt an agent that had already answered correctly.
    // Each list below is the set of fields on the matching struct that carry no
    // `#[serde(default)]`.
    assert_eq!(
        schema["required"],
        json!(["protocol_version", "outcome", "summary"])
    );
    assert_eq!(
        schema["properties"]["failure"]["required"],
        json!(["code", "message", "retryable"])
    );
    // EAC-FR-18: `options` is defaulted by `AgentEscalationQuestion` and
    // optional by `../tools/ESU-escalate-to-user-tool.md` ESU-FR-10.
    assert_eq!(
        schema["properties"]["escalation"]["required"],
        json!(["reason", "questions"])
    );
    assert_eq!(
        schema["properties"]["escalation"]["properties"]["questions"]["items"]["required"],
        json!(["question"])
    );
    assert_eq!(
        schema["properties"]["escalation"]["properties"]["questions"]["items"]["properties"]
            ["options"]["items"]["required"],
        json!(["answer", "summary", "description"])
    );
    // `SessionRef` defaults both of its fields.
    assert!(schema["properties"]["session"]["required"].is_null());
}

/// EAC-FR-32, EAC-FR-20, CCP-FR-07 — where a discarded key goes, and where it does not.
///
/// It reaches no caller and no boundary record. It *does* reach the activity
/// channel, because EAC-FR-32 carries the vendor's own line rather than a
/// reading of it, and a line that carried the envelope carries whatever the
/// envelope said. That is asserted positively rather than left out: the
/// carriage is bounded and by design, and a reader who finds it later should
/// find a test that expected it.
#[test]
fn a_discarded_metadata_key_reaches_no_caller_and_no_boundary_record() {
    const MARKER: &str = "unreviewed-key-marker-9f2x";

    let harness = harness_for("claude_code");
    let envelope = json!({
        "protocol_version": 1, "outcome": "success", "summary": "s",
        "metadata": {"transcript": MARKER}
    });
    let runtime = RecordingRuntime::replying(&claude_stdout(&envelope.to_string()));
    let sink = Arc::new(CollectedActivity::default());
    let execution = run_watching(&harness, runtime.clone(), task("go"), sink.clone()).expect("ran");

    assert_eq!(execution.process_outcome, ProcessOutcome::Completed);
    let response = execution.response.expect("envelope");
    assert!(response.metadata.is_none());
    assert!(
        !format!("{response:?}").contains(MARKER),
        "the discarded key survived somewhere in the returned envelope"
    );

    // Attributed to this launch by its container name, because the log buffer
    // is process-global and a scan of the whole rendering would prove nothing.
    let container = container_name(&runtime.only_run().argv);
    let page = crate::logging::LogBuffer::query(
        super::super::log_buffer(),
        &crate::logging::LogFilter::default(),
        None,
        20_000,
    )
    .expect("query");
    for record in page.records.iter().filter(|record| {
        record.fields.get("container").and_then(|v| v.as_str()) == Some(container.as_str())
            && matches!(
                record.level,
                crate::logging::LogLevel::Info | crate::logging::LogLevel::Warn
            )
    }) {
        assert!(
            !serde_json::to_string(&record.fields)
                .unwrap_or_default()
                .contains(MARKER),
            "a boundary record carried the discarded key: {}",
            record.message
        );
    }

    // EAC-FR-32: the vendor's line, verbatim and bounded. The envelope was in
    // it, so the key is too.
    assert!(
        sink.rendered().contains(MARKER),
        "the activity channel stopped carrying the vendor's own line"
    );
}
