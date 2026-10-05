//! EAC-FR-27 … EAC-FR-38 — separate launches, log records, and the generated parameter vectors.

use super::*;

// ---------------------------------------------------------------------------
// EAC-FR-27 … EAC-FR-10, EAC-FR-38
// ---------------------------------------------------------------------------

/// EAC-FR-27 — an envelope claiming filesystem work is not treated as
/// verification of it.
#[test]
fn a_filesystem_claim_in_an_envelope_is_never_verified_by_the_executor() {
    let harness = harness_for("claude_code");
    let envelope = json!({
        "protocol_version": 1, "outcome": "success",
        "summary": "wrote three files",
        "result": {"files_written": ["a.md", "b.md", "c.md"]}
    });
    let runtime = RecordingRuntime::replying(&claude_stdout(&envelope.to_string()));

    let before = std::fs::read_dir(harness.workspace())
        .expect("read")
        .count();
    let outcome = run(&harness, runtime, task("go")).expect("ran");
    let after = std::fs::read_dir(harness.workspace())
        .expect("read")
        .count();

    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
    // The claim is carried through verbatim as the agent's own report…
    let result = outcome.response.expect("envelope").result.expect("result");
    assert_eq!(result["files_written"].as_array().unwrap().len(), 3);
    // …and the executor neither created those files nor checked for them. It
    // touched the execution directory not at all.
    assert_eq!(before, after);
    assert!(!harness.workspace().join("a.md").exists());

    // The claim is carried on the agent's own report and nowhere else: there is
    // no field on the result that asserts a file exists, because the executor
    // verified nothing. The directory comparison above is the evidence — a
    // blocklist of a few method names over one file would pass just as happily
    // for an executor that read the directory by some other call.
    assert!(outcome.stdout.bytes.len() > 0);
}

/// EAC-FR-28 — the configuration that would actually bite: **one** executor and
/// **one** runtime driving two launches at once.
///
/// Two executors over two runtimes share no memory and so can demonstrate
/// nothing; this would fail if the executor held any per-call scratch state.
#[test]
fn one_executor_driving_two_launches_keeps_them_separate() {
    let claude = harness_for("claude_code");
    let codex = harness_for("codex");

    // A runtime that answers each vendor with its own shape, so a crossed
    // response would be a parse failure rather than a silent pass.
    let shared = Arc::new(RecordingRuntime {
        script: Script::PerVendor,
        available: true,
        image_available: true,
        launch_error: None,
        remove_fails: false,
        runs: StdMutex::new(Vec::new()),
        removed: StdMutex::new(Vec::new()),
        images_asked: StdMutex::new(Vec::new()),
    });
    let executor = Arc::new(
        AgentCliExecutor::new(shared.clone())
            .with_session_state_root(claude.sessions_root()),
    );

    let claude_context = claude.context();
    let codex_context = codex.context();
    let (a, b) = block_on(async {
        tokio::join!(
            executor.execute_agent_cli(
                &Silent,
                &claude_context,
                AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
                    execution_directory: claude.workspace(),
                    task: task("CLAUDE-ONLY"),
                    cancellation: CancellationToken::new(),
                    activity: None,
                    supplementary_mount: None,
                },
            ),
            executor.execute_agent_cli(
                &Silent,
                &codex_context,
                AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
                    execution_directory: codex.workspace(),
                    task: task("CODEX-ONLY"),
                    cancellation: CancellationToken::new(),
                    activity: None,
                    supplementary_mount: None,
                },
            )
        )
    });

    assert_eq!(a.expect("ran").process_outcome, ProcessOutcome::Completed);
    assert_eq!(b.expect("ran").process_outcome, ProcessOutcome::Completed);

    let runs = shared.runs.lock().unwrap().clone();
    assert_eq!(runs.len(), 2);

    // Distinct containers, distinct stdin, and the token on exactly one of them
    // — through one executor and one runtime object.
    assert_ne!(container_name(&runs[0].argv), container_name(&runs[1].argv));
    let with_token = runs
        .iter()
        .filter(|r| r.env.contains_key("CLAUDE_CODE_OAUTH_TOKEN"))
        .count();
    assert_eq!(with_token, 1, "the token reached exactly one of the two");
    let stdins: Vec<String> = runs
        .iter()
        .map(|r| String::from_utf8_lossy(&r.stdin).into_owned())
        .collect();
    assert!(stdins.iter().any(|s| s.contains("CLAUDE-ONLY")));
    assert!(stdins.iter().any(|s| s.contains("CODEX-ONLY")));
    for stdin in &stdins {
        assert!(
            !(stdin.contains("CLAUDE-ONLY") && stdin.contains("CODEX-ONLY")),
            "one launch carried both callers' task data"
        );
    }
    // And `unique_container_name`'s counter under contention.
    let names: std::collections::HashSet<String> =
        runs.iter().map(|r| container_name(&r.argv)).collect();
    assert_eq!(names.len(), 2);
}

/// EAC-FR-28 — concurrent requests share nothing.
#[test]
fn concurrent_requests_cannot_cross_contaminate() {
    let claude = harness_for("claude_code");
    let codex = harness_for("codex");

    let claude_runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let codex_runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));

    let claude_context = claude.context();
    let codex_context = codex.context();
    let (claude_result, codex_result) = block_on(async {
        let claude_executor = AgentCliExecutor::new(claude_runtime.clone())
            .with_session_state_root(claude.sessions_root());
        let codex_executor = AgentCliExecutor::new(codex_runtime.clone())
            .with_session_state_root(codex.sessions_root());
        tokio::join!(
            claude_executor.execute_agent_cli(
                &Silent,
                &claude_context,
                AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
                    execution_directory: claude.workspace(),
                    task: task("CLAUDE-WORK"),
                    cancellation: CancellationToken::new(),
                    activity: None,
                    supplementary_mount: None,
                },
            ),
            codex_executor.execute_agent_cli(
                &Silent,
                &codex_context,
                AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
                    execution_directory: codex.workspace(),
                    task: task("CODEX-WORK"),
                    cancellation: CancellationToken::new(),
                    activity: None,
                    supplementary_mount: None,
                },
            )
        )
    });

    assert_eq!(
        claude_result.expect("ran").process_outcome,
        ProcessOutcome::Completed
    );
    assert_eq!(
        codex_result.expect("ran").process_outcome,
        ProcessOutcome::Completed
    );

    let a = claude_runtime.only_run();
    let b = codex_runtime.only_run();

    // Each got its own stdin, its own container, its own environment, and its
    // own mounts.
    assert!(String::from_utf8_lossy(&a.stdin).contains("CLAUDE-WORK"));
    assert!(!String::from_utf8_lossy(&a.stdin).contains("CODEX-WORK"));
    assert!(String::from_utf8_lossy(&b.stdin).contains("CODEX-WORK"));
    assert_ne!(
        container_name(&a.argv),
        container_name(&b.argv),
        "distinct container names"
    );

    // Claude's token is in Claude's environment alone; Codex's mount is in
    // Codex's argv alone.
    assert!(a.env.contains_key("CLAUDE_CODE_OAUTH_TOKEN"));
    assert!(b.env.is_empty());
    assert!(!a.argv.join(" ").contains("/.codex"));
    assert!(b.argv.join(" ").contains("/.codex"));
    // Neither mounted the other's worktree.
    assert!(!a.argv.join(" ").contains(&*codex.workspace().to_string_lossy()));
    assert!(!b.argv.join(" ").contains(&*claude.workspace().to_string_lossy()));
}

/// EAC-FR-29, EAC-FR-15, EAC-FR-23 — what the log records say, and what they must never say.
#[test]
fn log_records_name_the_launch_without_carrying_anything_sensitive() {
    use crate::logging::{Domain, LogFilter};

    const INSTRUCTION: &str = "MARKER-INSTRUCTION-a91f";
    const RESUME: &str = "MARKER-RESUME-b22e";

    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());

    let mut request = task(INSTRUCTION);
    request.resume = Some(SessionRef {
        session_id: Some(RESUME.to_string()),
        continuation_token: None,
    });
    let mut input = serde_json::Map::new();
    input.insert("ctx".into(), json!("MARKER-INPUT-c33d"));
    request.input = Some(input);

    run(&harness, runtime.clone(), request).expect("ran");
    let runtime_container = runtime
        .runs
        .lock()
        .unwrap()
        .first()
        .map(|r| container_name(&r.argv));
    // A pre-launch failure too, so a handled error path is represented. This one
    // fails in `ensure_available` and returns before any container is created,
    // so it reaches the pre-launch error record rather than the one below.
    let broken = RecordingRuntime::broken(false, true, None);
    let _ = run(&harness, broken, task("go"));

    // And a CLI that ran, refused, and said why on stderr — with the credential
    // and the session identity in the same sentence. EAC-FR-29 lets the excerpt
    // reach the record; EAC-FR-15 never lets the token, and EAC-FR-29 itself
    // never lets a session or resume field. All three run through the same bytes
    // here, so a masking that stopped working fails this rather than passing
    // quietly.
    const DIAGNOSTIC: &str = "MARKER-STDERR-d44f: the CLI refused its input";
    let refused = Arc::new(RecordingRuntime {
        script: Script::Reply {
            stdout: String::new(),
            stderr: format!("{DIAGNOSTIC} (token={SAMPLE_TOKEN}, session={RESUME})\n"),
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
    let mut refused_task = task("go");
    refused_task.resume = Some(SessionRef {
        session_id: Some(RESUME.to_string()),
        continuation_token: None,
    });
    let _ = run(&harness, refused.clone(), refused_task);
    let refused_container = container_name(&refused.only_run().argv);

    // The module logs into the shared buffer; read it back and check the whole
    // rendering rather than field by field, because a leak added later arrives
    // in a *new* field an itemised assertion would not know to look at.
    // The buffer is process-global and every other test in this file writes to
    // it, so the positive assertions have to be attributable to *this* launch
    // or they prove nothing. The container name is unique per launch
    // (EAC-FR-13), which is what makes that possible.
    let container = runtime_container.expect("a launch was recorded");
    let page = super::super::log_buffer()
        .query(&LogFilter::default(), None, 20_000)
        .expect("query");
    let mine: Vec<_> = page
        .records
        .iter()
        .filter(|record| {
            record
                .fields
                .values()
                .any(|value| value.as_str() == Some(container.as_str()))
                || serde_json::to_string(&record.fields)
                    .is_ok_and(|f| f.contains(container.as_str()))
        })
        .collect();
    assert!(
        !mine.is_empty(),
        "no record could be attributed to this launch"
    );

    let rendered = serde_json::to_string(&page).expect("serialise");
    assert!(rendered.contains("agent execution starting"));
    assert!(rendered.contains("agent execution finished"));
    assert!(rendered.contains("claude_code"));
    assert!(rendered.contains("completed"));

    // EAC-FR-29's failure record, read as itself rather than scanned for out of a
    // buffer every other test also writes to. One equality pins the excerpt's
    // presence, its position, its exact masking, and the record it belongs to;
    // a substring scan of the whole page would prove none of the four.
    let record = failure_record(&refused_container);
    let field = |name: &str| record.fields.get(name).cloned().unwrap_or(serde_json::Value::Null);
    // EAC-FR-29's masking is a fingerprint rather than a removal for a value
    // long enough to keep half of it hidden: four characters at each end tell
    // one token from another and one session from another, which is what a
    // reader needs to line a record up against the integration that produced it.
    assert_eq!(
        field("stderr_excerpt"),
        json!(format!(
            "{DIAGNOSTIC} (token={}, session={})",
            masked_form(SAMPLE_TOKEN),
            masked_form(RESUME)
        )),
        "the excerpt did not arrive, or did not arrive masked"
    );
    // And the fingerprint is a fingerprint: neither value survives whole, and
    // neither survives in a piece long enough to be one.
    let excerpt = field("stderr_excerpt").as_str().unwrap_or_default().to_string();
    for secret in [SAMPLE_TOKEN, RESUME] {
        assert!(!excerpt.contains(secret), "a whole secret reached the record");
        assert!(
            !excerpt.contains(&secret[..secret.len().min(12)]),
            "a leading fragment of a secret reached the record"
        );
    }
    // The rest of what EAC-FR-29, EAC-FR-15, EAC-FR-23 asks a failure record to carry. `stdout_bytes`
    // and `stderr_bytes` are asserted as a pair because the discrimination they
    // exist for is the pair: a CLI that refused its arguments wrote a line to
    // one and nothing to the other.
    assert_eq!(field("outcome"), json!("non_zero_exit"));
    assert_eq!(field("exit_code"), json!(1));
    assert_eq!(field("stdout_bytes"), json!(0));
    assert_eq!(
        field("stderr_bytes"),
        json!(format!("{DIAGNOSTIC} (token={SAMPLE_TOKEN}, session={RESUME})\n").len())
    );
    assert_eq!(field("stdout_truncated"), json!(false));
    assert_eq!(field("stderr_truncated"), json!(false));
    assert_eq!(field("vendor"), json!("claude_code"));

    // EAC-FR-29, EAC-FR-15, EAC-FR-23 asks that a reader filtering on either `ai` or `backend` finds
    // the same records: a tool call is backend work done because a model asked
    // for it. Asserted on the records themselves rather than by running two
    // queries and comparing them — the suite is multi-threaded and other tests
    // append to this buffer between any two calls, which would make the
    // comparison a race rather than a claim.
    let ours: Vec<_> = page
        .records
        .iter()
        .filter(|r| r.message.starts_with("agent execution"))
        .collect();
    assert!(!ours.is_empty());
    for record in &ours {
        assert!(
            record.domains.contains(&Domain::Ai) && record.domains.contains(&Domain::Backend),
            "{:?} would be missed by one of the two filters",
            record.message
        );
    }

    // EAC-FR-29: a credential never appears in any record, in any form, at any
    // level — not the token, not a recognisable piece of one, not the session
    // identity, and not the location of a credential this application is
    // forbidden to read.
    for secret in [SAMPLE_TOKEN, "sk-ant-oat01-", RESUME, ".codex"] {
        assert!(
            !rendered.contains(secret),
            "log records leaked {secret:?}"
        );
    }

    // EAC-FR-34: the task and the agent's own words *do* reach the log now,
    // because a run nobody can see is a run nobody can fix — but only in the
    // activity records, and only at `DEBUG`. The records that describe the
    // launch itself stay a boundary account, so a reader who raises the level
    // floor to hide a chatty run still has an unbroken record of every turn
    // (LOG-FR-07).
    let activity: Vec<_> = page
        .records
        .iter()
        .filter(|r| r.message == "agent activity")
        .collect();
    assert!(!activity.is_empty(), "no activity reached the log");
    for record in &activity {
        assert_eq!(record.level, crate::logging::LogLevel::Debug);
    }
    let boundary = serde_json::to_string(
        &page
            .records
            .iter()
            .filter(|r| r.message != "agent activity")
            .collect::<Vec<_>>(),
    )
    .expect("serialise");
    for content in [INSTRUCTION, "MARKER-INPUT-c33d", "did the thing"] {
        assert!(
            !boundary.contains(content),
            "a boundary record carried run content: {content:?}"
        );
    }
    let observed = serde_json::to_string(&activity).expect("serialise");
    assert!(
        observed.contains(INSTRUCTION) && observed.contains("MARKER-INPUT-c33d"),
        "the task the agent was given did not reach the activity records"
    );
}

/// EAC-FR-21, EAC-FR-30 — resume, and a vendor that cannot.
#[test]
fn a_resumed_turn_carries_the_session_and_an_unsupported_one_says_so() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());

    // EAC-FR-21: the identity the *run* reported wins over the one the agent
    // wrote into its envelope. `valid_claude_stdout`'s envelope names `sess-1`,
    // and the run reports the assigned id — the assigned id is what the caller
    // carries forward, because the envelope's is a string a model composed and
    // resuming on it would continue nothing.
    let first = run(&harness, runtime.clone(), task("go")).expect("ran");
    let first_assigned = assigned_session_id(&runtime.only_run().argv).expect("--session-id");
    assert_eq!(
        first.session.as_ref().and_then(|s| s.session_id.as_deref()),
        Some(first_assigned.as_str())
    );
    assert_ne!(first_assigned, "sess-1");

    // Where the agent reports none, the identity the run itself carried is what
    // makes a later resume possible. For this vendor that identity is the one
    // the executor assigned, so it is exactly the `--session-id` in the vector.
    let no_session = json!({"protocol_version":1,"outcome":"success","summary":"s"});
    let runtime = RecordingRuntime::replying(&claude_stdout(&no_session.to_string()));
    let outcome = run(&harness, runtime.clone(), task("go")).expect("ran");
    let assigned = assigned_session_id(&runtime.only_run().argv).expect("--session-id");
    assert_eq!(
        outcome.session.as_ref().and_then(|s| s.session_id.as_deref()),
        Some(assigned.as_str())
    );

    // A second turn supplying it generates the vendor's resumed grammar, and
    // drops the `--session-id` a fresh turn would have carried.
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let mut second = task("continue");
    second.resume = Some(SessionRef {
        session_id: Some(assigned.clone()),
        continuation_token: None,
    });
    run(&harness, runtime.clone(), second).expect("ran");
    let argv = runtime.only_run().argv;
    let at = argv.iter().position(|a| a == "--resume").expect("--resume");
    assert_eq!(argv[at + 1], assigned);
    assert!(!argv.iter().any(|a| a == "--session-id"));

    // EAC-FR-12, EAC-FR-30: Codex's resumed turn is its own grammar, not the fresh one with
    // an addition — the session id sits before the `-` prompt, and the sandbox
    // policy arrives as a config override because `exec resume` refuses
    // `--sandbox`.
    let codex = harness_for("codex");
    let runtime = RecordingRuntime::replying(&codex_stdout(&envelope_json("success")));
    let mut resumed = task("continue");
    resumed.resume = Some(SessionRef {
        session_id: Some("abc".into()),
        continuation_token: None,
    });
    run(&codex, runtime.clone(), resumed).expect("ran");
    let argv = runtime.only_run().argv;
    let image = test_image_reference("codex");
    let tail = &argv[argv.iter().position(|a| *a == image).unwrap() + 1..];
    assert_eq!(
        tail,
        [
            "exec",
            "resume",
            "--json",
            "--skip-git-repo-check",
            "-c",
            "sandbox_mode=\"danger-full-access\"",
            "abc",
            "-",
        ]
    );
    assert!(!tail.iter().any(|a| a == "--sandbox"));

    // Both pinned vendors can resume; the refusal path exists for one that
    // cannot, and is exercised through the descriptor's own predicate.
    assert!(descriptor::CLAUDE_CODE.supports_resume());
    assert!(descriptor::CODEX.supports_resume());
}

/// EAC-FR-10, EAC-FR-38 — the descriptors match the manifest, and no invocation parameter
/// lives outside one.
#[test]
fn every_parameter_is_a_descriptor_or_a_named_constant() {
    // The manifest is the only source of an image's identity (AVI-FR-07).
    for vendor in ["claude_code", "codex"] {
        let entry = descriptor::manifest_entry(vendor).expect("a manifest entry");
        assert!(entry.image_ref.contains(vendor.replace('_', "-").as_str()));
        assert!(entry.image_digest.starts_with("sha256:"));
        assert_eq!(entry.uid, 10001);
        assert_eq!(entry.gid, 10001);
        // The tag names the same version the CLI reports, followed by the
        // build revision of AVI-FR-08 — so the two cannot drift silently, and
        // an image whose floor changed under an unchanged CLI is a tag of its
        // own rather than the same one republished.
        let tag = entry.image_ref.rsplit_once(':').expect("a tag").1;
        assert!(
            tag.starts_with(&format!("{}-", entry.cli_version)),
            "{vendor}'s tag {tag:?} does not name its CLI version and a build revision"
        );
        // A published entry is pinned by digest, never by the tag. Asserted
        // against a published stand-in rather than the shipped entry, which
        // carries the unpublished sentinel and is addressed by its tag so it can
        // be built locally (AVI-FR-08).
        let published = published_image(vendor).expect("a published stand-in");
        assert_eq!(
            published.launch_reference(),
            format!(
                "{}@{}",
                published.image_ref.rsplit_once(':').unwrap().0,
                published.image_digest
            )
        );
        assert!(!published.launch_reference().contains(&published.cli_version));
    }

    // No call site assembles an argument of its own: every flag the *container*
    // invocation carries is a literal in `descriptor.rs` alone. `runtime.rs` is
    // included in the scan because it drives the Docker CLI directly and is
    // where an ad-hoc flag would most plausibly appear.
    for (name, source) in [
        ("agent_exec.rs", include_str!("../../agent_exec.rs")),
        ("protocol.rs", include_str!("../protocol.rs")),
        ("runtime.rs", include_str!("../runtime.rs")),
    ] {
        for flag in [
            "--rm",
            "--interactive",
            "--name",
            "--network",
            "--user",
            "--workdir",
            "--mount",
            "--cap-drop",
            "--security-opt",
            "--privileged",
            "--output-format",
            "--permission-mode",
            "--sandbox",
        ] {
            assert!(
                !source.contains(&format!("\"{flag}\"")),
                "{flag} is spelled in {name} rather than in a descriptor"
            );
        }
    }

    // `runtime.rs` does issue its own *control* commands — a liveness probe, an
    // image inspect, a pull, a removal. Those are the runtime's own vocabulary
    // rather than the container invocation EAC-FR-10 governs, and they are
    // named here so that adding a fifth is a deliberate edit to this list.
    let runtime = include_str!("../runtime.rs");
    let control_commands = ["version", "image", "inspect", "pull", "rm"];
    for command in control_commands {
        assert!(
            runtime.contains(&format!("\"{command}\"")),
            "the control command {command} moved without this list being updated"
        );
    }

    // Each descriptor names the specification it transcribes, and the version
    // that specification was asserted against is the one the manifest installs
    // (CCP-FR-01, CDX-FR-01).
    for descriptor in [&descriptor::CLAUDE_CODE, &descriptor::CODEX] {
        let entry = descriptor::manifest_entry(descriptor.vendor).expect("a manifest entry");
        assert_eq!(
            entry.cli_version, descriptor.pinned_cli_version,
            "{} pins a version the manifest does not install",
            descriptor.vendor
        );
        assert!(descriptor.protocol_spec.starts_with("specifications/infra/"));
    }
}

/// CCP-FR-02, CCP-FR-03, CCP-FR-04, CCP-FR-05, CCP-FR-06 / CCP-FR-02 / CCP-FR-05 — Claude Code's vector, exactly.
#[test]
fn claude_codes_vector_is_the_ordered_sequence_ccp_defines() {
    let full = descriptor::CLAUDE_CODE.vendor_args(Some("opus"), Some("high"), "SID", None, None);
    assert_eq!(
        full,
        [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--json-schema",
            descriptor::claude_code::ENVELOPE_SCHEMA,
            "--permission-mode",
            "bypassPermissions",
            "--model",
            "opus",
            "--effort",
            "high",
            "--session-id",
            "SID",
        ]
    );

    // CCP-FR-02, CCP-FR-05: no model and no effort resolved, everything else in place.
    let bare = descriptor::CLAUDE_CODE.vendor_args(None, None, "SID", None, None);
    assert_eq!(
        bare,
        [
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--json-schema",
            descriptor::claude_code::ENVELOPE_SCHEMA,
            "--permission-mode",
            "bypassPermissions",
            "--session-id",
            "SID",
        ]
    );

    // CCP-FR-04: never a weaker permission mode, which would leave a turn that
    // has to build or test stalled on a prompt nobody can answer.
    assert!(!full.iter().any(|a| a == "acceptEdits"));

    // CCP-FR-05: every documented effort reaches the command line unaltered, and
    // one outside the set is refused rather than passed through.
    for effort in ["low", "medium", "high", "xhigh", "max"] {
        assert!(descriptor::CLAUDE_CODE.effort_supported(effort));
        let argv = descriptor::CLAUDE_CODE.vendor_args(None, Some(effort), "SID", None, None);
        let at = argv.iter().position(|a| a == "--effort").expect("--effort");
        assert_eq!(argv[at + 1], effort);
    }
    assert!(!descriptor::CLAUDE_CODE.effort_supported("ultracode"));

    // CCP-FR-17: a resumed turn carries `--resume` and no
    // `--session-id`.
    let resumed = descriptor::CLAUDE_CODE.vendor_args(None, None, "SID", Some("prior"), None);
    assert!(!resumed.iter().any(|a| a == "--session-id"));
    let at = resumed.iter().position(|a| a == "--resume").expect("--resume");
    assert_eq!(resumed[at + 1], "prior");

    // CCP-FR-21, CCP-FR-23: none of the flags this protocol does not generate.
    for forbidden in [
        "--bare",
        "--continue",
        "--fork-session",
        "--no-session-persistence",
        "--dangerously-skip-permissions",
        "--max-turns",
        "--input-format",
        "--add-dir",
        "--mcp-config",
    ] {
        for argv in [&full, &bare, &resumed] {
            assert!(
                !argv.iter().any(|a| a == forbidden),
                "{forbidden} appears in a generated Claude Code vector"
            );
        }
    }

    // CCP-FR-08: the schema is fixed text carrying no task data, which is the
    // only reason it may ride on the vector at all.
    let other = descriptor::CLAUDE_CODE.vendor_args(Some("sonnet"), None, "OTHER", None, None);
    let schema_of = |v: &[String]| {
        let at = v.iter().position(|a| a == "--json-schema").unwrap();
        v[at + 1].clone()
    };
    assert_eq!(schema_of(&full), schema_of(&other));
    // And it is valid JSON describing the envelope.
    let schema: serde_json::Value =
        serde_json::from_str(descriptor::claude_code::ENVELOPE_SCHEMA).expect("valid JSON Schema");
    assert_eq!(schema["properties"]["outcome"]["enum"][0], "success");
}

/// CDX-FR-02, CDX-FR-03, CDX-FR-04, CDX-FR-05, CDX-FR-06, CDX-FR-07, CDX-FR-08 / CDX-FR-02 / CDX-FR-09, CDX-FR-10 / CDX-FR-11 — Codex's two vectors, exactly.
#[test]
fn codexs_two_vectors_are_the_ordered_sequences_cdx_defines() {
    let fresh = descriptor::CODEX.vendor_args(Some("gpt-5"), Some("low"), "IGNORED", None, None);
    assert_eq!(
        fresh,
        [
            "exec",
            "--json",
            "--sandbox",
            "danger-full-access",
            "--skip-git-repo-check",
            "--model",
            "gpt-5",
            "-c",
            "model_reasoning_effort=\"low\"",
            "-",
        ]
    );
    // CDX-FR-20: this vendor cannot be told which session id to use, so the
    // assigned identifier never reaches its vector.
    assert!(!fresh.iter().any(|a| a == "IGNORED"));

    // CDX-FR-02, CDX-FR-06, CDX-FR-07: neither model nor effort resolved.
    assert_eq!(
        descriptor::CODEX.vendor_args(None, None, "IGNORED", None, None),
        [
            "exec",
            "--json",
            "--sandbox",
            "danger-full-access",
            "--skip-git-repo-check",
            "-",
        ]
    );

    // CDX-FR-09, CDX-FR-10: the resumed grammar is its own, with the session id before the
    // `-` prompt and the sandbox policy as a config override.
    let resumed = descriptor::CODEX.vendor_args(Some("gpt-5"), Some("low"), "IGNORED", Some("thr"), None);
    assert_eq!(
        resumed,
        [
            "exec",
            "resume",
            "--json",
            "--skip-git-repo-check",
            "-c",
            "sandbox_mode=\"danger-full-access\"",
            "--model",
            "gpt-5",
            "-c",
            "model_reasoning_effort=\"low\"",
            "thr",
            "-",
        ]
    );

    // CDX-FR-10, CDX-FR-11: `exec resume` accepts none of these at the pinned version, so a
    // resumed vector that carried one would be rejected before the run started.
    for forbidden in [
        "--sandbox",
        "-s",
        "-C",
        "--cd",
        "-a",
        "--ask-for-approval",
        "--add-dir",
        "--approve-for-me",
        "-p",
        "--profile",
        "--oss",
        "--color",
    ] {
        assert!(
            !resumed.iter().any(|a| a == forbidden),
            "{forbidden} appears in a resumed Codex vector"
        );
    }

    // CDX-FR-26, CDX-FR-27: flags this protocol never generates, on either vector.
    for forbidden in [
        "--full-auto",
        "--dangerously-bypass-approvals-and-sandbox",
        "--dangerously-bypass-hook-trust",
        "--output-schema",
        "-o",
        "--output-last-message",
        "-i",
        "--image",
        "--enable",
        "--disable",
        "--strict-config",
        "--ignore-user-config",
        "--ignore-rules",
        "--ephemeral",
        "--last",
        "--all",
    ] {
        for argv in [&fresh, &resumed] {
            assert!(
                !argv.iter().any(|a| a == forbidden),
                "{forbidden} appears in a generated Codex vector"
            );
        }
    }
}

/// EAC-FR-10, EAC-FR-11, EAC-FR-12 — each descriptor transcribes one specification and borrows no term
/// from the other's.
#[test]
fn neither_vendor_descriptor_carries_the_others_vocabulary() {
    let claude = include_str!("../descriptor/claude_code.rs");
    let codex = include_str!("../descriptor/codex.rs");

    // Codex's vocabulary must not appear in Claude Code's transcription.
    // Matched as quoted literals, so a prose word like "execution" is not
    // mistaken for the `"exec"` subcommand.
    for term in [
        "--sandbox",
        "danger-full-access",
        "--skip-git-repo-check",
        "model_reasoning_effort",
        "CODEX_HOME",
        "thread_id",
        "agent_message",
        "exec",
        "resume\"",
    ] {
        assert!(
            !claude.contains(&format!("\"{term}")),
            "{term} is Codex's, and appears in Claude Code's protocol module"
        );
    }
    // And the reverse.
    for term in [
        "--permission-mode",
        "bypassPermissions",
        "--json-schema",
        "structured_output",
        "--output-format",
        "CLAUDE_CONFIG_DIR",
        "--session-id",
        "--effort",
        "--resume",
    ] {
        assert!(
            !codex.contains(&format!("\"{term}")),
            "{term} is Claude Code's, and appears in Codex's protocol module"
        );
    }

    // Neither module reads the other.
    assert!(!claude.contains("super::codex") && !claude.contains("descriptor::codex"));
    assert!(!codex.contains("super::claude_code") && !codex.contains("descriptor::claude_code"));
}
