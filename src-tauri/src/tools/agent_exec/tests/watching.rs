//! EAC-FR-32 … EAC-FR-34 — watching a run happen, and what a watcher may not be handed.

use super::*;

// ---------------------------------------------------------------------------
// EAC-FR-33 … EAC-FR-32 — watching a run happen (EAC-FR-32 … EAC-FR-34)
// ---------------------------------------------------------------------------


/// EAC-FR-32, EAC-FR-33, EAC-FR-34, EAC-FR-THUT — a watched run reports its invocation, its
/// task, every line the CLI wrote, and the executor's closing line, in that order.
///
/// The order is the assertion, not a detail: what was asked and how it was asked
/// are the first two questions anybody debugging a turn has, and neither is
/// recoverable afterwards — the container is gone and the argv was never written
/// down anywhere else.
#[test]
fn a_watched_run_reports_its_invocation_its_task_and_every_line() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let sink = Arc::new(CollectedActivity::default());

    const WATCHED_INSTRUCTION: &str = "MARKER-WATCHED-b71d";
    let outcome = run_watching(
        &harness,
        runtime.clone(),
        task(WATCHED_INSTRUCTION),
        sink.clone(),
    )
    .expect("runs");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);

    let events = sink.events();
    assert_eq!(events[0].kind, "invocation");
    assert_eq!(events[0].channel, "executor");
    assert_eq!(events[1].kind, "task");
    assert_eq!(events[1].channel, "executor");

    // The invocation carries the whole generated vector, so a reader can see
    // which image, which mounts, and which flags this turn actually ran with.
    assert!(events[0].payload.contains("--interactive"));
    assert!(events[0].payload.contains("--output-format stream-json"));
    assert!(events[0].payload.contains(&container_name(&runtime.only_run().argv)));
    // And the task carries the document the agent was handed, verbatim.
    assert!(events[1].payload.contains(WATCHED_INSTRUCTION));
    let (sent, contract) = split_task_document(events[1].payload.as_bytes());
    assert_eq!(sent.instruction, WATCHED_INSTRUCTION);
    assert_eq!(contract, *RESPONSE_CONTRACT);

    // Then the CLI's own stream, normalized — the fixture is a stream, so this
    // is three events rather than one document — and last the executor's own
    // line that says how the run ended (EAC-FR-THUT).
    let kinds = sink.kinds();
    assert_eq!(
        &kinds[2..],
        ["started", "message", "finished", "finished"],
        "the vendor's events were not read: {kinds:?}"
    );
    let closing = events.last().expect("a closing event");
    assert_eq!(closing.channel, "executor");
    assert!(
        closing.summary.starts_with("Turn finished after "),
        "{}",
        closing.summary
    );
    let started = &events[2];
    assert_eq!(started.channel, "stdout");
    assert!(started.summary.contains("claude-sonnet-5"));
    assert!(events[3].summary.contains("working on it"));
}

/// EAC-FR-32 — an unwatched run behaves exactly as it did before anybody could
/// watch one.
///
/// The seam is an addition, not a substitution: what a caller gets back, and
/// what the runtime was asked for, must not depend on whether somebody is
/// looking.
#[test]
fn an_unwatched_run_is_the_same_run() {
    let harness = harness_for("claude_code");

    let watched_runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let watched = run_watching(
        &harness,
        watched_runtime.clone(),
        task("go"),
        Arc::new(CollectedActivity::default()),
    )
    .expect("runs");

    let plain_runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let plain = run(&harness, plain_runtime.clone(), task("go")).expect("runs");

    assert_eq!(watched.process_outcome, plain.process_outcome);
    assert_eq!(watched.response, plain.response);
    assert_eq!(watched.stdout.truncated, plain.stdout.truncated);

    // Both the vector and the captured stream differ only where they are
    // generated to differ: the container name and the session identity are
    // unique per launch (EAC-FR-13, EAC-FR-21), and the fixture echoes the
    // session back. Normalising those two is what leaves a comparison of
    // everything else.
    let normalise = |text: &str, argv: &[String]| {
        let name = container_name(argv);
        let session = assigned_session_id(argv).unwrap_or_default();
        text.replace(&name, "<name>").replace(&session, "<session>")
    };
    let watched_argv = watched_runtime.only_run().argv;
    let plain_argv = plain_runtime.only_run().argv;
    assert_eq!(
        normalise(&watched_argv.join("\u{1}"), &watched_argv),
        normalise(&plain_argv.join("\u{1}"), &plain_argv)
    );
    assert_eq!(
        normalise(&String::from_utf8_lossy(&watched.stdout.bytes), &watched_argv),
        normalise(&String::from_utf8_lossy(&plain.stdout.bytes), &plain_argv)
    );
}

/// EAC-FR-29, EAC-FR-32 — nothing a record may not carry reaches a watcher either.
///
/// The observer is a second path out of the executor, and EAC-FR-29 is about
/// every path rather than about the log. A launch's own credential, the session
/// identities it generated, and — for the vendor whose credential is a directory
/// — that directory's two spellings all have to be masked here as well.
#[test]
fn a_watcher_is_handed_nothing_a_record_may_not_carry() {
    let harness = harness_for("claude_code");
    // The CLI printing its own environment back is the case this exists for: a
    // debug dump, an error naming the variable it read, a stack trace carrying
    // the argv.
    let leaky = format!(
        "{}\n{}\n",
        json!({
            "type": "system", "subtype": "init", "model": "m", "tools": [],
            "session_id": SESSION_PLACEHOLDER,
            "env": { "CLAUDE_CODE_OAUTH_TOKEN": SAMPLE_TOKEN }
        }),
        claude_result_line(&envelope_json("success")),
    );
    let runtime = RecordingRuntime::replying(&leaky);
    let sink = Arc::new(CollectedActivity::default());

    run_watching(&harness, runtime.clone(), task("go"), sink.clone()).expect("runs");

    let rendered = sink.rendered();
    assert!(
        !rendered.contains(SAMPLE_TOKEN),
        "the credential reached a watcher whole"
    );
    assert!(
        !rendered.contains(&SAMPLE_TOKEN[..20]),
        "a recognisable piece of the credential reached a watcher"
    );
    // Masked rather than removed: the fingerprint is what lets a reader tell
    // which token ran without being handed one.
    assert!(
        rendered.contains(&masked_form(SAMPLE_TOKEN)),
        "the credential was not masked in place: {rendered}"
    );
    // The session the executor assigned is masked too, on every path.
    let assigned = assigned_session_id(&runtime.only_run().argv).expect("--session-id");
    assert!(!rendered.contains(&assigned));
}

/// EAC-FR-32 — watching a run changes nothing about the run, at the real seam.
///
/// The double computes its outcome and then replays lines into the observer, so
/// it is structurally incapable of showing interference. This drives the actual
/// pipe pump twice over the same child — once watched, once not — which is where
/// an observer could genuinely cost the capture bytes, an exit status, or a
/// truncation mark.
#[test]
fn watching_a_run_does_not_change_what_the_capture_holds() {
    #[derive(Default)]
    struct Counting(StdMutex<usize>);
    impl StreamObserver for Counting {
        fn line(&self, _channel: StreamChannel, bytes: &[u8], _whole: bool) {
            if !bytes.is_empty() {
                *self.0.lock().unwrap() += 1;
            }
        }
    }

    let script = "printf 'one\ntwo\nthree'; printf 'a warning\n' 1>&2; exit 3";
    let runtime = HostDockerCli::with_program("/bin/sh");
    let run = |observer: Option<&dyn StreamObserver>| {
        block_on(runtime.run(RunRequest {
            argv: &["-c".to_string(), script.to_string()],
            container: None,
            env: &BTreeMap::new(),
            stdin: b"",
            timeout: Duration::from_secs(20),
            cancel: CancellationToken::new(),
            stdout_limit: 4096,
            stderr_limit: 4096,
            observer,
        }))
        .expect("the child ran")
    };

    let observer = Counting::default();
    let watched = run(Some(&observer));
    let unwatched = run(None);

    assert_eq!(watched.stdout, unwatched.stdout);
    assert_eq!(watched.stderr, unwatched.stderr);
    assert_eq!(watched.exit_code, unwatched.exit_code);
    assert_eq!(watched.end, unwatched.end);
    assert_eq!(watched.exit_code, Some(3));
    // And the watcher did hear, so the comparison is between a watched run and
    // an unwatched one rather than between two unwatched ones.
    assert_eq!(*observer.0.lock().unwrap(), 4);
}

/// EAC-FR-29, EAC-FR-32, the vendor whose credential is a directory — its two spellings
/// are masked on the *sink* as well as in the failure record.
///
/// The sink is the newer of the two paths out of this tool, and the one that
/// carries far more: a whole run's output rather than one bounded excerpt of a
/// failure. A mask applied to the record alone would be a mask on the quieter
/// path only.
#[test]
fn a_watcher_of_a_codex_run_is_handed_neither_spelling_of_its_login_directory() {
    let harness = harness_for("codex");
    let container_path = "/home/agent/.codex";
    let host_path = harness
        .home
        .path()
        .join(".codex")
        .to_string_lossy()
        .into_owned();

    // The CLI naming its own login directory is not hypothetical: it is what
    // this vendor prints whenever the login there is missing or stale, which is
    // the failure a reader most often opens this stream for.
    let runtime = RecordingRuntime::refused(
        &format!(
            "{}\n",
            json!({
                "type": "error",
                "message": format!("no login at {container_path} (mounted from {host_path})")
            })
        ),
        &format!("also on stderr: {container_path}\n"),
        1,
    );
    let sink = Arc::new(CollectedActivity::default());

    run_watching(&harness, runtime, task("go"), sink.clone()).expect("the launch happened");

    let rendered = sink.rendered();
    assert!(
        rendered.contains("no login at"),
        "the diagnostic did not reach the watcher: {rendered}"
    );
    assert!(
        !rendered.contains(container_path),
        "a watcher was handed the Codex configuration path"
    );
    assert!(
        !rendered.contains(host_path.as_str()),
        "a watcher was handed the host path the login was mounted from"
    );
    // Masked in place rather than removed, on both channels.
    assert!(rendered.contains(&masked_form(container_path)));
    assert!(
        sink.events()
            .iter()
            .any(|e| e.channel == "stderr" && e.payload.contains(&masked_form(container_path))),
        "the stderr channel was not masked"
    );
}

/// EAC-FR-29 — the two events the executor writes about itself are masked in
/// their summary as well as in their payload.
///
/// A summary is a shortened line rather than a different one, so what a payload
/// may not carry a summary may not carry either. The rule needs its own test
/// because a summary is cut at 300 characters: a mask applied after the cut
/// still hides a credential that sits past it, which makes a run on a machine
/// with long temporary paths pass and the same run on a machine with short ones
/// leak.
#[test]
fn the_executors_own_events_are_masked_in_their_summary_as_well_as_their_payload() {
    // Codex, because its credential is a path in the invocation itself rather
    // than an environment name — the argv is where this vendor can leak.
    let harness = harness_for("codex");
    let host_path = harness
        .home
        .path()
        .join(".codex")
        .to_string_lossy()
        .into_owned();
    let sink = Arc::new(CollectedActivity::default());
    run_watching(
        &harness,
        RecordingRuntime::replying(&codex_stdout(&envelope_json("success"))),
        task("go"),
        sink.clone(),
    )
    .expect("runs");

    let events = sink.events();
    let invocation = &events[0];
    assert_eq!(invocation.kind, "invocation");
    // The summary is the payload, shortened — the same text through the same
    // rule, rather than a second rendering that masks on its own schedule.
    // The payload is the whole masked argv here, which is what makes the
    // equality meaningful: a cut payload would fail this as an inequality
    // rather than as the masking claim it is meant to be.
    assert!(
        !invocation.payload_truncated,
        "the invocation was cut, so the summary cannot be read against it"
    );
    assert_eq!(
        invocation.summary,
        descriptor::summary_line(&format!(
            "{} {}",
            descriptor::DOCKER_PROGRAM, invocation.payload
        )),
        "the invocation summary is not the masked invocation, shortened"
    );
    assert!(
        !invocation.payload.contains(&host_path) && !invocation.summary.contains(&host_path),
        "the login directory reached the invocation event"
    );

    // And the task event, where the credential a summary can carry is whatever
    // the caller put in its instruction.
    let harness = harness_for("claude_code");
    let sink = Arc::new(CollectedActivity::default());
    run_watching(
        &harness,
        RecordingRuntime::replying(&valid_claude_stdout()),
        task(&format!("resume with {SAMPLE_TOKEN} please")),
        sink.clone(),
    )
    .expect("runs");

    let events = sink.events();
    let task_event = &events[1];
    assert_eq!(task_event.kind, "task");
    assert!(
        !task_event.summary.contains(SAMPLE_TOKEN),
        "a credential reached the task summary"
    );
    assert!(
        task_event.summary.contains(&masked_form(SAMPLE_TOKEN)),
        "the task summary dropped the instruction instead of masking it: {}",
        task_event.summary
    );
}

/// EAC-FR-29 — masking happens before a summary is cut, wherever in the text
/// the credential sits.
///
/// The rule as arithmetic, with no launch and no temporary directory in it.
/// The test above it can only see a credential the 300-character cut happens to
/// reach, so on a machine whose paths are long it proves the rule for the
/// positions it reaches and nothing for the rest — which is exactly how the
/// leak this pins survived a green suite. Here the position is the parameter:
/// a value in front of the cut, one straddling it, and one far past it are each
/// asserted. Cutting before masking fails the offsets that fall inside the
/// line, because the line then carries the value itself where it should carry
/// the fingerprint; the offsets past the cut hold the other half of the rule,
/// that a value the line does not reach leaves no piece of itself behind.
#[test]
fn a_summary_is_cut_from_masked_text_wherever_the_credential_sits() {
    let fingerprint = masked_form(SAMPLE_TOKEN);
    for offset in [0, 150, 299, 300, 301, 5_000] {
        // Filler without a space in it: `summary_line` collapses runs of
        // whitespace, and a collapsing filler would move the credential to an
        // offset other than the one under test.
        let text = format!("{}{SAMPLE_TOKEN} tail", "x".repeat(offset));
        let summary = descriptor::summary_line(&mask_secrets(&text, &[SAMPLE_TOKEN]));
        assert!(
            !summary.contains(SAMPLE_TOKEN),
            "a credential at offset {offset} survived the summary"
        );
        // And the mask is a mask rather than the cut doing the work: where the
        // fingerprint fits inside the line, it is there in full.
        if offset + fingerprint.chars().count() <= descriptor::LIMIT_SUMMARY_LINE {
            assert!(
                summary.contains(&fingerprint),
                "a credential at offset {offset} was dropped instead of masked"
            );
        }
    }
}

/// EAC-FR-32, EAC-FR-33 — stderr reaches a watcher as diagnostics, on its own channel.
///
/// Both pinned protocols define stderr as diagnostics and nothing else
/// (CCP-FR-14, CDX-FR-18), so it is never read for an envelope and never
/// normalized against a vendor's event vocabulary — but it is exactly where a
/// CLI says why it refused, which is what a reader is here for.
#[test]
fn stderr_reaches_a_watcher_as_diagnostics_on_its_own_channel() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::refused(
        "",
        "Error: input must be provided on stdin\nsecond line\n",
        1,
    );
    let sink = Arc::new(CollectedActivity::default());

    let outcome =
        run_watching(&harness, runtime, task("go"), sink.clone()).expect("the launch happened");
    assert_eq!(outcome.process_outcome, ProcessOutcome::NonZeroExit);

    let diagnostics: Vec<_> = sink
        .events()
        .into_iter()
        .filter(|e| e.channel == "stderr")
        .collect();
    assert_eq!(diagnostics.len(), 2, "both lines, separately");
    assert!(diagnostics.iter().all(|e| e.kind == "diagnostic"));
    assert!(diagnostics[0].summary.contains("must be provided on stdin"));
    // This is the sentence the run used to fail without: a turn that exits
    // non-zero having said exactly why, where the reason reached nobody.
    assert!(diagnostics[0].payload.contains("must be provided on stdin"));
}

/// EAC-FR-33, EAC-FR-THUT — an event this build cannot read is reported rather than dropped.
///
/// A vendor that adds an event, and a line that is not JSON at all, both have to
/// reach a reader: an upgrade that made a run go quiet would be indistinguishable
/// from a run that had nothing to say.
#[test]
fn an_unreadable_line_is_reported_rather_than_dropped() {
    let harness = harness_for("claude_code");
    let stream = format!(
        "{}\n{}\n{}\n",
        "not json at all",
        json!({ "type": "some_future_event", "detail": "MARKER-FUTURE" }),
        claude_result_line(&envelope_json("success")),
    );
    let runtime = RecordingRuntime::replying(&stream);
    let sink = Arc::new(CollectedActivity::default());

    run_watching(&harness, runtime, task("go"), sink.clone()).expect("runs");

    let kinds = sink.kinds();
    assert_eq!(&kinds[2..], ["unrecognized", "unrecognized", "finished", "finished"]);
    let rendered = sink.rendered();
    assert!(rendered.contains("not json at all"));
    assert!(rendered.contains("MARKER-FUTURE"));
}

/// EAC-FR-32 — the two events the executor writes are bounded like the rest.
#[test]
fn an_oversized_event_is_cut_and_says_so() {
    // Well past the bound, so the cut is unambiguous.
    let long = "M".repeat(protocol::LIMIT_ACTIVITY_EVENT * 2);
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let sink = Arc::new(CollectedActivity::default());

    run_watching(&harness, runtime, task(&long), sink.clone()).expect("runs");

    let task_event = sink
        .events()
        .into_iter()
        .find(|e| e.kind == "task")
        .expect("a task event");
    assert!(task_event.payload_truncated, "an oversized event was not marked");
    // Cut *at* the bound rather than discarded: an upper bound alone is
    // satisfied by an empty string, which is the failure that would leave a
    // reader with a mark saying something was cut and nothing to read.
    assert!(task_event.payload.len() <= protocol::LIMIT_ACTIVITY_EVENT);
    assert!(
        task_event.payload.len() > protocol::LIMIT_ACTIVITY_EVENT - 4,
        "the event was discarded rather than cut: {} bytes",
        task_event.payload.len()
    );
    assert!(task_event.payload.contains("MMMM"), "the payload is not the task");
    // The summary is bounded on its own terms, and is never the whole payload.
    assert!(task_event.summary.chars().count() <= descriptor::LIMIT_SUMMARY_LINE + 1);
}

/// EAC-FR-29, EAC-FR-32 — a credential split across a delivery boundary is still masked.
///
/// The observer is handed a line at a time, but a line that will not end is
/// handed over at the reassembly bound instead — and a credential lying across
/// *that* cut is a whole value in neither piece. The executor holds the tail of
/// an unterminated piece back until the next one arrives, which is what makes
/// the mask able to match at all.
///
/// Driven through the observer directly rather than through a launch: the
/// double delivers whole lines, and a real quarter-megabyte line is not a
/// fixture worth building to reach one branch.
#[test]
fn a_credential_split_across_a_delivery_boundary_is_still_masked() {
    let sink = Arc::new(CollectedActivity::default());
    let held: Arc<dyn AgentActivitySink> = sink.clone();
    let observer = LaunchObserver {
        descriptor: &descriptor::CLAUDE_CODE,
        carry_len: protocol::longest_secret(&[SAMPLE_TOKEN]).saturating_sub(1),
        secrets: vec![SAMPLE_TOKEN.to_string()],
        sink: Some(&held),
        durable: None,
        cancel: CancellationToken::new(),
        durable_failure: StdMutex::new(None),
        log: Silent,
        logs_activity: true,
        vendor: "claude_code".to_string(),
        container: "synthesis-agent-test".to_string(),
        carry: StdMutex::new((Vec::new(), Vec::new())),
    };

    // The piece is long enough that most of it *is* emitted — which is the
    // production shape, and the one where holding the tail back is a decision
    // rather than a side effect of the piece being shorter than the carry. A
    // piece shorter than the carry emits nothing at all and would prove only
    // that nothing leaks when nothing is said.
    let filler = "F".repeat(8 * 1024);
    let split = SAMPLE_TOKEN.len() / 2;
    observer.line(
        StreamChannel::Stdout,
        format!("{filler}{}", &SAMPLE_TOKEN[..split]).as_bytes(),
        false,
    );

    let after_first = sink.rendered();
    // All of the piece but its last few bytes: what is held back is the length
    // of the longest secret less one, which is what makes rejoining possible
    // without ever emitting a fragment.
    assert!(
        after_first.contains(&"F".repeat(filler.len() - SAMPLE_TOKEN.len())),
        "holding the tail back swallowed the whole piece"
    );
    assert!(
        !after_first.contains(&SAMPLE_TOKEN[..split]),
        "the first half of a credential was emitted before its second arrived"
    );

    observer.line(
        StreamChannel::Stdout,
        format!("{}\n", &SAMPLE_TOKEN[split..]).as_bytes(),
        true,
    );

    let rendered = sink.rendered();
    assert!(
        !rendered.contains(SAMPLE_TOKEN),
        "the rejoined credential was emitted whole"
    );
    assert!(
        rendered.contains(&masked_form(SAMPLE_TOKEN)),
        "the two halves were never rejoined and masked: {rendered}"
    );
}

/// The same seam, where the piece is shorter than what must be held back: it is
/// held whole and emitted once the rest arrives, rather than being emitted in a
/// form that could carry a fragment.
#[test]
fn a_piece_shorter_than_the_carry_is_held_until_the_rest_arrives() {
    let sink = Arc::new(CollectedActivity::default());
    let held: Arc<dyn AgentActivitySink> = sink.clone();
    let observer = LaunchObserver {
        descriptor: &descriptor::CLAUDE_CODE,
        carry_len: protocol::longest_secret(&[SAMPLE_TOKEN]).saturating_sub(1),
        secrets: vec![SAMPLE_TOKEN.to_string()],
        sink: Some(&held),
        durable: None,
        cancel: CancellationToken::new(),
        durable_failure: StdMutex::new(None),
        log: Silent,
        logs_activity: true,
        vendor: "claude_code".to_string(),
        container: "synthesis-agent-test".to_string(),
        carry: StdMutex::new((Vec::new(), Vec::new())),
    };

    let split = SAMPLE_TOKEN.len() / 2;
    observer.line(
        StreamChannel::Stdout,
        format!("tok={}", &SAMPLE_TOKEN[..split]).as_bytes(),
        false,
    );
    assert!(sink.events().is_empty(), "a piece too short to be safe was emitted");

    observer.line(
        StreamChannel::Stdout,
        format!("{}\n", &SAMPLE_TOKEN[split..]).as_bytes(),
        true,
    );

    let rendered = sink.rendered();
    assert!(!rendered.contains(SAMPLE_TOKEN));
    assert!(rendered.contains(&masked_form(SAMPLE_TOKEN)));
    // Held, not dropped: the text either side of the credential survives.
    assert!(rendered.contains("tok="));
}

/// The other half of the same rule: a piece that *is* whole is emitted whole,
/// with nothing held back — otherwise every ordinary line would arrive one line
/// late, which is the failure this whole seam exists to prevent.
#[test]
fn a_whole_line_is_emitted_at_once_with_nothing_held_back() {
    let sink = Arc::new(CollectedActivity::default());
    let held: Arc<dyn AgentActivitySink> = sink.clone();
    let observer = LaunchObserver {
        descriptor: &descriptor::CLAUDE_CODE,
        carry_len: protocol::longest_secret(&[SAMPLE_TOKEN]).saturating_sub(1),
        secrets: vec![SAMPLE_TOKEN.to_string()],
        sink: Some(&held),
        durable: None,
        cancel: CancellationToken::new(),
        durable_failure: StdMutex::new(None),
        log: Silent,
        logs_activity: true,
        vendor: "claude_code".to_string(),
        container: "synthesis-agent-test".to_string(),
        carry: StdMutex::new((Vec::new(), Vec::new())),
    };

    observer.line(StreamChannel::Stderr, b"a short diagnostic", true);

    let events = sink.events();
    assert_eq!(events.len(), 1, "a whole line did not arrive as one event");
    assert_eq!(events[0].payload, "a short diagnostic");
}

/// EAC-FR-29, EAC-FR-15, EAC-FR-23 — the masking rule itself: a fingerprint above the threshold, a
/// removal below it, every occurrence, and the longest match first.
#[test]
fn a_masked_value_is_a_fingerprint_only_where_a_fingerprint_hides_most_of_it() {
    // Long enough that four characters at each end is a hint rather than the
    // value: what a reader gets is enough to tell one token from another.
    let masked = masked_form(SAMPLE_TOKEN);
    assert!(masked.starts_with("sk-a"));
    assert!(masked.contains('…'));
    assert_eq!(masked.chars().count(), 9);
    assert!(!SAMPLE_TOKEN.contains(&masked));

    // The threshold, from both sides. Below it a fingerprint would show most of
    // the value, so nothing is shown at all.
    let short = "x".repeat(MIN_FINGERPRINT_LEN - 1);
    assert_eq!(masked_form(&short), REDACTED);
    let long = "x".repeat(MIN_FINGERPRINT_LEN);
    assert_ne!(masked_form(&long), REDACTED);

    // Character counts rather than byte counts, so a non-ASCII value is never
    // cut mid-character.
    let accented = "ééééééééééééééééé";
    let masked = masked_form(accented);
    assert!(!masked.contains('\u{FFFD}'));
    assert_eq!(masked.chars().count(), 9);

    // Every occurrence, and an empty secret ignored.
    assert_eq!(
        mask_secrets("a SECRET b SECRET", &["SECRET", ""]),
        format!("a {REDACTED} b {REDACTED}")
    );

    // Longest first: a secret that contains another is masked as the whole
    // value rather than broken up by the shorter one and left partly readable.
    let outer = "/home/agent/.codex/auth.json";
    let inner = "/home/agent/.codex";
    let masked = mask_secrets(&format!("reading {outer}"), &[inner, outer]);
    assert!(!masked.contains(inner), "the longer secret was left readable");
    assert!(!masked.contains("auth.json"));
    assert_eq!(masked, format!("reading {}", masked_form(outer)));

    // Text carrying no secret is returned as it stands.
    assert_eq!(mask_secrets("nothing here", &["absent"]), "nothing here");
}

/// EAC-FR-THUT — a run that did not complete ends with the executor's own line,
/// which names the exit code and carries none of the run's output.
#[test]
fn a_run_that_exits_nonzero_ends_with_a_line_naming_its_exit_code() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::refused("", "MARKER-STDERR-c3f1", 3);
    let sink = Arc::new(CollectedActivity::default());

    let outcome = run_watching(&harness, runtime, task("go"), sink.clone()).expect("runs");
    assert_eq!(outcome.process_outcome, ProcessOutcome::NonZeroExit);

    let events = sink.events();
    let closing = events.last().expect("a closing event");
    assert_eq!(closing.channel, "executor");
    assert_eq!(closing.kind, "finished");
    assert!(
        closing.summary.starts_with("Turn stopped: the agent process exited with code 3 after "),
        "{}",
        closing.summary
    );
    assert_eq!(closing.payload, closing.summary);
    assert!(!closing.payload.contains("MARKER-STDERR"));
    assert_eq!(
        events.iter().filter(|e| e.channel == "executor" && e.kind == "finished").count(),
        1,
        "the executor closes a run once"
    );
}

/// EAC-FR-THUT — a run that reached its deadline ends with the executor's own
/// line, which names the limit the request carried and none of the output that
/// arrived before the deadline.
#[test]
fn a_run_that_times_out_ends_with_a_line_naming_the_limit_it_reached() {
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
    let sink = Arc::new(CollectedActivity::default());
    let mut request = task("go");
    request.execution.timeout_ms = 4_000;

    let outcome = run_watching(&harness, runtime, request, sink.clone()).expect("runs");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Timeout);
    assert!(!outcome.stdout.bytes.is_empty(), "the fixture wrote output before the deadline");

    let events = sink.events();
    let closing = events.last().expect("a closing event");
    assert_eq!(closing.channel, "executor");
    assert_eq!(closing.kind, "finished");
    assert!(
        closing
            .summary
            .starts_with("Turn stopped: it reached the execution time limit of 4 s after "),
        "{}",
        closing.summary
    );
    let stdout = String::from_utf8_lossy(&outcome.stdout.bytes).to_string();
    assert!(!closing.payload.contains(stdout.trim()));
    assert_eq!(
        events.iter().filter(|e| e.channel == "executor" && e.kind == "finished").count(),
        1,
        "the executor closes a run once"
    );
}
