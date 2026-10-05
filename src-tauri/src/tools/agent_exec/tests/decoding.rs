//! CCP-FR-07, CDX-FR-14 — the vendor readers, session handling, and the image a project commits.

use super::*;

/// The envelope decoder in isolation: the cases the extraction path funnels
/// into it, without a container in the way.
#[test]
fn decode_envelope_distinguishes_absent_malformed_and_repeated_documents() {
    let valid = envelope_json("success");
    assert!(decode_envelope(&valid).is_ok());
    // Trailing whitespace is not a second document.
    assert!(decode_envelope(&format!("{valid}\n\n  ")).is_ok());

    assert_eq!(decode_envelope("").unwrap_err(), EnvelopeInvalid::NotFound);
    assert_eq!(
        decode_envelope("hello").unwrap_err(),
        EnvelopeInvalid::Malformed
    );
    assert_eq!(
        decode_envelope(&valid[..valid.len() - 3]).unwrap_err(),
        EnvelopeInvalid::Malformed
    );
    assert_eq!(
        decode_envelope(&format!("{valid} {valid}")).unwrap_err(),
        EnvelopeInvalid::MultipleDocuments
    );
    // A second document that is itself rubbish is still two documents.
    assert_eq!(
        decode_envelope(&format!("{valid} 12")).unwrap_err(),
        EnvelopeInvalid::MultipleDocuments
    );
}

/// CDX-FR-12 / CDX-FR-13 / CDX-FR-17 / CDX-FR-20 — the Codex event reader.
#[test]
fn the_codex_reader_takes_the_last_completed_agent_message() {
    let descriptor = &descriptor::CODEX;
    let valid = envelope_json("success");

    // CDX-FR-12: a turn narrates before it concludes, and the conclusion is the
    // envelope. Reasoning, commands, and file changes are passed over.
    let stream = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n",
        json!({"type": "thread.started", "thread_id": "thr-1"}),
        json!({"type": "turn.started"}),
        json!({"type":"item.completed","item":{"id":"i1","type":"reasoning","text":"hmm"}}),
        json!({"type":"item.completed","item":{"id":"i2","type":"command_execution",
               "command":"cargo test","exit_code":0,"status":"completed"}}),
        json!({"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"narrating"}}),
        json!({"type":"item.completed","item":{"id":"i4","type":"agent_message","text":valid}})
    );
    let (envelope, session) = descriptor.extract(&stream, None).expect("extracts");
    assert_eq!(envelope.summary, "did the thing");
    // CDX-FR-20: the identity comes from `thread.started`, not from any item.
    assert_eq!(session.as_deref(), Some("thr-1"));

    // CDX-FR-17: a non-JSON line, an unknown event type, and an unknown item
    // type are ignored rather than treated as a failure — the vendor documents
    // that unknown fields and events may appear.
    let noisy = format!(
        "Reading prompt from stdin...\n{}\n{}\n{}\n{}\n",
        json!({"type": "thread.started", "thread_id": "thr-2"}),
        json!({"type": "some.future.event", "payload": 1}),
        json!({"type":"item.completed","item":{"id":"i1","type":"web_search","query":"x"}}),
        json!({"type":"item.completed","item":{"id":"i2","type":"agent_message","text":valid}})
    );
    let (envelope, session) = descriptor.extract(&noisy, None).expect("noise is ignored");
    assert_eq!(envelope.summary, "did the thing");
    assert_eq!(session.as_deref(), Some("thr-2"));

    // CDX-FR-15: a transient reconnect notice arrives as `type: "error"` and is
    // not by itself fatal.
    let reconnecting = format!(
        "{}\n{}\n{}\n{}\n",
        json!({"type": "thread.started", "thread_id": "thr-3"}),
        json!({"type": "error", "message": "Reconnecting... 1/5"}),
        json!({"type": "error", "message": "Reconnecting... 2/5"}),
        json!({"type":"item.completed","item":{"id":"i1","type":"agent_message","text":valid}})
    );
    assert!(descriptor.extract(&reconnecting, None).is_ok());

    // CDX-FR-16: a turn that failed yields no envelope even where an earlier
    // agent message is present.
    let failed = format!(
        "{}\n{}\n{}\n",
        json!({"type": "thread.started", "thread_id": "thr-4"}),
        json!({"type":"item.completed","item":{"id":"i1","type":"agent_message","text":valid}}),
        json!({"type": "turn.failed", "error": {"message": "model unavailable"}})
    );
    assert_eq!(
        descriptor.extract(&failed, None).unwrap_err(),
        EnvelopeInvalid::VendorRunFailed
    );

    // CDX-FR-13: events but never an agent message, and nothing at all.
    let no_message = format!("{}\n", json!({"type":"thread.started","thread_id":"thr-5"}));
    assert_eq!(
        descriptor.extract(&no_message, None).unwrap_err(),
        EnvelopeInvalid::NotFound
    );
    assert_eq!(
        descriptor.extract("", None).unwrap_err(),
        EnvelopeInvalid::NotFound
    );
    // An agent message whose text is prose rather than an envelope. The CLI does
    // not validate the shape (CDX-FR-14), so this is the expected failure mode.
    let prose = format!(
        "{}\n",
        json!({"type":"item.completed","item":{"id":"i1","type":"agent_message",
               "text":"I finished the task."}})
    );
    assert_eq!(
        descriptor.extract(&prose, None).unwrap_err(),
        EnvelopeInvalid::Malformed
    );
}

/// CCP-FR-09 / CCP-FR-10 / CCP-FR-12 / CCP-FR-13 / CCP-FR-16 — the Claude Code
/// result reader, including the ordered extraction positions.
#[test]
fn the_claude_code_reader_prefers_structured_output_then_falls_back_to_result() {
    let valid = envelope_json("success");
    let parsed: serde_json::Value = serde_json::from_str(&valid).unwrap();

    // CCP-FR-09, first half: both positions carry a document, and the first wins.
    // A distinguishable summary in the text position proves the object was used.
    let mut other = parsed.clone();
    other["summary"] = json!("from the text position");
    let both = json!({
        "type": "result", "subtype": "success", "is_error": false,
        "structured_output": parsed,
        "result": other.to_string(),
        "session_id": "s-1"
    });
    let (envelope, session) = descriptor::CLAUDE_CODE
        .extract(&both.to_string(), None)
        .expect("extracts");
    assert_eq!(envelope.summary, "did the thing");
    assert_eq!(session.as_deref(), Some("s-1"));

    // CCP-FR-09, second half: no `structured_output`, so the text position is
    // used.
    let (envelope, _) = descriptor::CLAUDE_CODE
        .extract(&claude_stdout(&valid), None)
        .expect("extracts");
    assert_eq!(envelope.summary, "did the thing");

    // CCP-FR-10: neither position yields a document.
    let neither = json!({
        "type": "result", "subtype": "success", "is_error": false,
        "result": "I finished the task.", "session_id": "s-1"
    });
    assert_eq!(
        descriptor::CLAUDE_CODE
            .extract(&neither.to_string(), None)
            .unwrap_err(),
        EnvelopeInvalid::Malformed
    );
    let absent = json!({"type":"result","subtype":"success","is_error":false,"session_id":"s"});
    assert_eq!(
        descriptor::CLAUDE_CODE
            .extract(&absent.to_string(), None)
            .unwrap_err(),
        EnvelopeInvalid::NotFound
    );

    // CCP-FR-12: a failed run, whatever the exit status — including the
    // documented case of a run that reports its failure under subtype success.
    for document in [
        json!({"type":"result","subtype":"error_max_turns","result":valid,"session_id":"s"}),
        json!({"type":"result","subtype":"error_during_execution","result":valid,"session_id":"s"}),
        json!({"type":"result","subtype":"error_max_structured_output_retries",
               "result":valid,"session_id":"s"}),
        json!({"type":"result","subtype":"success","is_error":true,
               "result":valid,"session_id":"s"}),
    ] {
        assert_eq!(
            descriptor::CLAUDE_CODE
                .extract(&document.to_string(), None)
                .unwrap_err(),
            EnvelopeInvalid::VendorRunFailed,
            "{document} should be a failed run"
        );
    }

    // CCP-FR-16: the identity the executor assigned is asserted back.
    let assigned = claude_stdout(&valid).replace(SESSION_PLACEHOLDER, "mine");
    assert!(descriptor::CLAUDE_CODE
        .extract(&assigned, Some("mine"))
        .is_ok());
    assert_eq!(
        descriptor::CLAUDE_CODE
            .extract(&assigned, Some("theirs"))
            .unwrap_err(),
        EnvelopeInvalid::SessionMismatch
    );

    // CCP-FR-13: stdout is one JSON event per line, and the result event is the
    // one extraction reads. The progress events beside it are not documents
    // competing with it, so a stream is read rather than rejected.
    let stream = claude_stdout(&valid);
    assert!(
        stream.lines().count() > 1,
        "the fixture must be a stream, or this asserts nothing"
    );
    assert!(descriptor::CLAUDE_CODE.extract(&stream, None).is_ok());

    // A line this build cannot parse is skipped rather than failing the read:
    // the CLI writes its own notices into this stream, and one appearing beside
    // a well-formed result is not a reason to discard the result.
    assert!(descriptor::CLAUDE_CODE
        .extract(&format!("Starting up…\n{stream}"), None)
        .is_ok());

    // Two result events is the turn ending twice, and the turn that ended last
    // is the turn. Reading the first would answer with a superseded document.
    let second = claude_result_line(&envelope_json("failure"));
    let (envelope, _) = descriptor::CLAUDE_CODE
        .extract(&format!("{stream}{second}\n"), None)
        .expect("the last result event");
    assert_eq!(envelope.outcome, AgentOutcome::Failure);

    // A stream whose result line was cut off has no result event in it, however
    // many complete events came before.
    let cut = &stream[..stream.len() - 4];
    assert_eq!(
        descriptor::CLAUDE_CODE.extract(cut, None).unwrap_err(),
        EnvelopeInvalid::NotFound
    );
    assert_eq!(
        descriptor::CLAUDE_CODE.extract("", None).unwrap_err(),
        EnvelopeInvalid::NotFound
    );
    // Progress alone, with no result event at all: a turn that was killed
    // mid-flight looks exactly like this.
    let progress = stream.lines().next().unwrap();
    assert_eq!(
        descriptor::CLAUDE_CODE.extract(progress, None).unwrap_err(),
        EnvelopeInvalid::NotFound
    );
}

/// EAC-FR-30 / EAC-FR-31 — a resume that quietly began a new conversation is
/// reported, not returned as a continuation.
///
/// One pinned vendor cannot fail a resume: asked to continue a session it no
/// longer holds, it starts a fresh one and reports success (CDX-FR-22). The only
/// thing that tells the two apart is the identity the run came back with.
#[test]
fn a_resume_that_silently_started_a_new_session_is_reported() {
    let harness = harness_for("codex");
    let mut resumed = task("continue");
    resumed.resume = Some(SessionRef {
        session_id: Some("a-session-the-cli-no-longer-holds".into()),
        continuation_token: None,
    });

    // The run succeeds and reports a thread that is not the one asked for —
    // exactly what a silently-fresh session looks like on the wire.
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    let outcome = run(&harness, runtime, resumed).expect("ran");

    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    assert!(
        outcome.resume_unavailable,
        "a fresh session was returned as though it continued the requested one"
    );
    assert_eq!(
        outcome.session.and_then(|s| s.session_id).as_deref(),
        Some(CODEX_THREAD_ID),
        "the caller still learns which session it actually got"
    );

    // And a resume the vendor genuinely honoured is not flagged.
    let mut honoured = task("continue");
    honoured.resume = Some(SessionRef {
        session_id: Some(CODEX_THREAD_ID.into()),
        continuation_token: None,
    });
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    let outcome = run(&harness, runtime, honoured).expect("ran");
    assert!(!outcome.resume_unavailable);
}

/// EAC-FR-31 — a session-state directory that cannot be established is a
/// refusal, not a launch without it.
#[test]
fn a_session_state_directory_that_cannot_be_established_refuses_the_launch() {
    let harness = harness_for("claude_code");

    // Something that is not a directory, exactly where the mount's source
    // belongs. A launch that shrugged and continued would produce a turn that
    // looks successful while losing every session it creates.
    let occupied = descriptor::session_state_dir(
        &harness.sessions_root(),
        "claude_code",
        PROJECT_KEY,
    );
    std::fs::create_dir_all(occupied.parent().expect("a parent")).expect("parent");
    std::fs::write(&occupied, b"not a directory").expect("occupy the path");

    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let error = run(&harness, runtime.clone(), task("go")).expect_err("must refuse");
    assert_eq!(
        error,
        AgentExecutionError::SessionStateUnavailable(SessionStateProblem::Unusable)
    );
    assert_eq!(error.kind(), "session_state_unavailable");
    // Refused before anything was launched.
    assert!(runtime.runs.lock().unwrap().is_empty());
}

/// CCP-FR-12 / CDX-FR-16 through the executor — a run the vendor itself reports
/// as failed is invalid output, **including under exit 0**.
///
/// The unit tests prove the reader classifies these; only this proves the
/// executor surfaces them as `invalid_structured_output` with a null response
/// rather than as a completed turn. The exit-0 half is the whole point: a
/// failure arising inside the run is printed as the result on stdout, so the
/// exit status alone never establishes that a turn succeeded.
#[test]
fn a_run_the_vendor_reports_as_failed_is_invalid_output_even_under_exit_zero() {
    let valid = envelope_json("success");

    let claude_failures = [
        ("subtype error_max_turns", json!({"type":"result","subtype":"error_max_turns",
            "result":valid,"session_id":SESSION_PLACEHOLDER}).to_string()),
        ("subtype error_during_execution", json!({"type":"result","subtype":"error_during_execution",
            "result":valid,"session_id":SESSION_PLACEHOLDER}).to_string()),
        ("schema retries exhausted", json!({"type":"result",
            "subtype":"error_max_structured_output_retries",
            "result":valid,"session_id":SESSION_PLACEHOLDER}).to_string()),
        ("is_error under a success subtype", json!({"type":"result","subtype":"success",
            "is_error":true,"result":valid,"session_id":SESSION_PLACEHOLDER}).to_string()),
        ("no subtype at all", json!({"type":"result","result":valid,
            "session_id":SESSION_PLACEHOLDER}).to_string()),
    ];

    let harness = harness_for("claude_code");
    for (name, stdout) in claude_failures {
        let outcome = run(&harness, RecordingRuntime::replying(&stdout), task("go"))
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::InvalidStructuredOutput,
            "{name} was not reported as invalid output"
        );
        assert!(outcome.response.is_none(), "{name} carried a response");
        assert_eq!(outcome.exit_code, Some(0), "{name} should be an exit-0 case");
    }

    // The same on the other vendor, whose failure arrives as an event rather
    // than as a field — and after a good message, which must not rescue it.
    let codex = harness_for("codex");
    let failed = format!(
        "{}\n{}\n{}\n",
        json!({"type":"thread.started","thread_id":"t"}),
        json!({"type":"item.completed","item":{"id":"i1","type":"agent_message","text":valid}}),
        json!({"type":"turn.failed","error":{"message":"model unavailable"}})
    );
    let outcome = run(&codex, RecordingRuntime::replying(&failed), task("go")).expect("ran");
    assert_eq!(
        outcome.process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );
    assert!(outcome.response.is_none());

    // And the reason reaches the log, so a reader can tell a failed run from a
    // malformed one (EAC-FR-29).
    let page = super::super::log_buffer()
        .query(&crate::logging::LogFilter::default(), None, 20_000)
        .expect("query");
    let rendered = serde_json::to_string(&page.records).expect("serialise");
    assert!(
        rendered.contains("vendor_run_failed"),
        "the log does not distinguish a vendor-reported failure"
    );
}

/// CCP-FR-14 / CDX-FR-18 — diagnostics on stderr alongside a good answer on
/// stdout.
///
/// Every other success case in this suite has an empty stderr, so nothing
/// otherwise proves the two streams stay separate on the path that succeeds —
/// only on the paths that fail.
#[test]
fn diagnostics_on_stderr_do_not_disturb_a_good_answer_on_stdout() {
    const CLAUDE_NOISE: &str = "Warning: 1 MCP server skipped due to invalid config:\n";
    const CODEX_NOISE: &str = "workdir: /workspace\nmodel: gpt-5.1\nsandbox: danger-full-access\n";

    for (vendor, stdout, stderr) in [
        ("claude_code", valid_claude_stdout(), CLAUDE_NOISE),
        (
            "codex",
            codex_stdout(&envelope_json("success")),
            CODEX_NOISE,
        ),
    ] {
        let harness = harness_for(vendor);
        let runtime = RecordingRuntime::replying_with_stderr(&stdout, stderr);
        let outcome = run(&harness, runtime, task("go")).expect("ran");

        assert_eq!(
            outcome.process_outcome,
            ProcessOutcome::Completed,
            "{vendor}: diagnostics on stderr must not disturb the answer"
        );
        assert_eq!(outcome.response.expect("an envelope").summary, "did the thing");
        // Preserved intact for a reader, and never merged into the stream that
        // was parsed.
        assert_eq!(
            String::from_utf8_lossy(&outcome.stderr.bytes),
            stderr,
            "{vendor}: stderr was not preserved"
        );
        assert!(
            !String::from_utf8_lossy(&outcome.stdout.bytes).contains(stderr.trim()),
            "{vendor}: stderr leaked into stdout"
        );
    }
}

/// CDX-FR-12's untested half — the envelope comes from a *completed* item and
/// from no other event.
#[test]
fn only_a_completed_codex_item_can_carry_the_envelope() {
    let descriptor = &descriptor::CODEX;
    let valid = envelope_json("success");
    let mut other = serde_json::from_str::<serde_json::Value>(&valid).unwrap();
    other["summary"] = json!("still typing");

    // An in-flight message is not the conclusion, so a stream that only ever
    // updates one has no envelope in it.
    let only_updated = format!(
        "{}\n{}\n",
        json!({"type":"thread.started","thread_id":"t"}),
        json!({"type":"item.updated","item":{"id":"i1","type":"agent_message",
               "text": other.to_string()}})
    );
    assert_eq!(
        descriptor.extract(&only_updated, None).unwrap_err(),
        EnvelopeInvalid::NotFound
    );

    // And where both appear, the completed one wins whatever the update said.
    let updated_then_completed = format!(
        "{}\n{}\n{}\n",
        json!({"type":"thread.started","thread_id":"t"}),
        json!({"type":"item.updated","item":{"id":"i1","type":"agent_message",
               "text": other.to_string()}}),
        json!({"type":"item.completed","item":{"id":"i1","type":"agent_message","text":valid}})
    );
    let (envelope, _) = descriptor
        .extract(&updated_then_completed, None)
        .expect("extracts");
    assert_eq!(envelope.summary, "did the thing");

    // A final completed message carrying no usable text is a *missing* answer,
    // not a licence to promote the narration before it — that would answer the
    // caller with the wrong message rather than with none.
    let last_has_no_text = format!(
        "{}\n{}\n{}\n",
        json!({"type":"thread.started","thread_id":"t"}),
        json!({"type":"item.completed","item":{"id":"i1","type":"agent_message","text":valid}}),
        json!({"type":"item.completed","item":{"id":"i2","type":"agent_message"}})
    );
    assert_eq!(
        descriptor.extract(&last_has_no_text, None).unwrap_err(),
        EnvelopeInvalid::NotFound
    );
}

/// An empty resolved model or effort generates no flag at all.
///
/// Worth pinning because the failure mode is silent and severe: drop the
/// emptiness filter and `--model` is emitted with `--effort` as its value, so
/// the CLI runs a model named `--effort`.
#[test]
fn an_empty_model_or_effort_generates_no_flag() {
    assert_eq!(
        descriptor::CLAUDE_CODE.vendor_args(Some(""), Some(""), "SID", None, None),
        descriptor::CLAUDE_CODE.vendor_args(None, None, "SID", None, None)
    );
    assert_eq!(
        descriptor::CODEX.vendor_args(Some(""), Some(""), "SID", None, None),
        descriptor::CODEX.vendor_args(None, None, "SID", None, None)
    );
}

/// CDX-FR-07 — a value that would break out of a quoted TOML override is refused
/// rather than interpolated.
///
/// Nothing composes these but the application's own descriptor, so this guards a
/// future identifier rather than a caller. It is still the difference between a
/// bad effort id being dropped and a bad effort id rewriting the sandbox policy.
#[test]
fn a_codex_override_value_cannot_break_out_of_its_quotes() {
    for hostile in [
        "high\" \nsandbox_mode=\"danger-full-access",
        "a\"b",
        "a\\b",
        "a\nb",
    ] {
        assert!(
            !descriptor::CODEX.effort_supported(hostile),
            "{hostile:?} should be refused"
        );
        let argv = descriptor::CODEX.vendor_args(None, Some(hostile), "SID", None, None);
        assert!(
            !argv.iter().any(|a| a.contains("sandbox_mode")),
            "{hostile:?} reached the vector and could rewrite configuration"
        );
    }
    // The identifiers the application actually offers all pass.
    for ok in ["low", "medium", "high"] {
        assert!(descriptor::CODEX.effort_supported(ok));
    }
}

/// EAC-FR-19 / EAC-FR-21 — the declarative positions each descriptor names match
/// the specification it transcribes.
#[test]
fn each_descriptor_declares_the_positions_its_specification_defines() {
    assert_eq!(
        descriptor::CLAUDE_CODE.response_sources,
        &[
            ResponseSource::JsonObjectField {
                key: "structured_output"
            },
            ResponseSource::JsonStringField { key: "result" },
        ]
    );
    assert_eq!(
        descriptor::CODEX.response_sources,
        &[ResponseSource::JsonlLastItem {
            event_type: "item.completed",
            item_type: "agent_message",
            text_key: "text",
        }]
    );

    // One vendor names its session before the run; the other can only report one.
    assert!(descriptor::CLAUDE_CODE.assigns_session_id());
    assert!(!descriptor::CODEX.assigns_session_id());

    // Both pinned vendors can resume, and both need state that outlives the
    // container to do it (EAC-FR-31).
    assert!(descriptor::CLAUDE_CODE.supports_resume());
    assert!(descriptor::CODEX.supports_resume());
    assert!(matches!(
        descriptor::CLAUDE_CODE.session_state,
        SessionStateMount::Dedicated { .. }
    ));
    assert!(matches!(
        descriptor::CODEX.session_state,
        SessionStateMount::SharedWithCredentialMount { .. }
    ));
}

/// A mount source Docker would misread is refused rather than passed through:
/// `--mount` is comma-separated, so a comma in a path would become a second
/// option.
#[test]
fn a_mount_source_that_docker_would_misread_is_refused() {
    assert!(descriptor::mount_source(Path::new("/tmp/plain")).is_some());
    assert!(descriptor::mount_source(Path::new("/tmp/with,comma")).is_none());
    assert!(descriptor::mount_source(Path::new("")).is_none());
}

/// EAC-FR-39, EAC-FR-17 — the image a container is created from is the one the **open
/// project commits** for the resolved vendor, used directly.
///
/// No shipped manifest is read, no digest is pinned by, and no fallback stands
/// behind an absent entry: a project that configures none launches nothing
/// (EAC-FR-38).
#[test]
fn the_container_is_created_from_the_image_the_project_commits() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());

    let executor = AgentCliExecutor::new(runtime.clone())
        .with_session_state_root(harness.sessions_root());
    let result = block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ));

    assert_eq!(
        result.expect("ran").process_outcome,
        ProcessOutcome::Completed
    );
    let asked = runtime.images_asked.lock().unwrap().clone();
    assert_eq!(
        asked,
        vec![test_image_reference("claude_code")],
        "the project's own reference, used directly",
    );
    // The shipped manifest took no part in it: neither its reference nor its
    // digest appears anywhere in the generated vector (AVI-FR-19, AVI-FR-07, AVI-FR-09).
    let entry = descriptor::manifest_entry("claude_code").expect("a manifest entry");
    let runs = runtime.runs.lock().unwrap().clone();
    assert!(
        !runs[0]
            .argv
            .iter()
            .any(|a| a.contains(&entry.image_ref) || a.contains(&entry.image_digest)),
        "no shipped image reference or digest reached the launch",
    );
}

/// EAC-FR-39, EAC-FR-17 — a project that configures no image for the resolved vendor is
/// `VendorImageUnresolved`, with no container created and no credential
/// requested (EAC-FR-38).
#[test]
fn a_project_that_configures_no_image_launches_nothing() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());

    let executor = AgentCliExecutor::new(runtime.clone())
        .with_session_state_root(harness.sessions_root());
    let context = LaunchContext {
        store: &harness.store,
        integrations: &harness.ai,
        project_key: PROJECT_KEY,
        fs: &harness.fs,
        images: &NoImages,
    };
    let error = block_on(executor.execute_agent_cli(
        &Silent,
        &context,
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .expect_err("a project that configures no image launches nothing");

    assert_eq!(error.kind(), "vendor_image_unresolved");
    assert!(
        matches!(error, AgentExecutionError::VendorImageUnresolved(_)),
        "{error:?}"
    );
    assert!(
        runtime.images_asked.lock().unwrap().is_empty(),
        "no image was asked for",
    );
    assert!(
        runtime.runs.lock().unwrap().is_empty(),
        "no container was created",
    );
}

/// EAC-FR-39, EAC-FR-17 — a configured image the backend cannot find or pull is
/// `ImageUnavailable`, which is the existing failure and not a new one
/// (EAC-FR-17).
#[test]
fn a_configured_image_that_cannot_be_pulled_is_image_unavailable() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::broken(true, false, None);

    let executor = AgentCliExecutor::new(runtime.clone())
        .with_session_state_root(harness.sessions_root());
    let error = block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .expect_err("an image nothing can supply refuses the launch");

    assert_eq!(error.kind(), "image_unavailable");
    assert!(
        runtime.runs.lock().unwrap().is_empty(),
        "no container was created",
    );
}

/// EAC-FR-39, EAC-FR-13, EAC-FR-17 — the seam receives the container specification, and the vector
/// the Docker CLI backend renders from it is the one the descriptors define
/// (EAC-FR-39).
#[test]
fn the_seam_receives_the_specification_the_cli_vector_is_rendered_from() {
    let harness = harness_for("codex");
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));

    let executor = AgentCliExecutor::new(runtime.clone())
        .with_session_state_root(harness.sessions_root());
    block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .expect("runs");

    let recorded = runtime.runs.lock().unwrap().clone();
    let spec = recorded[0]
        .container
        .clone()
        .expect("production always hands the seam a container specification");
    // The specification carries the project's image, the workspace user
    // mapping, and the least privileges — and the vector is exactly what it
    // renders to, so neither backend composes a parameter of its own.
    assert_eq!(spec.image, test_image_reference("codex"));
    assert_eq!(spec.dropped_capabilities(), vec!["ALL".to_string()]);
    assert_eq!(spec.security_options(), vec!["no-new-privileges".to_string()]);
    assert_eq!(
        recorded[0].argv,
        spec.docker_run_args(),
        "the rendered vector is the specification's own",
    );
}

/// EAC-FR-39, EAC-FR-13, EAC-FR-17 — an unverified Docker backend is `RuntimeUnavailable` before any
/// container is created, and nothing here verifies one (EAC-FR-39).
#[test]
fn an_unverified_docker_backend_refuses_before_any_container() {
    let harness = harness_for("claude_code");
    // The production binding, against a store whose Docker backend has never
    // verified — which is what a fresh machine holds (GSS-FR-35).
    let executor = AgentCliExecutor::default().with_session_state_root(harness.sessions_root());
    let error = block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: task("go"),
            cancellation: CancellationToken::new(),
            activity: None,
            supplementary_mount: None,
        },
    ))
    .expect_err("an unverified backend launches nothing");

    assert_eq!(error.kind(), "runtime_unavailable");
    // And the store is exactly as it was: resolution reads, and verifies
    // nothing (GSS-FR-40).
    assert_eq!(
        harness.store.load_docker_backend().unwrap().outbound().state,
        crate::docker::DockerBackendState::Unverified
    );
}
