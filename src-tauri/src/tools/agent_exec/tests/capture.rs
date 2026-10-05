//! EAC-FR-22 … EAC-FR-26 — the two outcome axes, stream capture, and how a run ends.

use super::*;

// ---------------------------------------------------------------------------
// EAC-FR-22 … EAC-FR-23 — the two axes, and stream capture
// ---------------------------------------------------------------------------

/// EAC-FR-22 — an agent-reported failure or escalation is a *completed*
/// execution, distinguishable from every executor failure.
#[test]
fn agent_reported_failure_and_escalation_are_completed_executions() {
    let harness = harness_for("claude_code");

    let runtime = RecordingRuntime::replying(&claude_stdout(&envelope_json("failure")));
    let outcome = run(&harness, runtime, task("go")).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    assert_eq!(outcome.exit_code, Some(0));
    let response = outcome.response.expect("envelope");
    assert_eq!(response.outcome, AgentOutcome::Failure);
    let failure = response.failure.expect("failure detail");
    assert_eq!(failure.code, "blocked");
    assert!(failure.retryable);

    let runtime = RecordingRuntime::replying(&claude_stdout(&envelope_json("escalation_required")));
    let outcome = run(&harness, runtime, task("go")).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    let escalation = outcome
        .response
        .expect("envelope")
        .escalation
        .expect("escalation detail");
    // EAC-FR-22 / EAC-FR-36: the question list reaches the caller in the order
    // it was written, with every option's three fields intact.
    assert_eq!(escalation.questions.len(), 1);
    assert_eq!(escalation.questions[0].question, "which one?");
    assert_eq!(
        escalation.questions[0]
            .options
            .iter()
            .map(|option| option.answer.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(escalation.questions[0].options[0].summary, "Take a");
    assert_eq!(
        escalation.questions[0].options[1].description,
        "The second path."
    );
}

/// EAC-FR-22, EAC-FR-23 — a non-zero exit keeps its stderr and parses nothing.
#[test]
fn a_non_zero_exit_preserves_stderr_and_parses_no_envelope() {
    let harness = harness_for("claude_code");
    let runtime = Arc::new(RecordingRuntime {
        script: Script::Reply {
            stdout: String::new(),
            stderr: "authentication failed\n".into(),
            exit_code: 1,
        },
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails: false,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
    });

    let outcome = run(&harness, runtime, task("go")).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::NonZeroExit);
    assert_eq!(outcome.exit_code, Some(1));
    assert!(outcome.response.is_none());
    assert!(outcome.stdout.bytes.is_empty());
    assert_eq!(outcome.stderr.bytes, b"authentication failed\n");
}

/// EAC-FR-13 — the invocation attaches stdin and allocates no terminal.
///
/// Named here rather than left as two elements of a thirty-element vector.
/// `claude_codes_generated_invocation_is_exactly_the_descriptor` pins the whole
/// ordered vector and would notice either flag moving, but it states "this is
/// the vector" and not "the task can reach the agent" — and the second is the
/// requirement, for both vendors, that survives a future reordering.
#[test]
fn the_invocation_attaches_stdin_and_allocates_no_terminal() {
    for vendor in ["claude_code", "codex"] {
        let harness = harness_for(vendor);
        let stdout = match vendor {
            "claude_code" => valid_claude_stdout(),
            _ => codex_stdout(&envelope_json("success")),
        };
        let runtime = RecordingRuntime::replying(&stdout);
        run(&harness, runtime.clone(), task("go")).expect("ran");
        let argv = runtime.only_run().argv;

        // EAC-FR-09's payload reaches a container that asked for stdin, or it
        // reaches nobody: `docker run` gives `/dev/null` to one that did not.
        assert!(
            argv.iter().any(|a| a == "--interactive"),
            "{vendor}: the task would be written to a stream nothing reads"
        );
        // And no pseudo-terminal, which would invite the CLI to write progress
        // decoration into the stream EAC-FR-19 parses for an envelope.
        assert!(
            !argv.iter().any(|a| a == "--tty" || a == "-t"),
            "{vendor}: a terminal was allocated"
        );
    }
}

/// EAC-FR-29 — a rejected envelope's record carries the reason and the excerpt
/// together.
///
/// The outcome a reader has to combine two fields to understand: `reason` says
/// what was wrong with what the agent wrote, and the excerpt says what the CLI
/// itself was complaining about while it wrote it. Neither alone identifies a
/// vendor that answered in prose because it was failing for its own reasons.
#[test]
fn an_invalid_envelope_record_carries_both_the_reason_and_the_excerpt() {
    let harness = harness_for("claude_code");
    let runtime = Arc::new(RecordingRuntime {
        script: Script::Reply {
            stdout: "not an envelope at all".into(),
            stderr: "MARKER-INVALID-f66b: the model is unavailable\n".into(),
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

    let outcome = run(&harness, runtime.clone(), task("go")).expect("ran");
    assert_eq!(
        outcome.process_outcome,
        ProcessOutcome::InvalidStructuredOutput
    );

    let record = failure_record(&container_name(&runtime.only_run().argv));
    assert_eq!(
        record.fields["outcome"],
        json!("invalid_structured_output")
    );
    // Both, on one record.
    assert!(
        record.fields["reason"].is_string(),
        "the record does not say why the output was rejected"
    );
    assert_eq!(
        record.fields["stderr_excerpt"],
        json!("MARKER-INVALID-f66b: the model is unavailable")
    );
    // A clean exit that still failed: the exit status alone would have read as
    // success, which is why the outcome is on its own axis.
    assert_eq!(record.fields["exit_code"], json!(0));
}

/// EAC-FR-29 — the stderr excerpt is bounded, masked, and honest about being a
/// tail.
///
/// Exercised directly rather than only through a launch: a launch can show the
/// excerpt arriving, but the cases that matter here are the ones a real stderr
/// reaches rarely and a leak would hide in — a credential split by either of the
/// two cuts, a multi-byte character straddling one, bytes that are not text at
/// all.
#[test]
fn the_stderr_excerpt_is_bounded_masked_and_marked_when_it_is_a_tail() {
    // Nothing to say adds no excerpt, rather than an empty string a reader has
    // to interpret.
    assert_eq!(stderr_excerpt(b"", false, &[]), None);
    assert_eq!(stderr_excerpt(b"   \n\t\n", false, &[]), None);

    // Short enough to carry whole, and carried whole — no marker, because there
    // is nothing above it that was dropped.
    assert_eq!(
        stderr_excerpt(b"Error: refused\n", false, &[]),
        Some("Error: refused".to_string())
    );

    // Every occurrence, not the first.
    assert_eq!(
        stderr_excerpt(b"tok=abc123 again abc123", false, &["abc123"]),
        Some(format!("tok={REDACTED} again {REDACTED}"))
    );
    // An empty secret matches at every position and would replace nothing
    // usefully, so it is ignored rather than allowed to shred the text.
    assert_eq!(
        stderr_excerpt(b"plain text", false, &["", "absent"]),
        Some("plain text".to_string())
    );

    // Over the bound: the tail is what is kept, the head is what is dropped, and
    // the marker says so. The head is marked, so "kept the tail" is asserted by
    // what is absent rather than only by a length.
    let long = format!(
        "HEAD-MARKER{}LAST-LINE",
        "x".repeat(LIMIT_STDERR_EXCERPT * 2)
    );
    let excerpt = stderr_excerpt(long.as_bytes(), false, &[]).expect("an excerpt");
    assert!(excerpt.starts_with('…'), "a tail did not say it was one");
    assert!(excerpt.ends_with("LAST-LINE"), "the tail was not the tail");
    assert!(
        !excerpt.contains("HEAD-MARKER"),
        "the whole text was returned with a marker glued to it"
    );
    assert!(
        excerpt.len() <= LIMIT_STDERR_EXCERPT + '…'.len_utf8(),
        "the excerpt outgrew its bound"
    );

    // The bound itself, from both sides. This is what catches the marker being
    // applied a byte early or `saturating_sub` becoming a comparison.
    for (size, marked) in [
        (LIMIT_STDERR_EXCERPT - 1, false),
        (LIMIT_STDERR_EXCERPT, false),
        (LIMIT_STDERR_EXCERPT + 1, true),
    ] {
        let excerpt = stderr_excerpt(&b"q".repeat(size), false, &[]).expect("an excerpt");
        assert_eq!(
            excerpt.starts_with('…'),
            marked,
            "a {size}-byte stderr was marked as a tail: {}",
            excerpt.starts_with('…')
        );
    }

    // The case masking-after-cutting gets wrong: a credential lying *across*
    // the excerpt cut is a whole value in neither half, so a mask applied to the
    // tail alone has nothing to match and the end of the token rides into the
    // record. Placed so that the cut falls inside it, and asserted on the
    // fragments rather than only on the whole value — the whole value is the one
    // thing that would still be absent either way.
    let secret = "sk-ant-oat01-SECRETVALUE";
    let head = &secret[..12];
    let tail = &secret[12..];
    let mut straddling = "y".repeat(10);
    straddling.push_str(secret);
    straddling.push_str(&"t".repeat(LIMIT_STDERR_EXCERPT - 12));
    let excerpt = stderr_excerpt(straddling.as_bytes(), false, &[secret]).expect("an excerpt");
    assert!(!excerpt.contains(secret), "the credential survived whole");
    assert!(
        !excerpt.contains(tail),
        "the end of a credential across the excerpt cut reached the record"
    );
    assert!(
        !excerpt.contains(head),
        "the start of a credential across the excerpt cut reached the record"
    );

    // The same hazard one cut earlier, which masking cannot answer at all.
    // Capture keeps the *first* `LIMIT_STDERR` bytes, so a truncated stream ends
    // mid-line and a credential lying across that cut is a whole value nowhere
    // in what was captured. The incomplete final line goes whole.
    let truncated_capture = format!("first line\nsecond line\n{head}");
    let excerpt =
        stderr_excerpt(truncated_capture.as_bytes(), true, &[secret]).expect("an excerpt");
    assert!(
        !excerpt.contains(head),
        "half a credential survived the capture bound"
    );
    assert!(
        excerpt.ends_with("second line"),
        "the complete lines above the capture cut were dropped too"
    );
    // A truncated capture that never reached a line ending is one incomplete
    // line, and there is nothing left of it to report once it is dropped.
    assert_eq!(stderr_excerpt(head.as_bytes(), true, &[secret]), None);

    // A multi-byte character straddling the excerpt cut is dropped rather than
    // panicking on the slice. The cut has to land *inside* the character for
    // this to exercise anything, which means putting it near the head: the cut
    // sits `len - LIMIT_STDERR_EXCERPT` bytes in.
    let mut multibyte = String::from("HEAD");
    multibyte.push('é');
    multibyte.push_str(&"z".repeat(LIMIT_STDERR_EXCERPT - 1));
    assert_eq!(
        multibyte.len() - LIMIT_STDERR_EXCERPT,
        5,
        "the cut no longer lands inside the multi-byte character"
    );
    let excerpt = stderr_excerpt(multibyte.as_bytes(), false, &[]).expect("an excerpt");
    assert!(
        !excerpt.contains('é') && !excerpt.contains('\u{FFFD}'),
        "the cut landed mid-character and the excerpt shows it"
    );
    assert!(
        excerpt.ends_with(&"z".repeat(LIMIT_STDERR_EXCERPT - 1)),
        "advancing past the character took more than the character"
    );

    // Masking can make text longer than it was — `<redacted>` is wider than a
    // short secret — so a stderr that fitted whole can leave as a marked tail.
    // The bound is on what is stored, which is the decision being pinned here.
    let grown = format!("{}abc", "w".repeat(LIMIT_STDERR_EXCERPT - 3));
    let excerpt = stderr_excerpt(grown.as_bytes(), false, &["abc"]).expect("an excerpt");
    assert!(
        excerpt.starts_with('…') && excerpt.ends_with(REDACTED),
        "masking grew the text and the excerpt did not stay inside its bound"
    );

    // Bytes that are not text at all are lossily decoded rather than dropping
    // the diagnostic or panicking on the way to a log record.
    let excerpt = stderr_excerpt(&[0xFF, 0xFE, b'o', b'k'], false, &[]).expect("an excerpt");
    assert!(excerpt.ends_with("ok"));
}

/// EAC-FR-29 — a Codex failure's excerpt masks the credential *path*.
///
/// This vendor's credential is a directory rather than an environment value, so
/// nothing in the launch's environment names it and a mask built from that
/// environment alone would let the path through. The CLI names the directory in
/// its own diagnostics whenever the login there is missing or stale, which is
/// exactly the failure a reader most often reaches this record for.
#[test]
fn a_codex_failure_excerpt_carries_no_credential_path() {
    let harness = harness_for("codex");
    // The container path the CLI knows the directory by, and the host path a
    // Docker diagnostic would name it by.
    let container_path = "/home/agent/.codex";
    let host_path = harness
        .home
        .path()
        .join(".codex")
        .to_string_lossy()
        .into_owned();

    let runtime = Arc::new(RecordingRuntime {
        script: Script::Reply {
            stdout: String::new(),
            stderr: format!(
                "MARKER-CODEX-e55a: no login found at {container_path} (mounted from {host_path})\n"
            ),
            exit_code: 1,
        },
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails: false,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
    });

    let outcome = run(&harness, runtime.clone(), task("go")).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::NonZeroExit);

    // This launch's own record, not a scan of a buffer every other test in the
    // file writes to — a `<redacted>` another test produced would satisfy a scan
    // without this launch having masked anything.
    let record = failure_record(&container_name(&runtime.only_run().argv));
    let excerpt = record.fields["stderr_excerpt"]
        .as_str()
        .expect("an excerpt")
        .to_string();

    // The diagnostic itself is what makes the record worth reading.
    assert!(
        excerpt.contains("MARKER-CODEX-e55a"),
        "the excerpt did not reach the failure record"
    );
    // And neither spelling of the credential directory rode in with it.
    assert!(
        !excerpt.contains(container_path),
        "the excerpt carried the Codex configuration path"
    );
    assert!(
        !excerpt.contains(host_path.as_str()),
        "the excerpt carried the host path the login was mounted from"
    );
}

/// EAC-FR-23 — bounded capture, kept separate, and a truncated stdout is never
/// parsed.
#[test]
fn streams_are_bounded_separate_and_a_truncated_stdout_is_not_parsed() {
    let harness = harness_for("claude_code");

    let flooded_out = Arc::new(RecordingRuntime {
        script: Script::Flood { stream: "stdout" },
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails: false,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
    });
    let outcome = run(&harness, flooded_out, task("go")).expect("ran");
    assert!(outcome.stdout.truncated);
    assert!(outcome.stderr.bytes.is_empty());
    assert_eq!(
        outcome.process_outcome,
        ProcessOutcome::InvalidStructuredOutput,
        "a prefix is never parsed"
    );
    assert!(outcome.response.is_none());

    let flooded_err = Arc::new(RecordingRuntime {
        script: Script::Flood { stream: "stderr" },
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails: false,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
    });
    let outcome = run(&harness, flooded_err, task("go")).expect("ran");
    assert!(outcome.stderr.truncated);
    assert!(outcome.stdout.bytes.is_empty(), "streams never merge");
}

/// The bounded reader itself, over a real pipe: past the bound the stream is
/// still drained, so the child is never blocked by a caller that stopped
/// caring (EAC-FR-23).
#[test]
fn bounded_capture_drains_past_its_limit_over_a_real_pipe() {
    let sh = HostDockerCli::with_program("sh");
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &[
                "-c".to_string(),
                "printf 'x%.0s' $(seq 1 20000); printf 'e%.0s' $(seq 1 500) >&2".to_string(),
            ],
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_secs(20),
            cancel: CancellationToken::new(),
            stdout_limit: 1000,
            stderr_limit: 100,
            observer: None,
        })
        .await
    })
    .expect("ran");

    assert_eq!(outcome.end, RunEnd::Exited);
    assert_eq!(outcome.stdout.bytes.len(), 1000);
    assert!(outcome.stdout.truncated);
    assert_eq!(outcome.stderr.bytes.len(), 100);
    assert!(outcome.stderr.truncated);
    assert!(outcome.stdout.bytes.iter().all(|b| *b == b'x'));
    assert!(outcome.stderr.bytes.iter().all(|b| *b == b'e'));
}

// ---------------------------------------------------------------------------
// EAC-FR-24, EAC-FR-25, EAC-FR-26 … EAC-FR-26 — ending a run
// ---------------------------------------------------------------------------

/// EAC-FR-24, EAC-FR-25, EAC-FR-26 — timeout and cancellation, over a real child process.
#[test]
fn a_hanging_child_is_killed_by_the_deadline_and_by_cancellation() {
    // "Killed, not waited out" is read off a marker the child would create on
    // the far side of its sleep, not off a stopwatch. A wall-clock bound answers
    // a different question on every machine — and on a loaded CI runner it
    // answers it wrongly — while the marker's absence means the same thing at
    // any speed: the 60 seconds never elapsed. Same device as
    // `a_timeout_kills_the_grandchildren_too` below.
    let marker = tempfile::tempdir().expect("dir");
    let deadline_flag = marker.path().join("deadline-survived");
    let cancel_flag = marker.path().join("cancel-survived");

    let sh = HostDockerCli::with_program("sh");
    let hang = [
        "-c".to_string(),
        format!("sleep 60; touch {}", deadline_flag.to_string_lossy()),
    ];
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &hang,
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_millis(300),
            cancel: CancellationToken::new(),
            stdout_limit: 1024,
            stderr_limit: 1024,
            observer: None,
        })
        .await
    })
    .expect("ran");
    assert_eq!(outcome.end, RunEnd::TimedOut);
    assert!(outcome.exit_code.is_none());
    assert!(
        !deadline_flag.exists(),
        "the child outlived the deadline that was supposed to kill it"
    );

    // The same child, ended by the caller instead.
    let hang = [
        "-c".to_string(),
        format!("sleep 60; touch {}", cancel_flag.to_string_lossy()),
    ];
    let token = CancellationToken::new();
    let flag = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        flag.cancel();
    });
    let outcome = block_on(async {
        sh.run(RunRequest {
            argv: &hang,
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_secs(30),
            cancel: token,
            stdout_limit: 1024,
            stderr_limit: 1024,
            observer: None,
        })
        .await
    })
    .expect("ran");
    assert_eq!(outcome.end, RunEnd::Cancelled);
    assert!(
        !cancel_flag.exists(),
        "the child outlived the cancellation that was supposed to kill it"
    );
}

/// EAC-FR-24 — a complete envelope followed by a hang is still a timeout: a
/// partial run is never promoted to a success.
#[test]
fn a_complete_envelope_before_a_timeout_is_still_a_timeout() {
    let harness = harness_for("claude_code");
    let runtime = Arc::new(RecordingRuntime {
        script: Script::TimeOut,
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails: false,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
    });

    let outcome = run(&harness, runtime, task("go")).expect("ran");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Timeout);
    assert!(outcome.response.is_none(), "never promoted to a success");
    // The bytes that did arrive are still handed back for the caller to read.
    assert!(!outcome.stdout.bytes.is_empty());
}

/// EAC-FR-26 — every path out removes its container, and no name is reused.
#[test]
fn every_path_out_removes_its_container_under_a_fresh_name() {
    let harness = harness_for("claude_code");
    let mut names: Vec<String> = Vec::new();

    let scripts = [
        Script::Reply {
            stdout: valid_claude_stdout(),
            stderr: String::new(),
            exit_code: 0,
        },
        Script::Reply {
            stdout: String::new(),
            stderr: "boom".into(),
            exit_code: 3,
        },
        Script::Reply {
            stdout: "not json".into(),
            stderr: String::new(),
            exit_code: 0,
        },
        Script::TimeOut,
        Script::Cancel,
        Script::Terminate,
    ];

    for script in scripts {
        let runtime = Arc::new(RecordingRuntime {
            script,
            available: true,
            image_available: true,
            launch_error: None,
            remove_fails: false,
            runs: StdMutex::new(Vec::new()),
            removed: StdMutex::new(Vec::new()),
            images_asked: StdMutex::new(Vec::new()),
        });
        run(&harness, runtime.clone(), task("go")).expect("ran");

        let removed = runtime.removed.lock().unwrap().clone();
        assert_eq!(removed.len(), 1, "exactly one removal per run");
        let launched_name = container_name(&runtime.only_run().argv);
        assert_eq!(removed[0], launched_name, "the container that ran is the one removed");
        names.push(launched_name);
    }

    let unique: std::collections::HashSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "no container name is reused");
}
