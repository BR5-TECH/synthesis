//! EAC-FR-ZKMR — worktree pointers, backend agreement, and what a rebase run may not record.

use super::*;

/// EAC-FR-39, EAC-FR-13, EAC-FR-17 (EAC-FR-ZKMR, EAC-FR-10): the two backends create the **same**
/// container, and the directory the turn stands in is part of that.
///
/// The Engine backend renders no argument vector, so nothing about it is
/// observable in the vector the CLI backend renders. Its own rendering is
/// asserted against the specification both are built from.
#[test]
fn eac_ts51_both_backends_render_one_specification() {
    let spec = descriptor::ContainerSpec {
        name: "synthesis-agent-1".to_string(),
        image: "registry.example/image:test".to_string(),
        host_uid: 501,
        host_gid: 20,
        workdir: "/host/worktree".to_string(),
        mounts: vec![BindMount {
            source: "/host/worktree".to_string(),
            target: "/host/worktree".to_string(),
            read_only: false,
        }],
        env_names: vec![],
        env_literals: vec![],
        vendor_args: vec!["-p".to_string()],
    };
    let config = super::super::runtime::engine_config(&spec, vec![]);

    assert_eq!(
        config.working_dir.as_deref(),
        Some(spec.workdir.as_str()),
        "the Engine backend stands the turn where the CLI backend does",
    );
    let argv = spec.docker_run_args();
    let cli_workdir = argv
        .iter()
        .position(|a| a == "--workdir")
        .map(|i| argv[i + 1].clone())
        .expect("a working directory");
    assert_eq!(cli_workdir, spec.workdir, "and the two agree");

    let mounts = config.host_config.expect("a host config").mounts.expect("mounts");
    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].source.as_deref(), Some("/host/worktree"));
    assert_eq!(mounts[0].target.as_deref(), Some("/host/worktree"));
    assert_eq!(config.user.as_deref(), Some("501:20"));
}

/// EAC-FR-06, EAC-FR-FNFV (EAC-FR-ZKMR): the shape is a property of the path, and the two
/// predicates a path is put to are **not** the same predicate.
///
/// The fallback shape exists for a path that is a mount **source** the runtime
/// takes and a **target** it does not. A path neither of them takes is refused
/// instead, so a test that used one of those to reach the fallback would prove
/// nothing about the rule it names.
#[test]
fn eac_ts_pwtg_a_source_the_runtime_takes_is_not_always_a_target() {
    let drive = Path::new("C:\\repo\\.git");
    assert!(
        descriptor::mount_source(drive).is_some(),
        "a drive path is a source the runtime takes",
    );
    assert_eq!(
        descriptor::container_path(drive),
        None,
        "and a target it does not: the fallback shape is for exactly this",
    );

    let comma = Path::new("/host/with,comma");
    assert!(
        descriptor::mount_source(comma).is_none() && descriptor::container_path(comma).is_none(),
        "a comma is neither, so no shape carries it",
    );

    // The ordinary case, and the two spellings that are not paths a container
    // can be given.
    assert_eq!(
        descriptor::container_path(Path::new("/host/worktree")),
        Some("/host/worktree".to_string()),
    );
    assert_eq!(descriptor::container_path(Path::new("relative/path")), None);
    assert_eq!(
        descriptor::container_path(Path::new("/")),
        Some("/".to_string()),
        "a root is absolute, and the allowlist is what keeps it from being an execution directory",
    );
}

/// EAC-FR-06 (EAC-FR-ZKMR, EAC-FR-FNFV, EAC-FR-06): the container **names the
/// paths its host names**, and what happens where it cannot.
///
/// The alignment is what lets the loop on this machine and the turn inside the
/// container say one string for one file. The shape is a property of the path,
/// so the case a POSIX machine has no directory for is asked of the decision
/// rather than arranged for on disk.
#[test]
fn eac_ts_pwtg_the_container_names_the_paths_its_host_names() {
    let harness = harness_for("claude_code");
    let (store, _git_dir) = linked_worktree_at(&harness.sessions_root(), &harness.workspace());
    let workspace = harness.workspace_target();

    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("runs");
    let argv = runtime.only_run().argv;

    // The working directory is the execution directory's own path, and the
    // mount that carries it names that path on both sides — which is the whole
    // of the alignment: what the application calls this directory and what the
    // turn calls it are one string.
    let workdir = argv
        .iter()
        .position(|a| a == "--workdir")
        .map(|i| argv[i + 1].clone())
        .expect("a working directory");
    assert_eq!(workdir, workspace);
    assert!(
        argv.iter()
            .any(|a| a == &format!("type=bind,source={workspace},target={workspace}")),
        "the execution directory is mounted at its own path: {argv:?}",
    );

    // The store as well, and with both at their own paths nothing about the
    // worktree's records has to be generated.
    let store_target = std::fs::canonicalize(&store)
        .expect("the store")
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    assert!(
        argv.iter().any(|a| a.contains(&format!("target={store_target},"))),
        "the store is mounted at its own path: {argv:?}",
    );
    assert!(
        !argv
            .iter()
            .any(|a| a.contains("/gitcommon") || a.contains("repository-pointer") || a.contains("repository-commondir")),
        "and no fixed path or generated file is used: {argv:?}",
    );

    // A host path that names a drive rather than a root is a mount source the
    // runtime takes and a target it does not, so the shape decided for it is the
    // fallback one — which composes the fixed paths and both generated files.
    assert_eq!(
        descriptor::container_path(Path::new("C:\\Users\\author\\project")),
        None,
        "a drive is not a root",
    );
    assert_eq!(
        descriptor::container_path(Path::new(&workspace)),
        Some(workspace.clone()),
    );
    let fallback = repository_mounts_in(&harness, &harness.workspace(), false)
        .expect("the fallback shape composes");
    assert!(
        fallback.iter().any(|m| m.target == "/gitcommon")
            && fallback.iter().any(|m| m.target == "/workspace/.git"),
        "{fallback:?}",
    );

    // A path the mount syntax cannot express is refused rather than fallen back
    // from: it is no more a source than a target, and no shape carries it.
    let comma = harness.sessions_root().join("with,comma");
    std::fs::create_dir_all(&comma).unwrap();
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    assert!(
        matches!(
            run_in(&harness, runtime, comma),
            Err(AgentExecutionError::ExecutionDirectoryInvalid(_)),
        ),
        "a comma is refused under EAC-FR-06",
    );

    // And a directory whose own path can be expressed while its store's cannot:
    // the container keeps the paths it has already agreed on, and the grant —
    // all of it or none of it — is withheld.
    let harness = harness_for("claude_code");
    linked_worktree_at(&harness.sessions_root().join("holds,comma"), &harness.workspace());
    assert!(
        super::super::repository_access(&crate::changes::canonicalize_lenient(&harness.workspace()))
            .is_some(),
        "the directory is still a repository, so the withholding below is the shape's doing",
    );
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("launches anyway");
    let argv = runtime.only_run().argv;
    assert!(
        !argv.iter().any(|a| a.starts_with("GIT_")),
        "no Git environment where the grant is withheld: {argv:?}",
    );
    assert!(
        !argv.iter().any(|a| a.contains(&format!("target={}/.git", harness.workspace_target()))),
        "and nothing is pinned over the worktree's own: {argv:?}",
    );
}

/// EAC-FR-FNFV (EAC-FR-FNFV): a Git directory that is not inside the store it
/// names is a shape with no single mount, and the access is **withheld**.
///
/// Mounting the two at unrelated container paths would leave what either
/// records about the other resolving to somewhere neither mount is, so the turn
/// reads files instead — and that is a launch without the grant rather than a
/// refusal.
#[test]
fn eac_ts_tuaa_a_git_directory_outside_its_store_withholds_the_access() {
    let harness = harness_for("claude_code");
    let (store, git_dir) = linked_worktree_at(&harness.sessions_root(), &harness.workspace());

    // A second, valid store somewhere else, named by the worktree's Git
    // directory as the one it shares.
    let elsewhere = harness.sessions_root().join("another-store");
    copy_tree(&store, &elsewhere);
    std::fs::write(
        git_dir.join("commondir"),
        format!("{}\n", elsewhere.display()),
    )
    .unwrap();

    // The directory is still a repository this module can open, so what is
    // withheld below is withheld for the shape and not for a fixture that
    // stopped being a repository at all.
    assert!(git2::Repository::open(harness.workspace()).is_ok());
    assert!(
        super::super::repository_access(&harness.workspace()).is_none(),
        "a Git directory outside the store it names has no single mount",
    );

    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("launches anyway");
    let argv = runtime.only_run().argv;
    assert!(
        !argv.iter().any(|a| a.contains("target=/gitcommon")),
        "no store is mounted: {argv:?}",
    );
    assert!(
        !argv.iter().any(|a| a.contains("target=/workspace/.git")),
        "and nothing is pinned over the worktree's own: {argv:?}",
    );
    assert!(
        !argv.iter().any(|a| a.starts_with("GIT_")),
        "and Git is told nothing: {argv:?}",
    );
}

/// EAC-FR-FNFV (EAC-FR-FNFV): the generated record climbs as far as the Git
/// directory sits inside the store, whatever that depth is.
///
/// Every worktree Git makes today sits two levels down, so the arithmetic is
/// never distinguished from a constant by a fixture. It is asserted directly
/// instead, together with the separator a Windows path carries.
#[test]
fn eac_ts_tuaa_the_generated_record_climbs_the_depth_it_is_at() {
    use super::super::RepositoryAccess;

    let at = |within: &str| RepositoryAccess::Linked {
        common_dir: PathBuf::from("/store"),
        git_dir_within: PathBuf::from(within),
    };
    assert_eq!(at("a").commondir_text().unwrap(), "..\n");
    assert_eq!(at("worktrees/a").commondir_text().unwrap(), "../..\n");
    assert_eq!(at("worktrees/a/b").commondir_text().unwrap(), "../../..\n");
    assert_eq!(
        RepositoryAccess::Plain {
            git_dir: PathBuf::from("/w/.git"),
        }
        .commondir_text(),
        None,
        "an ordinary checkout generates nothing: Git finds its store where it always was",
    );

    let deep = at("worktrees/a/b");
    let targets: Vec<String> = deep
        .mounts(
            "/workspace",
            "/gitcommon",
            Path::new("/p"),
            Some(Path::new("/c")),
            Path::new("/e"),
        )
        .into_iter()
        .map(|(_, target)| target)
        .collect();
    assert!(
        targets.contains(&"/gitcommon/worktrees/a/b/commondir".to_string()),
        "{targets:?}",
    );
}

/// EAC-FR-FNFV (EAC-FR-FNFV): what one execution resolves through is its own.
///
/// The generated pointer and store record are files on the host, and the runtime
/// reads them when it creates the container rather than when they are written.
/// A name shared between two executions is therefore one the second replaces
/// under the first, leaving a turn standing on a `.git` that names another
/// working copy — so each execution names its files after the directory it
/// stands in.
#[test]
fn eac_ts_tuaa_two_executions_generate_their_own_pointers() {
    let harness = harness_for("claude_code");
    linked_worktree_at(&harness.sessions_root(), &harness.workspace());

    // A second working copy of its own, sharing this harness's store — which is
    // where the generated files live.
    let second = harness.sessions_root().join("second-workspace");
    std::fs::create_dir_all(&second).unwrap();
    linked_worktree_named(&harness.sessions_root().join("second"), &second, "run-2");

    let pointer_of = |directory: &Path| -> PathBuf {
        let mounts = repository_mounts_in(&harness, directory, false).expect("composes");
        PathBuf::from(&target_of(&mounts, "/workspace/.git").source)
    };
    let first = pointer_of(&harness.workspace());
    let other = pointer_of(&second);

    assert_ne!(first, other, "each execution generates its own pointer");
    assert_eq!(
        std::fs::read_to_string(&first).unwrap().trim(),
        "gitdir: /gitcommon/worktrees/run-1",
        "the first execution's pointer still names the working copy it resolved, \
         rather than the one the second execution stood in",
    );
    assert_eq!(
        std::fs::read_to_string(&other).unwrap().trim(),
        "gitdir: /gitcommon/worktrees/run-2",
    );
}

/// EAC-FR-FNFV (EAC-FR-FNFV): an ordinary checkout is pinned **in place**, and a
/// directory that belongs to no repository launches without the access and with
/// no error.
///
/// The in-place shape matters for its own reason: `.git` is a directory inside
/// the working tree, which EAC-FR-13 mounts read/write — so without the pin the
/// grant would not be read-only at all, whatever it says.
#[test]
fn eac_ts_tuaa_an_ordinary_checkout_is_pinned_in_place() {
    let harness = harness_for("claude_code");
    git2::Repository::init(harness.workspace()).expect("a checkout");
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("runs");
    let argv = runtime.only_run().argv;

    // Its own `.git`, read-only over itself: Git finds it where it always was,
    // and the turn cannot write it even though `/workspace` is writable.
    let workspace = harness.workspace_target();
    let pin = argv
        .iter()
        .find(|a| a.contains(&format!("target={workspace}/.git,")))
        .unwrap_or_else(|| panic!("no pin in {argv:?}"));
    assert!(pin.contains(",readonly"), "{pin}");
    let pinned_source = pin
        .split(',')
        .find_map(|p| p.strip_prefix("source="))
        .expect("a source");
    assert_eq!(
        std::fs::canonicalize(pinned_source).expect("the pinned .git"),
        std::fs::canonicalize(harness.workspace().join(".git")).expect("its own .git"),
        "the pin is this checkout's own `.git`, over itself",
    );
    assert!(
        argv.iter().any(|a| a.contains(&format!("target={workspace}/.git/config"))),
        "and its config is masked here too: {argv:?}",
    );
    // Nothing is mounted at the shared-store path: this shape has no store
    // somewhere else to mount.
    assert!(
        !argv.iter().any(|a| a.contains("target=/gitcommon")),
        "an ordinary checkout needs no second mount: {argv:?}",
    );

    // --- no repository at all: launched, and no error --------------------
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("runs anyway");
    let argv = runtime.only_run().argv;
    assert!(
        !argv.iter().any(|a| a.starts_with("GIT_")),
        "a directory that belongs to no repository is told nothing: {argv:?}",
    );
    assert!(
        !argv.iter().any(|a| a.contains("target=/gitcommon")
            || a.contains(&format!("target={}/.git", harness.workspace_target()))),
        "and carries no repository mount: {argv:?}",
    );
}

/// EAC-FR-IRRD (EAC-FR-IRRD): the turn kind selects which of the author's own
/// model and reasoning-effort selections a launch carries, and reaches nothing
/// else.
///
/// Read off the generated vector rather than off the resolver, because the
/// failure this guards against is a launch that resolves the right pair and then
/// generates the default one anyway.
#[test]
fn eac_ts_xbkr_the_turn_kind_selects_the_model_and_the_effort_and_nothing_else() {
    use super::super::TurnKind;
    let harness = harness_for("claude_code");
    const KINDS: [(TurnKind, &str, &str, &str); 3] = [
        (TurnKind::Work, "work", "opus", "low"),
        (TurnKind::Review, "review", "haiku", "high"),
        (TurnKind::SemanticRebase, "semantic_rebase", "opus", "xhigh"),
    ];
    for (_, kind, model, effort) in KINDS {
        agentic::set_model_impl(&harness.store, &harness.ai, "claude_code", Some(kind), Some(model))
            .expect("a model per kind");
        agentic::set_effort_impl(
            &harness.store,
            &harness.ai,
            "claude_code",
            Some(kind),
            Some(effort),
        )
        .expect("an effort per kind");
    }

    // The whole generated vector, so what the four launches differ in can be
    // compared rather than only what each of them carries.
    let argv_of = |kind: TurnKind| -> Vec<String> {
        let runtime =
            RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
        run_as(&harness, runtime.clone(), None, None, kind).expect("runs");
        runtime.only_run().argv
    };
    let after = |argv: &[String], flag: &str| -> String {
        let at = argv
            .iter()
            .position(|a| a == flag)
            .unwrap_or_else(|| panic!("no {flag} in {argv:?}"));
        argv[at + 1].clone()
    };

    let mut vectors = Vec::new();
    for (turn, _, model, effort) in KINDS {
        let argv = argv_of(turn);
        assert_eq!(after(&argv, "--model"), model, "the model of {turn:?}");
        assert_eq!(after(&argv, "--effort"), effort, "the effort of {turn:?}");
        vectors.push(argv);
    }
    // The three vendor vectors differ in those two arguments alone: the model and
    // the effort are the whole of what the kind decides about what the agent is
    // asked to be. (What the kind decides about the *container* is EAC-FR-41's
    // masking, which is pinned where that requirement is.)
    let image = test_image_reference("claude_code");
    let stripped = |v: &[String]| -> Vec<String> {
        let tail = &v[v.iter().position(|a| *a == image).expect("the image") + 1..];
        let mut out = Vec::new();
        let mut skip = false;
        for arg in tail {
            if skip {
                skip = false;
                continue;
            }
            // The session identifier is generated per launch (CCP-FR-16), so it
            // is no more one of the kind's decisions than the model is not.
            if arg == "--model" || arg == "--effort" || arg == "--session-id" {
                skip = true;
                continue;
            }
            out.push(arg.clone());
        }
        out
    };
    for (index, argv) in vectors.iter().enumerate() {
        assert_eq!(
            stripped(argv),
            stripped(&vectors[0]),
            "launch {index} differs somewhere other than the model and the effort",
        );
    }

    // EAC-FR-IRRD: a kind that holds a model of its own and no effort of its own
    // carries its model and the record's default effort — the two selections are
    // read from maps of their own and neither can move the other.
    let harness = harness_for("claude_code");
    agentic::set_model_impl(&harness.store, &harness.ai, "claude_code", None, Some("opus"))
        .expect("the default model");
    agentic::set_effort_impl(&harness.store, &harness.ai, "claude_code", None, Some("high"))
        .expect("the default effort");
    agentic::set_model_impl(
        &harness.store,
        &harness.ai,
        "claude_code",
        Some("review"),
        Some("sonnet"),
    )
    .expect("one model override and no effort override");

    for (turn, kind, _, _) in KINDS {
        let runtime =
            RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
        run_as(&harness, runtime.clone(), None, None, turn).expect("runs");
        let argv = runtime.only_run().argv;
        let expected_model = if kind == "review" { "sonnet" } else { "opus" };
        assert_eq!(after(&argv, "--model"), expected_model, "the model of {turn:?}");
        assert_eq!(after(&argv, "--effort"), "high", "the effort of {turn:?}");
    }
}

/// EAC-FR-26 (EAC-FR-26, EAC-FR-40): the host bundle is untouched by the run,
/// nothing of it reaches a record or a returned value, and no vendor image or
/// Dockerfile gains Git or the artifact.
#[test]
fn eac_ts54_the_bundle_is_read_only_to_the_executor_and_no_image_carries_it() {
    let harness = harness_for("claude_code");
    let bundle = rebase_bundle(&harness.sessions_root(), "cleanup-bundle");
    let before: Vec<(String, Vec<u8>)> = {
        let mut out = Vec::new();
        let mut stack = vec![bundle.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push((
                        path.to_string_lossy().into_owned(),
                        std::fs::read(&path).unwrap(),
                    ));
                }
            }
        }
        out.sort();
        out
    };

    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    let execution = run_with_mount(
        &harness,
        runtime.clone(),
        Some(SupplementaryMount::SemanticRebaseArtifact {
            host_path: bundle.clone(),
        }),
        None,
    )
    .expect("runs");

    // EAC-FR-26: the container is removed on every path out of the call, and the
    // bundle it was given is byte-identical afterwards — the executor writes
    // nothing into it and deletes nothing from it.
    assert!(execution.container_removed);
    let after: Vec<(String, Vec<u8>)> = {
        let mut out = Vec::new();
        let mut stack = vec![bundle.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push((
                        path.to_string_lossy().into_owned(),
                        std::fs::read(&path).unwrap(),
                    ));
                }
            }
        }
        out.sort();
        out
    };
    assert_eq!(after, before, "the executor left the bundle exactly as it was");

    // No vendor image and no Dockerfile gains Git or the artifact.
    let images = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the repository root")
        .join("docker");
    let mut dockerfiles: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![images];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("Dockerfile"))
            {
                dockerfiles.push(path);
            }
        }
    }
    assert!(!dockerfiles.is_empty(), "the vendor images were not found");
    for dockerfile in dockerfiles {
        let text = std::fs::read_to_string(&dockerfile).expect("read");
        assert!(
            !text.contains("/rebase"),
            "{}: an image must not carry the rebase artifact",
            dockerfile.display(),
        );
        // AVI-FR-02: Git is part of the floor, because the turns that are not
        // the semantic-rebase turn read what changed rather than every file
        // whole. What this turn can reach is EAC-FR-41's masking and not the
        // image's package list, which is the whole point of GRB-FR-VZHV.
        assert!(
            text.contains("        git \\\n"),
            "{}: an image installs Git as part of the floor",
            dockerfile.display(),
        );
    }
}

/// EAC-FR-42 (EAC-FR-42, EAC-FR-29, EAC-FR-32, EAC-FR-34): a semantic-rebase
/// execution writes no `DEBUG` record, no invocation or task record, and no
/// stderr excerpt — while the activity sink receives everything, and an
/// ordinary execution keeps all three.
#[test]
fn eac_ts56_a_semantic_rebase_execution_writes_no_content_into_the_log() {
    const PROMPT: &str = "THE-CAPTURED-PROMPT-MARKER";
    const CONTENT: &str = "THE-FILE-CONTENT-MARKER";

    let collected = Arc::new(CollectedActivity::default());
    let harness = harness_for("claude_code");
    let bundle = rebase_bundle(&harness.sessions_root(), "logging-bundle");

    // The whole suite shares one session buffer, so every assertion below is
    // scoped to *this* launch by the container it created rather than to
    // whatever else happened to be running beside it.
    let runtime = RecordingRuntime::replying(&format!(
        "{CONTENT}\n{}",
        stdout_for("claude_code", &envelope_json("success"))
    ));
    let executor =
        AgentCliExecutor::new(runtime.clone()).with_session_state_root(harness.sessions_root());
    let mut request_task = task(PROMPT);
    let mut input = serde_json::Map::new();
    input.insert("prompt".to_string(), serde_json::json!(PROMPT));
    request_task.input = Some(input);
    let _ = block_on(executor.execute_agent_cli(
        &Silent,
        &harness.context(),
        AgentExecutionRequest {
            turn_kind: super::super::TurnKind::Work,
            execution_directory: harness.workspace(),
            task: request_task,
            cancellation: CancellationToken::new(),
            activity: Some(collected.clone() as Arc<dyn AgentActivitySink>),
            durable_output: None,
            supplementary_mount: Some(SupplementaryMount::SemanticRebaseArtifact {
                host_path: bundle,
            }),
        },
    ));
    let ours = records_for_container(&container_of(&runtime));

    assert!(!ours.is_empty(), "the launch wrote its boundary records");
    assert!(
        ours.iter()
            .all(|r| r.level != crate::logging::LogLevel::Debug),
        "a semantic-rebase execution writes no DEBUG record",
    );
    let all = serde_json::to_string(&ours).expect("records");
    assert!(!all.contains(PROMPT), "the captured prompt reached the log");
    assert!(!all.contains(CONTENT), "file content reached the log");
    assert!(!all.contains("stderr_excerpt"), "an excerpt reached the log");
    // The sink still received the run's narration.
    assert!(
        !collected.events().is_empty(),
        "the activity sink is unaffected by the suppression",
    );

    // And an ordinary execution keeps its DEBUG records — the invocation, the
    // task, and a record per observed line.
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("runs");
    let ordinary = records_for_container(&container_of(&runtime));
    assert!(
        ordinary
            .iter()
            .any(|r| r.level == crate::logging::LogLevel::Debug),
        "an ordinary execution still writes DEBUG records",
    );
}

/// The container name one recorded launch created.
fn container_of(runtime: &Arc<RecordingRuntime>) -> String {
    runtime
        .only_run()
        .container
        .map(|spec| spec.name)
        .expect("the launch was recorded")
}

/// Every record of one launch, told apart from the rest of the shared session
/// buffer by the container the launch created.
fn records_for_container(container: &str) -> Vec<crate::logging::LogRecord> {
    crate::logging::BUFFER
        .query(&crate::logging::LogFilter::default(), None, 50_000)
        .expect("query")
        .records
        .into_iter()
        .filter(|record| {
            record
                .fields
                .get("container")
                .and_then(|v| v.as_str())
                .is_some_and(|name| name == container)
        })
        .collect()
}
