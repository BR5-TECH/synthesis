//! EAC-FR-40 … EAC-FR-42 — the semantic-rebase mount, its bundle, and repository access.

use super::*;

// ---------------------------------------------------------------------------
// EAC-FR-40, EAC-FR-13 … EAC-FR-42, EAC-FR-29, EAC-FR-32, EAC-FR-34 — the semantic-rebase capability
// ---------------------------------------------------------------------------

/// EAC-FR-40 / GRB-FR-TOOU (EAC-FR-40, EAC-FR-13, GRB-FR-TOOU, GRB-FR-VZHV): the
/// bundle is mounted read-only at the fixed path, both vendors receive the
/// identical mount, the source worktree and the repository directory are
/// mounted nowhere, and no task field could have named a mount.
#[test]
fn eac_ts52_the_semantic_rebase_mount_is_fixed_read_only_and_identical_per_vendor() {
    // Each vendor launches from its own store — EAC-FR-40 requires a bundle to
    // sit under the application's own, which a build under test roots at the
    // harness's directory — so what "identical" claims here is the **contract**
    // the container sees: the same fixed target, the same access mode, and the
    // same document path. The host source is per-harness by construction.
    let mut targets: Vec<String> = Vec::new();
    for vendor in ["claude_code", "codex"] {
        let harness = harness_for(vendor);
        let bundle = rebase_bundle(&harness.sessions_root(), "rebase-bundle");
        let runtime = RecordingRuntime::replying(&stdout_for(vendor, &envelope_json("success")));
        run_with_mount(
            &harness,
            runtime.clone(),
            Some(SupplementaryMount::SemanticRebaseArtifact {
                host_path: bundle.clone(),
            }),
            None,
        )
        .expect("runs");

        let recorded = runtime.only_run();
        let mount = recorded
            .argv
            .iter()
            .find(|a| a.contains("target=/rebase"))
            .unwrap_or_else(|| panic!("{vendor}: no /rebase mount in {:?}", recorded.argv));
        assert!(mount.contains(",readonly"), "{vendor}: {mount}");
        // The container-facing half of the mount, which is the part both
        // vendors must receive identically.
        let container_facing: String = mount
            .split(',')
            .filter(|part| !part.starts_with("source="))
            .collect::<Vec<_>>()
            .join(",");
        targets.push(container_facing);

        // GRB-FR-VZHV: the mount set is the execution directory, the artifact,
        // and the vendor's own — no source worktree and no repository directory
        // anywhere in it.
        let mounts: Vec<&String> = recorded
            .argv
            .iter()
            .filter(|a| a.starts_with("type=bind"))
            .collect();
        for mount in &mounts {
            assert!(
                mount.contains(&format!("target={}", harness.workspace_target()))
                    || mount.contains("target=/rebase")
                    || mount.contains(&harness.session_state_dir(vendor))
                    || mount.contains("/.codex"),
                "{vendor}: an unexpected mount: {mount}",
            );
        }

        // The stdin document names no mount and carries no bundle content.
        let stdin = String::from_utf8_lossy(&recorded.stdin).to_string();
        assert!(!stdin.contains("/rebase"), "{vendor}: the task names a mount");
        assert!(!stdin.contains("artifact_version"), "{vendor}");
    }
    // Both vendors received the identical target, mode, and document path.
    assert_eq!(targets[0], targets[1]);
    assert_eq!(targets[0], "type=bind,target=/rebase,readonly");

    // And no field of the task can express a mount, a container path, or an
    // access mode.
    let source = include_str!("../protocol.rs");
    let task_struct = source
        .split("pub struct AgentTaskRequest {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("the task struct");
    for forbidden in ["mount", "container_path", "read_only", "readonly"] {
        assert!(
            !task_struct.contains(forbidden),
            "AgentTaskRequest must not carry a {forbidden} field"
        );
    }
}

/// EAC-FR-13 (EAC-FR-40): an ordinary execution launches with exactly the
/// mounts it always had.
#[test]
fn eac_ts52_an_ordinary_execution_carries_no_rebase_mount() {
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("runs");
    let recorded = runtime.only_run();
    assert!(
        !recorded.argv.iter().any(|a| a.contains("/rebase")),
        "an ordinary execution names no rebase mount: {:?}",
        recorded.argv
    );
}

/// FSA-FR-KVWD, GSU-FR-RJRF (EAC-FR-08, EAC-FR-40): every refused bundle is refused before any
/// container is created.
#[test]
fn eac_ts53_an_invalid_bundle_is_refused_before_any_container() {
    let harness = harness_for("claude_code");
    let root = harness.sessions_root();

    let missing = root.join("absent");
    let a_file = root.join("a-file");
    std::fs::write(&a_file, b"not a directory").unwrap();
    let traversal = root.join("..").join("elsewhere");

    let too_large = root.join("too-large");
    std::fs::create_dir_all(&too_large).unwrap();
    std::fs::write(
        too_large.join("huge.json"),
        vec![b'x'; (SEMANTIC_REBASE_FILE_LIMIT + 1) as usize],
    )
    .unwrap();

    let overlapping = harness.workspace().join("inside");
    std::fs::create_dir_all(&overlapping).unwrap();

    // EAC-FR-40: the vendor's **session-state** directory, which the container
    // may write to — a bundle inside it would sit behind a writable mount.
    let session_state = PathBuf::from(harness.session_state_dir("claude_code"));
    let in_session_state = session_state.join("bundle");
    std::fs::create_dir_all(&in_session_state).unwrap();

    // EAC-FR-40: outside the application's own store, and a link out of it —
    // which the same canonical comparison closes.
    let outside = tempfile::tempdir().expect("outside");
    let outside_bundle = rebase_bundle(outside.path(), "elsewhere");
    let linked = root.join("linked-out");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_bundle, &linked).expect("link");
    #[cfg(not(unix))]
    std::fs::create_dir_all(&linked).unwrap();

    for (host, what) in [
        (missing, "missing"),
        (a_file, "a regular file"),
        (traversal, "a parent segment"),
        (too_large, "over the file limit"),
        (overlapping, "overlapping the execution directory"),
        (in_session_state, "overlapping the session-state directory"),
        (session_state.clone(), "the session-state directory itself"),
        (outside_bundle, "outside the application's store"),
        (linked, "reaching through a link out of the store"),
    ] {
        let runtime = RecordingRuntime::replying(&stdout_for(
            "claude_code",
            &envelope_json("success"),
        ));
        let failure = run_with_mount(
            &harness,
            runtime.clone(),
            Some(SupplementaryMount::SemanticRebaseArtifact { host_path: host }),
            None,
        )
        .expect_err(what);
        assert!(
            matches!(failure, AgentExecutionError::SupplementaryMountInvalid(_)),
            "{what}: got {failure:?}"
        );
        assert_eq!(runtime.launched(), 0, "{what}: a container was created");
    }

    // EAC-FR-08: **the bundle whole carries no bound.** A report of many
    // hundreds of files whose total is far past any one file's limit, and every
    // one of which is inside it, is mounted rather than refused — the agent
    // reads it one file at a time through its index, so a total would refuse
    // what nobody reads at once. One file sits **exactly on** the bound, which
    // is the size a truncated side is written at.
    let bulk = rebase_bundle(&root, "bulk");
    let side = vec![b'x'; 24 * 1024];
    let at_the_bound = vec![b'x'; SEMANTIC_REBASE_FILE_LIMIT as usize];
    // Nested project paths under each mirror, which is the depth the producer
    // actually writes and the depth the validator's walk has to reach.
    for n in 0..400 {
        let path = format!("src/area{}/nested/deep/file{n:04}.ts", n % 7);
        for mirror in ["source", "run", "merged"] {
            let host = bulk.join(mirror).join(&path);
            std::fs::create_dir_all(host.parent().unwrap()).unwrap();
            std::fs::write(&host, &side).unwrap();
        }
    }
    // One file sits **exactly on** the bound, which is the size a cut file is
    // written at.
    std::fs::write(
        bulk.join("merged/src/area0/nested/deep/file0000.ts"),
        &at_the_bound,
    )
    .unwrap();

    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(
        &harness,
        runtime.clone(),
        Some(SupplementaryMount::SemanticRebaseArtifact {
            host_path: bulk.clone(),
        }),
        None,
    )
    .expect("a bundle past any total is mounted");
    assert_eq!(runtime.launched(), 1, "the container was not created");
    // The mount really reached the argv rather than merely surviving validation.
    let recorded = runtime.only_run();
    assert!(
        recorded
            .argv
            .iter()
            .any(|value| value.contains(&format!("target={SEMANTIC_REBASE_TARGET}"))),
        "the bundle is not mounted at {SEMANTIC_REBASE_TARGET}"
    );
}

/// EAC-FR-FNFV, EAC-FR-40 / GRB-FR-VZHV (EAC-FR-41, GRB-FR-VZHV): repository metadata inside the
/// execution directory is masked for a semantic-rebase execution — the `.git`
/// entry a linked worktree carries at its root and every nested one beneath it
/// — and for no other execution.
#[test]
fn eac_ts55_repository_metadata_is_masked_for_a_semantic_rebase_execution() {
    let harness = harness_for("claude_code");
    let workspace = harness.workspace();
    // A linked worktree's `.git` file at the root, and a nested checkout.
    std::fs::write(workspace.join(".git"), b"gitdir: /elsewhere/.git/worktrees/x").unwrap();
    std::fs::create_dir_all(workspace.join("vendor/nested/.git")).unwrap();
    // EAC-FR-41: **every** nested `.git`, including one a package manager
    // vendored. A dependency directory is no floor here: a checkout under
    // `node_modules` is a whole repository, and an unmasked one is exactly what
    // this rule exists to prevent.
    std::fs::create_dir_all(workspace.join("node_modules/some-pkg/.git")).unwrap();

    let bundle = rebase_bundle(&harness.sessions_root(), "masking-bundle");
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(
        &harness,
        runtime.clone(),
        Some(SupplementaryMount::SemanticRebaseArtifact {
            host_path: bundle.clone(),
        }),
        None,
    )
    .expect("runs");

    let recorded = runtime.only_run();
    let mask_source = |masked: &str| -> PathBuf {
        let mount = recorded
            .argv
            .iter()
            .find(|a| a.contains(masked))
            .unwrap_or_else(|| panic!("no mask for {masked} in {:?}", recorded.argv));
        assert!(mount.contains(",readonly"), "{mount}");
        let source = mount
            .split(',')
            .find_map(|part| part.strip_prefix("source="))
            .unwrap_or_else(|| panic!("no source in {mount}"));
        PathBuf::from(source)
    };

    // EAC-FR-41: **each entry is masked by something of its own kind.** The
    // root `.git` of a linked worktree — which every graduation run is given —
    // is a regular *file*, and the kernel refuses a bind mount of a directory
    // onto a file with ENOTDIR, so a directory mask there would not hide the
    // repository: it would fail the launch.
    let workspace = harness.workspace_target();
    let root_mask = mask_source(&format!("target={workspace}/.git"));
    assert!(
        root_mask.is_file(),
        "a `.git` file must be masked by a file, not {root_mask:?}",
    );
    assert_eq!(
        std::fs::read(&root_mask).expect("readable"),
        Vec::<u8>::new(),
        "and the file it is masked by holds nothing",
    );
    for nested in [
        format!("target={workspace}/vendor/nested/.git"),
        format!("target={workspace}/node_modules/some-pkg/.git"),
    ] {
        let mask = mask_source(&nested);
        assert!(
            mask.is_dir(),
            "a `.git` directory must be masked by a directory, not {mask:?} ({nested})",
        );
        assert!(
            std::fs::read_dir(&mask)
                .expect("readable")
                .next()
                .is_none(),
            "and the directory it is masked by is empty",
        );
    }

    // EAC-FR-41: the second condition on its own. A turn that named
    // `semantic_rebase` and supplied no mount is masked all the same, so a turn
    // that reached the executor under the wrong name still reads no repository.
    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_as(&harness, runtime.clone(), None, None, super::super::TurnKind::SemanticRebase).expect("runs");
    assert!(
        runtime
            .only_run()
            .argv
            .iter()
            .any(|a| a.contains(&format!("target={}/.git", harness.workspace_target()))),
        "the turn kind alone masks",
    );
}

/// EAC-FR-40 (EAC-FR-41, EAC-FR-FNFV): **masking and the read-only grant are
/// exclusive**, over a directory that really is a repository.
///
/// Asserted on a real linked worktree rather than on a `.git` pointing nowhere:
/// against a directory that resolves to no repository the grant is absent
/// whatever the guard says, so the whole exclusivity clause would pass with the
/// guard deleted. This is the test that fails when it is.
#[test]
fn eac_ts55_masking_and_repository_access_are_exclusive() {
    let harness = harness_for("claude_code");
    linked_worktree_at(&harness.sessions_root(), &harness.workspace());

    let argv_for = |kind: super::super::TurnKind, mount: Option<SupplementaryMount>| -> Vec<String> {
        let runtime =
            RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
        run_as(&harness, runtime.clone(), mount, None, kind).expect("runs");
        runtime.only_run().argv
    };
    let masked = |argv: &[String]| {
        argv.iter()
            .any(|a| a.contains(&format!("target={}/.git", harness.workspace_target()))
                && a.contains("repository-mask"))
    };
    // Read off the Git environment rather than a mount target: the grant carries
    // it in every shape (EAC-FR-ZKMR), where the store's own target is one path
    // in the aligned shape and another in the fallback one. A predicate written
    // against a single target would pass against the other shape while proving
    // nothing.
    let granted = |argv: &[String]| argv.iter().any(|a| a.starts_with("GIT_OPTIONAL_LOCKS"));

    // An ordinary turn over a real repository: granted, and masked nowhere.
    let ordinary = argv_for(super::super::TurnKind::Work, None);
    assert!(granted(&ordinary), "the store is mounted: {ordinary:?}");
    assert!(!masked(&ordinary), "and nothing is masked: {ordinary:?}");

    // The same directory, named as the one kind EAC-FR-41 masks.
    let rebase = argv_for(super::super::TurnKind::SemanticRebase, None);
    assert!(masked(&rebase), "the turn kind masks: {rebase:?}");
    assert!(!granted(&rebase), "and the grant is withheld: {rebase:?}");

    // And by the mount, which is the other of the two conditions.
    let bundle = rebase_bundle(&harness.sessions_root(), "exclusivity-bundle");
    let with_mount = argv_for(
        super::super::TurnKind::Work,
        Some(SupplementaryMount::SemanticRebaseArtifact { host_path: bundle }),
    );
    assert!(masked(&with_mount), "the mount masks: {with_mount:?}");
    assert!(!granted(&with_mount), "and the grant is withheld: {with_mount:?}");
}

/// EAC-FR-FNFV (EAC-FR-FNFV): a run's repository is mounted **read-only**, its
/// stored credentials are masked, and the `.git` the next turn resolves from is
/// pinned so this turn cannot redirect it.
///
/// The linked-worktree case is the one that matters: every graduation run in
/// `git` mode is given one, and its `.git` is a *file* recording a host path
/// that means nothing inside a container.
#[test]
fn eac_ts_tuaa_a_turn_reaches_its_repository_read_only() {
    let harness = harness_for("claude_code");
    let (store, _git_dir) = linked_worktree_at(&harness.sessions_root(), &harness.workspace());
    let workspace = harness.workspace_target();

    let runtime = RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
    run_with_mount(&harness, runtime.clone(), None, None).expect("runs");
    let argv = runtime.only_run().argv;

    let mount_for = |target: &str| -> String {
        argv.iter()
            .find(|a| a.contains(&format!("target={target},")) || a.ends_with(&format!("target={target}")))
            .unwrap_or_else(|| panic!("no mount for {target} in {argv:?}"))
            .clone()
    };

    // EAC-FR-ZKMR: the store at its own path. The turn and the application then
    // name one string for one file, and the records the worktree carries name
    // paths the container has.
    // Canonical, because that is what Git reports a store as and therefore what
    // the mount names — on macOS `/var` is a link to `/private/var`.
    let store_target = std::fs::canonicalize(&store)
        .expect("the store")
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let mounted = mount_for(&store_target);
    assert!(mounted.contains(",readonly"), "{mounted}");
    assert!(
        mounted.contains(&format!("source={store_target}")),
        "the store is mounted over itself: {mounted}",
    );
    assert!(
        !argv.iter().any(|a| a.contains("/gitcommon")),
        "no fixed store path is used where the host's own serves: {argv:?}",
    );
    assert_eq!(
        mounted,
        format!("type=bind,source={store_target},target={store_target},readonly"),
        "one string on both sides, and read-only",
    );
    assert!(
        !argv
            .iter()
            .filter(|a| a.starts_with("type=bind"))
            .any(|a| a.contains("//")),
        "no mount is composed through a doubled separator: {argv:?}",
    );

    // The credential mask. A remote URL routinely carries a token inside it, so
    // a store mounted without this hands every ordinary turn the project's
    // stored credentials.
    let masked_config = mount_for(&format!("{store_target}/config"));
    assert!(masked_config.contains(",readonly"), "{masked_config}");
    assert!(
        masked_config.contains("repository-mask"),
        "config is covered by the empty mask: {masked_config}",
    );

    // EAC-FR-FNFV: the `.git` the next turn resolves from is pinned. It is the
    // worktree's own file here, mounted over itself: it already names a path
    // the container has, and nothing about it is generated.
    let pin = mount_for(&format!("{workspace}/.git"));
    assert!(pin.contains(",readonly"), "{pin}");
    assert!(
        pin.contains(&format!("source={workspace}/.git")),
        "the pin is the worktree's own file: {pin}",
    );
    assert!(
        !argv.iter().any(|a| a.contains("repository-pointer") || a.contains("repository-commondir")),
        "nothing is generated in this shape: {argv:?}",
    );

    // EAC-FR-ZKMR: and the record that pin carries names a path **inside the
    // store this launch mounted**. That is the whole assumption the aligned
    // shape rests on, where the fallback shape generates a pointer that is
    // correct by construction: a worktree recording its store under another
    // spelling would resolve to nothing inside the container.
    let recorded = std::fs::read_to_string(harness.workspace().join(".git")).expect("the pin");
    let named = recorded
        .trim()
        .strip_prefix("gitdir: ")
        .expect("a gitdir pointer")
        .to_string();
    assert!(
        named.starts_with(&format!("{store_target}/")),
        "the worktree resolves its store inside the mounted one: {named} vs {store_target}",
    );
    assert!(
        Path::new(&named).join("commondir").exists(),
        "and the Git directory it names is really there: {named}",
    );

    // The command line the turn runs, asked where the container's paths and the
    // host's are the same paths — which is what the aligned shape makes them,
    // so the host's own worktree is the container's view of it.
    if git_cli_available() {
        for args in [&["log", "--oneline", "-1"][..], &["status", "--short"][..]] {
            let out = git_in(&harness.workspace(), args);
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr),
            );
        }
    }

    // EAC-FR-FNFV: a store before the mask that covers an entry inside it, or
    // the cover is applied to a path the store then hides.
    let position_of = |mount: &str| {
        argv.iter()
            .position(|a| a == mount)
            .unwrap_or_else(|| panic!("{mount} is not in {argv:?}"))
    };
    assert!(
        position_of(&mounted) < position_of(&masked_config),
        "the store is mounted before what covers its entries",
    );

    // EAC-FR-ZKMR: as little as it can be told, in either shape. A `GIT_DIR`
    // exported to the whole container redirects **every** Git command in it,
    // including the ones a build or a test suite makes about some other
    // directory entirely.
    for absent in ["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR"] {
        assert!(
            !argv.iter().any(|a| a.starts_with(&format!("{absent}="))),
            "{absent} must not be exported: {argv:?}",
        );
    }
    for expected in [
        "GIT_OPTIONAL_LOCKS=0",
        "GIT_CONFIG_COUNT=1",
        "GIT_CONFIG_KEY_0=safe.directory",
        "GIT_CONFIG_VALUE_0=*",
    ] {
        assert!(argv.iter().any(|a| a == expected), "{expected} missing from {argv:?}");
    }

    // --- the fallback shape, composed for the same worktree ----------------
    //
    // Where the container cannot be given the host's paths, the store stands at
    // the fixed path and the two records that then name nothing the container
    // has are generated and pinned.
    let fallback = repository_mounts_in(&harness, &harness.workspace(), false)
        .expect("the fallback shape composes");
    let targets: Vec<&str> = fallback.iter().map(|m| m.target.as_str()).collect();
    assert_eq!(
        targets,
        vec![
            "/gitcommon",
            "/gitcommon/config",
            "/gitcommon/worktrees/run-1/commondir",
            "/workspace/.git",
        ],
        "the store, its mask, the generated record, and the generated pointer",
    );
    assert!(fallback.iter().all(|m| m.read_only), "{fallback:?}");
    let pointer = target_of(&fallback, "/workspace/.git");
    assert!(
        pointer.source.contains("repository-pointer"),
        "the pin is the generated pointer in this shape: {pointer:?}",
    );
    assert_eq!(
        std::fs::read_to_string(&pointer.source).unwrap().trim(),
        "gitdir: /gitcommon/worktrees/run-1",
        "and it names the container path the store was mounted at",
    );
    let record = target_of(&fallback, "commondir");
    assert_eq!(
        std::fs::read_to_string(&record.source).unwrap().trim(),
        "../..",
        "the generated record names the store relative to the Git directory",
    );

    // Rendered, because a mount set is not an invocation: each has to reach the
    // argv in the runtime's own form, and each has to come **after** the
    // read/write execution-directory mount it is nested inside — the pin at
    // `/workspace/.git` is covered by `/workspace` itself.
    let spec = descriptor::ContainerSpec {
        name: "synthesis-agent-1".to_string(),
        image: "image:test".to_string(),
        host_uid: 501,
        host_gid: 20,
        workdir: descriptor::WORKSPACE_TARGET.to_string(),
        mounts: std::iter::once(BindMount {
            source: "/host/worktree".to_string(),
            target: descriptor::WORKSPACE_TARGET.to_string(),
            read_only: false,
        })
        .chain(fallback.iter().cloned())
        .collect(),
        env_names: vec![],
        env_literals: vec![],
        vendor_args: vec![],
    };
    let rendered = spec.docker_run_args();
    let at = |target: &str| {
        rendered
            .iter()
            .position(|a| a.ends_with(&format!("target={target}")) || a.contains(&format!("target={target},")))
            .unwrap_or_else(|| panic!("{target} is not in {rendered:?}"))
    };
    assert!(
        at("/workspace") < at("/gitcommon")
            && at("/workspace") < at("/workspace/.git")
            && at("/gitcommon") < at("/gitcommon/config"),
        "a mount is applied after whatever it sits inside: {rendered:?}",
    );
    assert!(
        rendered
            .iter()
            .filter(|a| a.starts_with("type=bind"))
            .filter(|a| a.contains("/gitcommon") || a.contains("/workspace/.git"))
            .all(|a| a.ends_with(",readonly")),
        "and every one of them is read-only: {rendered:?}",
    );

    // The plain shape in the same fallback: its store is inside the working
    // tree, so it follows `/workspace` and no second mount is made.
    let checkout = harness.sessions_root().join("ordinary");
    std::fs::create_dir_all(&checkout).unwrap();
    git2::Repository::init(&checkout).expect("a checkout");
    let plain = repository_mounts_in(&harness, &checkout, false).expect("composes");
    assert_eq!(
        plain.iter().map(|m| m.target.as_str()).collect::<Vec<_>>(),
        vec!["/workspace/.git", "/workspace/.git/config"],
        "no store mount, and nothing generated",
    );
}

/// EAC-FR-FNFV (EAC-FR-FNFV): a worktree whose Git directory records its store
/// as an **absolute host path** still resolves inside the container.
///
/// libgit2 — which is what this application creates every worktree with —
/// writes that record as the host path, and Git reads it before it reads
/// anything else. So without the generated record the turn's first Git command
/// fails on a path the container does not have, whatever else is mounted
/// correctly. The container is stood in for by materialising the mounts the
/// argv names, with the host store removed afterwards: what is left is exactly
/// what the turn can reach.
#[test]
fn eac_ts_tuaa_a_worktree_recording_an_absolute_store_still_resolves() {
    let harness = harness_for("claude_code");
    let (store, git_dir) = linked_worktree_at(&harness.sessions_root(), &harness.workspace());
    std::fs::write(harness.workspace().join("a.md"), b"one\n").unwrap();

    // The record libgit2 left. Absolute, which is what the fallback shape has to
    // answer for: it names a path that shape's container does not have.
    //
    // This pins the library's behaviour rather than Git's: a git2 that came to
    // write the relative form would fail here, which is news worth reading
    // rather than a regression.
    let own = std::fs::read_to_string(git_dir.join("commondir")).expect("a record");
    assert!(
        Path::new(own.trim()).is_absolute(),
        "the worktree records its store as a host path: {own}",
    );

    let fallback = repository_mounts_in(&harness, &harness.workspace(), false)
        .expect("the fallback shape composes");
    let source_of = |target: &str| target_of(&fallback, target).source.clone();

    // Materialise what that shape's container sees. `/gitcommon` and
    // `/workspace` are absolute container paths, so they are re-rooted under one
    // temporary directory — which is the point: a **relative** record resolves
    // under any root, and an absolute one under none but its own.
    let view = harness.sessions_root().join("container-view");
    let mounted_store = view.join("gitcommon");
    let mounted_workspace = view.join("workspace");
    copy_tree(&PathBuf::from(source_of("/gitcommon")), &mounted_store);
    copy_tree(&harness.workspace(), &mounted_workspace);
    std::fs::write(
        mounted_workspace.join(".git"),
        format!("gitdir: {}/worktrees/run-1\n", mounted_store.display()),
    )
    .unwrap();
    // The store's back-pointer names where the worktree stands.
    std::fs::write(
        mounted_store.join("worktrees/run-1/gitdir"),
        format!("{}/.git\n", mounted_workspace.display()),
    )
    .unwrap();

    // The host store is gone, exactly as it is absent from that container.
    std::fs::remove_dir_all(&store).unwrap();

    // What the turn would stand on without the generated record: the worktree's
    // own, naming a host path that is not there. Nothing resolves.
    assert!(
        git2::Repository::open(&mounted_workspace)
            .and_then(|r| r.head().map(|_| ()))
            .is_err(),
        "an absolute host record is what fails the turn's first Git command",
    );

    // And with the generated record over it.
    let record = mounted_store.join("worktrees/run-1/commondir");
    std::fs::copy(
        source_of("/gitcommon/worktrees/run-1/commondir"),
        &record,
    )
    .unwrap();

    let repo = git2::Repository::open(&mounted_workspace).expect("the worktree resolves");
    let mut walk = repo.revwalk().expect("a revision walk");
    walk.push_head().expect("a HEAD to walk from");
    assert!(walk.count() > 0, "there is history to log");
    repo.statuses(None).expect("a status to read");
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    repo.diff_tree_to_workdir_with_index(Some(&tree), None)
        .expect("a diff against HEAD");

    // The control: a record that names somewhere neither mount is fails. Without
    // it this test would pass on a Git that never read the record at all.
    std::fs::write(&record, b"../../nowhere\n").unwrap();
    assert!(
        git2::Repository::open(&mounted_workspace)
            .and_then(|r| r.head().map(|_| ()))
            .is_err(),
        "the record is what the resolution goes through",
    );
    std::fs::write(&record, b"../..\n").unwrap();

    // The turn runs the Git **command line**, and libgit2 agreeing that a record
    // resolves is not the same client agreeing — it was the command line that
    // failed. It is asked under the container-faithful shape, which the
    // assertions above are not: the store's back-pointer to the worktree keeps
    // the host path Git wrote there, and no container has that path. The command
    // line reads the back-pointer only where it is asked about worktree
    // registrations, so the ordinary commands still answer — where libgit2 opens
    // it for a status, which is why that client is asked under the other shape.
    std::fs::write(
        mounted_store.join("worktrees/run-1/gitdir"),
        b"/nowhere-a-container-has/.git\n",
    )
    .unwrap();
    if git_cli_available() {
        for args in [
            &["log", "--oneline", "-1"][..],
            &["status", "--short"][..],
            &["diff", "HEAD", "--stat"][..],
        ] {
            let out = git_in(&mounted_workspace, args);
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr),
            );
        }

        // And the same client under the record the worktree carries itself,
        // which is the failure the generated one exists to prevent.
        std::fs::write(&record, own.as_bytes()).unwrap();
        assert!(
            !git_in(&mounted_workspace, &["status", "--short"]).status.success(),
            "an absolute host record fails the command line the turn runs",
        );
    }
}

/// Whether this machine has a Git command line to ask.
fn git_cli_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// One Git command in a working copy, told what the executor tells it
/// (EAC-FR-FNFV) and nothing else.
fn git_in(workdir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "safe.directory")
        .env("GIT_CONFIG_VALUE_0", "*")
        .output()
        .expect("a git command line")
}
