//! A Codex launch in each authentication mode: what the container is given,
//! what the returned session records, a resume across a mode change, and the
//! launch files the vector names.

use super::*;
use crate::tools::agent_exec::descriptor::codex;

/// A fake access token, shaped as AIC-FR-BXFL accepts it.
const ACCESS_TOKEN: &str = "codex-access-FAKE-TEST-TOKEN-NOT-A-REAL-CREDENTIAL-0001";

/// The instruction block that makes a scanned turn carry the reminder.
const WITH_BLOCK: &str =
    "Do the work.\n\n## Milestones\n\nSYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1}\n";

/// A Codex harness verified in the `access_token` mode.
fn token_harness() -> Harness {
    harness_verified_with(
        "codex",
        Some(VerifyConfig {
            path: Some("/usr/bin/codex".into()),
            auth_mode: Some("access_token".into()),
            access_token: Some(ACCESS_TOKEN.into()),
            ..Default::default()
        }),
    )
}

fn codex_runtime() -> Arc<RecordingRuntime> {
    RecordingRuntime::replying(&codex_stdout(&envelope_json("success")))
}

fn resumed(token: Option<&str>) -> AgentTaskRequest {
    let mut request = task("continue");
    request.resume = Some(SessionRef {
        session_id: Some(CODEX_THREAD_ID.into()),
        continuation_token: token.map(str::to_string),
    });
    request
}

fn mounts_of(recorded: &Recorded) -> Vec<BindMount> {
    recorded.container.as_ref().expect("a container").mounts.clone()
}

/// EAC-FR-QHIU, EAC-FR-15, CDX-FR-EUBZ, CDX-FR-24 — in the `access_token` mode the token reaches
/// the container by name, the session state is a dedicated per-project directory at
/// `/home/agent/.codex`, and no login directory is mounted.
#[test]
fn an_access_token_launch_gets_the_token_and_its_own_session_directory() {
    let harness = token_harness();
    let runtime = codex_runtime();
    let outcome = run(&harness, runtime.clone(), task("go")).expect("runs");
    assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);

    let recorded = runtime.only_run();
    // The name in the vector, the value in the client environment alone.
    let at = recorded.argv.iter().position(|a| a == "CODEX_ACCESS_TOKEN").expect("by name");
    assert_eq!(recorded.argv[at - 1], "--env");
    assert!(!recorded.argv.iter().any(|a| a.contains(ACCESS_TOKEN)));
    assert_eq!(recorded.env.get("CODEX_ACCESS_TOKEN").map(String::as_str), Some(ACCESS_TOKEN));
    assert!(recorded.argv.iter().any(|a| a == "CODEX_HOME=/home/agent/.codex"));
    for forbidden in ["CODEX_API_KEY", "OPENAI_API_KEY"] {
        assert!(!recorded.argv.iter().any(|a| a.contains(forbidden)), "{forbidden}");
    }

    let mounts = mounts_of(&recorded);
    let state = mounts.iter().find(|m| m.target == "/home/agent/.codex").expect("session state");
    assert_eq!(state.source, harness.session_state_dir("codex"));
    assert!(!state.read_only);
    let login = harness.home.path().join(".codex").to_string_lossy().into_owned();
    assert!(!mounts.iter().any(|m| m.source == login), "no login directory is mounted");
}

/// EAC-FR-16, CDX-FR-23 — in the `login_folder` mode the login directory is the session
/// directory, mounted read/write, and no dedicated directory is made.
#[test]
fn a_login_folder_launch_mounts_the_login_directory_alone() {
    let harness = harness_for("codex");
    let runtime = codex_runtime();
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();
    let mounts = mounts_of(&recorded);
    let state = mounts.iter().find(|m| m.target == "/home/agent/.codex").expect("login mount");
    assert_eq!(state.source, harness.home.path().join(".codex").to_string_lossy());
    assert!(!state.read_only);
    assert_eq!(mounts.iter().filter(|m| m.target == "/home/agent/.codex").count(), 1);
    assert!(!recorded.argv.iter().any(|a| a == "CODEX_ACCESS_TOKEN"));
    assert!(!std::path::Path::new(&harness.session_state_dir("codex")).exists());
}

/// EAC-FR-IGXZ — the returned Codex session records the mode it ran under, and the value the
/// envelope reports is ignored.
#[test]
fn a_codex_session_records_its_authentication_mode() {
    let mut envelope: serde_json::Value = serde_json::from_str(&envelope_json("success")).unwrap();
    envelope["session"]["continuation_token"] = json!("agent-wrote-this");
    for (harness, mode) in
        [(harness_for("codex"), "login_folder"), (token_harness(), "access_token")]
    {
        let runtime = RecordingRuntime::replying(&codex_stdout(&envelope.to_string()));
        let outcome = run(&harness, runtime, task("go")).expect("runs");
        let session = outcome.session.expect("a session");
        assert_eq!(session.session_id.as_deref(), Some(CODEX_THREAD_ID));
        assert_eq!(session.continuation_token.as_deref(), Some(mode));
    }
    // Claude Code's session is not touched.
    let claude = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    let outcome = run(&claude, runtime, task("go")).expect("runs");
    assert_eq!(outcome.session.and_then(|s| s.continuation_token), None);
}

/// EAC-FR-IGXZ, EAC-FR-30 — a resume recorded under the other mode reports `resume_unavailable`
/// with no container created; a `null` token reads as `login_folder`.
#[test]
fn a_resume_across_a_mode_change_is_refused_before_any_container() {
    for (harness, token) in [
        (token_harness(), None),
        (token_harness(), Some("login_folder")),
        (harness_for("codex"), Some("access_token")),
    ] {
        let runtime = codex_runtime();
        let outcome = run(&harness, runtime.clone(), resumed(token)).expect("answers");
        assert!(outcome.resume_unavailable, "{token:?}");
        assert!(outcome.container_removed);
        assert!(outcome.session.is_none());
        assert_eq!(runtime.launched(), 0, "{token:?}: no container is created");
    }
}

/// EAC-FR-IGXZ — a resume under the mode it was recorded in runs, and so does a `login_folder`
/// resume of a token that names no mode.
#[test]
fn a_resume_under_the_same_mode_runs() {
    for (harness, token) in [
        (harness_for("codex"), None),
        (harness_for("codex"), Some("login_folder")),
        (harness_for("codex"), Some("a-token-from-an-older-build")),
        (token_harness(), Some("access_token")),
    ] {
        let runtime = codex_runtime();
        let outcome = run(&harness, runtime.clone(), resumed(token)).expect("runs");
        assert!(!outcome.resume_unavailable, "{token:?}");
        let argv = runtime.only_run().argv;
        assert_eq!(&argv[argv.len() - 2..], [CODEX_THREAD_ID, "-"]);
    }
}

/// The launch-files mount of a recorded run, where it has one.
fn launch_files_mount(recorded: &Recorded) -> Option<BindMount> {
    mounts_of(recorded).into_iter().find(|m| m.target == "/opt/synthesis/vendor")
}

/// EAC-FR-BOXF, EAC-FR-VTLG, CDX-FR-14 — every Codex launch mounts one read-only,
/// content-addressed directory that holds the full fixed set of launch files, whatever the turn:
/// two turns of one contract, a turn of another contract and a plain turn all share it.
#[test]
fn every_codex_turn_mounts_the_same_launch_files_directory() {
    let harness = harness_for("codex");
    let mut sources = Vec::new();
    for contract in [
        Some(ResultContract::ReviewVerdict),
        Some(ResultContract::ReviewVerdict),
        Some(ResultContract::PullRequestSummary),
        None,
    ] {
        let runtime = codex_runtime();
        let mut request = task("judge the work");
        request.result_contract = contract;
        run(&harness, runtime.clone(), request).expect("runs");
        let recorded = runtime.only_run();
        let mount = launch_files_mount(&recorded).expect("the launch files are mounted");
        assert!(mount.read_only);
        match contract {
            Some(contract) => {
                let at = recorded.argv.iter().position(|a| a == "--output-schema").expect("schema");
                assert_eq!(
                    recorded.argv[at + 1],
                    format!("/opt/synthesis/vendor/{}", codex::output_schema_file(contract))
                );
            }
            None => assert!(!recorded.argv.iter().any(|a| a == "--output-schema")),
        }
        sources.push(mount.source);
    }
    assert!(sources.iter().all(|s| s == &sources[0]), "one directory for every turn: {sources:?}");

    let dir = std::path::PathBuf::from(&sources[0]);
    assert!(dir.starts_with(harness.sessions_root().join("vendor-files")));
    let files = descriptor::CODEX.launch_files();
    assert_eq!(dir.file_name().unwrap().to_string_lossy(), descriptor::launch_files_digest(&files));
    for file in &files {
        let written = std::fs::read(dir.join(file.name)).expect("the launch file");
        assert_eq!(written, file.contents, "{}", file.name);
    }
    let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert_eq!(entries.len(), files.len(), "the directory holds the declared files alone");
    // No staging directory is left beside it.
    let siblings: Vec<String> = std::fs::read_dir(dir.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(siblings.len(), 1, "{siblings:?}");
}

/// EAC-FR-BOXF, EAC-FR-WBIF, CDX-FR-OQVA, CDX-FR-WJZY — a scanned turn whose instruction holds the
/// block carries the reminder hook, which prints the reminder file; a plain turn carries no hook.
/// Claude Code declares no launch files, so its launch has no such mount.
#[test]
fn a_milestone_turn_gets_the_reminder_hook_and_a_plain_turn_does_not() {
    let harness = harness_for("codex");
    let runtime = codex_runtime();
    run(&harness, runtime.clone(), task(WITH_BLOCK)).expect("runs");
    let recorded = runtime.only_run();
    let argv = &recorded.argv;
    let at = argv.iter().position(|a| a == "--dangerously-bypass-hook-trust").expect("bypass");
    assert_eq!(argv[at + 1], "-c");
    assert_eq!(argv[at + 2], codex::reminder_hook_override());
    assert_eq!(argv.last().map(String::as_str), Some("-"));
    let dir = launch_files_mount(&recorded).expect("mounted").source;
    let printed =
        std::fs::read_to_string(std::path::Path::new(&dir).join("milestone-reminder.json"))
            .expect("the reminder file");
    assert_eq!(printed, codex::milestone_reminder_output());

    let runtime = codex_runtime();
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();
    assert_eq!(launch_files_mount(&recorded).expect("mounted").source, dir);
    assert!(!recorded.argv.iter().any(|a| a.starts_with("--dangerously-bypass-hook")));

    let claude = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&claude, runtime.clone(), task(WITH_BLOCK)).expect("runs");
    assert!(launch_files_mount(&runtime.only_run()).is_none());
}

/// EAC-FR-40, EAC-FR-BOXF — a supplied bundle that overlaps the launch-files directory is
/// refused.
#[test]
fn a_bundle_overlapping_the_launch_files_is_refused() {
    let root = tempfile::tempdir().expect("root");
    let fs = FsAccess::builder().allow_root(root.path()).build().expect("fs");
    let files = root.path().join("vendor-files").join("abc");
    std::fs::create_dir_all(&files).unwrap();
    let execution = root.path().join("execution").to_string_lossy().into_owned();
    let check = |host: &std::path::Path| {
        validate_supplementary_mount(
            &fs,
            &[root.path().to_path_buf()],
            host,
            &execution,
            None,
            Some(&files),
            &[],
        )
    };
    assert_eq!(
        check(&files).unwrap_err(),
        AgentExecutionError::SupplementaryMountInvalid("bundle overlaps the launch files".into())
    );
    // A bundle inside the launch files, or one that holds them, overlaps as well.
    let inside = files.join("nested");
    std::fs::create_dir_all(&inside).unwrap();
    let holding = root.path().join("vendor-files");
    for overlapping in [&inside, &holding] {
        assert_eq!(
            check(overlapping).unwrap_err(),
            AgentExecutionError::SupplementaryMountInvalid(
                "bundle overlaps the launch files".into()
            ),
            "{}",
            overlapping.display()
        );
    }
    let elsewhere = root.path().join("bundle");
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert!(check(&elsewhere).is_ok());
    // A sibling whose name only starts with the same text does not overlap.
    let sibling = root.path().join("vendor-files").join("abcd");
    std::fs::create_dir_all(&sibling).unwrap();
    assert!(check(&sibling).is_ok(), "a sibling is not inside the launch files");
}

/// EAC-FR-QHIU, EAC-FR-29 — the access token never reaches a watched turn's activity, the
/// invocation included.
#[test]
fn the_access_token_reaches_no_activity() {
    let harness = token_harness();
    let sink = Arc::new(CollectedActivity::default());
    run_watching(&harness, codex_runtime(), task("go"), sink.clone()).expect("runs");
    let rendered = sink.rendered();
    assert!(rendered.contains("CODEX_ACCESS_TOKEN"), "the name is shown");
    assert!(!rendered.contains(ACCESS_TOKEN), "the value is not");
}

/// CDX-FR-HVLL — the auth mode changes only the credential and the session-state directory:
/// a turn with a contract and the reminder has the same vendor arguments, the same stdin, and
/// reads the same stream in the `login_folder` and the `access_token` mode.
#[test]
fn the_auth_mode_changes_neither_the_vector_nor_the_stdin() {
    let launch = |harness: &Harness| {
        let runtime = codex_runtime();
        let mut request = task(WITH_BLOCK);
        request.result_contract = Some(ResultContract::WorkResult);
        let outcome = run(harness, runtime.clone(), request).expect("runs");
        assert_eq!(outcome.process_outcome, ProcessOutcome::Completed);
        let recorded = runtime.only_run();
        let stdin = String::from_utf8(recorded.stdin.clone())
            .expect("utf-8")
            .replace(&harness.workspace_target(), "<workspace>");
        (recorded.container.expect("a container").vendor_args, stdin, outcome.response)
    };
    let (login_args, login_stdin, login_answer) = launch(&harness_for("codex"));
    let (token_args, token_stdin, token_answer) = launch(&token_harness());
    assert_eq!(login_args, token_args);
    assert_eq!(login_stdin, token_stdin);
    assert_eq!(login_answer, token_answer);
}

/// EAC-FR-05, AIC-FR-15, AIC-FR-30 — a Codex record in the `access_token` mode whose token the
/// vault does not hold is degraded, so the turn is refused before the handoff and before any
/// container exists, and the refusal names no path of the user's home. The handoff's own
/// `token_missing` refusal is covered in `agentic/tests/codex_auth.rs`.
#[test]
fn an_access_token_launch_without_a_stored_token_creates_no_container() {
    let harness = harness_for("codex");
    let (mut records, active) = harness.store.load_agentic_registry().expect("registry");
    let codex = records.iter_mut().find(|r| r.vendor == "codex").expect("codex record");
    codex.codex_auth_mode = agentic::CodexAuthMode::AccessToken;
    harness.store.save_agentic_registry(records, active).expect("saved");

    let runtime = codex_runtime();
    let error = run(&harness, runtime.clone(), task("go")).unwrap_err();
    assert!(matches!(error, AgentExecutionError::IntegrationUnresolved(_)), "{error:?}");
    assert_eq!(runtime.launched(), 0, "the runtime was never touched");
    assert!(!format!("{error:?}").contains(&*harness.home.path().to_string_lossy()));
}

/// EAC-FR-QHIU, EAC-FR-29 — in the `access_token` mode no log record the launch writes, and no
/// rendering of its result, carries the token, also when the CLI fails and its stderr is
/// captured.
#[test]
fn no_log_record_of_an_access_token_launch_carries_the_token() {
    let harness = token_harness();
    for runtime in [
        codex_runtime(),
        RecordingRuntime::refused("", &format!("auth failed for {ACCESS_TOKEN}"), 1),
    ] {
        let outcome = run(&harness, runtime.clone(), task("go")).expect("runs");
        assert!(!format!("{outcome:?}").contains(ACCESS_TOKEN), "the result renders the token");
    }
    let page = super::super::log_buffer()
        .query(&crate::logging::LogFilter::default(), None, 50_000)
        .expect("query");
    let rendered = serde_json::to_string(&page).expect("serialise");
    assert!(rendered.contains("agent execution starting"), "the launch logged");
    assert!(!rendered.contains(ACCESS_TOKEN), "a log record carries the token");
}

/// EAC-FR-BOXF — a directory already at the digest's name is used as it is and never written
/// again, and a base that cannot hold a directory is a launch failure that leaves nothing
/// behind. A failed write inside a new staging directory has no seam here: the staging name is
/// unique per launch, so the test cannot place an obstacle in it.
#[test]
fn the_launch_files_directory_is_reused_and_a_failure_leaves_nothing() {
    let root = tempfile::tempdir().expect("root");
    let fs = FsAccess::builder().allow_root(root.path()).build().expect("fs");
    let executor =
        AgentCliExecutor::new(RecordingRuntime::replying("")).with_session_state_root(root.path());
    let files = descriptor::CODEX.launch_files();

    let first = executor.launch_files_dir(&fs, &files).expect("written");
    let marker = first.join("left-by-an-earlier-launch");
    std::fs::write(&marker, b"kept").unwrap();
    let again = executor.launch_files_dir(&fs, &files).expect("reused");
    assert_eq!(again, first);
    assert_eq!(std::fs::read(&marker).unwrap(), b"kept", "the directory was not rewritten");

    let blocked = tempfile::tempdir().expect("blocked root");
    std::fs::write(blocked.path().join("vendor-files"), b"a file, not a directory").unwrap();
    let fs = FsAccess::builder().allow_root(blocked.path()).build().expect("fs");
    let executor = AgentCliExecutor::new(RecordingRuntime::replying(""))
        .with_session_state_root(blocked.path());
    let error = executor.launch_files_dir(&fs, &files).unwrap_err();
    assert!(matches!(error, AgentExecutionError::LaunchFailed(_)), "{error:?}");
    let entries: Vec<_> = std::fs::read_dir(blocked.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "nothing is left beside the blocking file");
}
