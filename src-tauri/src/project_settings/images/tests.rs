//! The tests of the project image list: what the scan of a root finds, and
//! how a build line is read.

use super::*;

fn root_with(files: &[&str]) -> (tempfile::TempDir, crate::fs::RootFs) {
    let dir = tempfile::tempdir().unwrap();
    for file in files {
        let path = dir.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "FROM scratch\n").unwrap();
    }
    let root = crate::fs::RootFs::for_root(dir.path());
    (dir, root)
}

/// PSS-FR-23 (PSS-FR-23): the reference is the name alone, or the name and
/// the tag.
#[test]
fn the_image_reference_carries_the_tag_only_where_one_is_configured() {
    let untagged = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: None,
        dockerfile: None,
        build_context: None,
    };
    assert_eq!(untagged.image_reference().as_deref(), Some("acme/agent"));

    let tagged = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: Some("2.1".into()),
        dockerfile: None,
        build_context: None,
    };
    assert_eq!(tagged.image_reference().as_deref(), Some("acme/agent:2.1"));

    // A tag that is empty once trimmed is no tag at all, so Docker's own
    // default applies rather than a reference ending in a colon.
    let blank = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: Some("   ".into()),
        dockerfile: None,
        build_context: None,
    };
    assert_eq!(blank.image_reference().as_deref(), Some("acme/agent"));

    let nameless = ProjectVendorImage {
        image_name: "  ".into(),
        tag: Some("2.1".into()),
        dockerfile: None,
        build_context: None,
    };
    assert_eq!(nameless.image_reference(), None);
}

/// PSS-FR-25 (PSS-FR-24): the three refusals, each told apart from the
/// next.
#[test]
fn the_dockerfile_path_is_relative_inside_the_project_and_a_file() {
    let (dir, root) = root_with(&["docker/agent.Dockerfile"]);

    assert!(validate_dockerfile(&root, "docker/agent.Dockerfile").is_ok());
    assert_eq!(
        validate_dockerfile(&root, "/etc/Dockerfile").unwrap_err(),
        DockerfileProblem::AbsolutePath
    );
    assert_eq!(
        validate_dockerfile(&root, "../outside/Dockerfile").unwrap_err(),
        DockerfileProblem::EscapesProjectRoot
    );
    // A directory does not identify a regular file.
    assert_eq!(
        validate_dockerfile(&root, "docker").unwrap_err(),
        DockerfileProblem::NotARegularFile
    );
    // Nor does a path that names nothing.
    assert_eq!(
        validate_dockerfile(&root, "docker/absent.Dockerfile").unwrap_err(),
        DockerfileProblem::NotARegularFile
    );
    // A path that dips out and back in stays inside, and is judged on what
    // it names rather than on how it is spelled.
    assert!(validate_dockerfile(&root, "docker/../docker/agent.Dockerfile").is_ok());
    drop(dir);
}

/// PSS-FR-25 (PSS-FR-25): the four states a surface renders, and the
/// distinction between an absent Dockerfile and an invalid one.
#[test]
fn a_status_says_what_the_entry_is_worth() {
    let (_dir, root) = root_with(&["docker/agent.Dockerfile"]);

    // Nothing configured.
    let unset = status_for(&root, "claude_code", None);
    assert_eq!(unset.configuration, VendorImageConfiguration::Unset);
    assert_eq!(unset.dockerfile_state, DockerfileState::Absent);
    assert_eq!(unset.graduation_state, VendorGraduationState::ImageNameMissing);

    // Configured, no Dockerfile: not a problem, and usable.
    let no_build = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: None,
        dockerfile: None,
        build_context: None,
    };
    let status = status_for(&root, "claude_code", Some(&no_build));
    assert_eq!(status.configuration, VendorImageConfiguration::Configured);
    assert_eq!(status.dockerfile_state, DockerfileState::Absent);
    assert_eq!(status.dockerfile_problem, None);
    assert_eq!(status.graduation_state, VendorGraduationState::Usable);

    // Configured with a Dockerfile that is valid here.
    let buildable = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: Some("2.1".into()),
        dockerfile: Some("docker/agent.Dockerfile".into()),
        build_context: None,
    };
    let status = status_for(&root, "codex", Some(&buildable));
    assert_eq!(status.dockerfile_state, DockerfileState::Valid);
    assert_eq!(status.image_reference.as_deref(), Some("acme/agent:2.1"));
    assert_eq!(status.graduation_state, VendorGraduationState::Usable);

    // Configured with a Dockerfile that is not.
    let broken = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: None,
        dockerfile: Some("docker/gone.Dockerfile".into()),
        build_context: None,
    };
    let status = status_for(&root, "codex", Some(&broken));
    assert_eq!(status.dockerfile_state, DockerfileState::Invalid);
    assert_eq!(
        status.dockerfile_problem,
        Some(DockerfileProblem::NotARegularFile)
    );
    assert_eq!(
        status.graduation_state,
        VendorGraduationState::DockerfileInvalid
    );
    // And the entry still holds the path: reading validates nothing away.
    assert_eq!(status.dockerfile.as_deref(), Some("docker/gone.Dockerfile"));
}

/// PSS-FR-25 (PSS-FR-25, AIC-FR-33): OpenCode's entry is stored and may be
/// valid; what is missing is execution support.
#[test]
fn opencode_reports_execution_unsupported_rather_than_a_bad_entry() {
    let (_dir, root) = root_with(&["docker/agent.Dockerfile"]);
    let entry = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: None,
        dockerfile: Some("docker/agent.Dockerfile".into()),
        build_context: None,
    };
    let status = status_for(&root, "opencode", Some(&entry));
    assert_eq!(status.dockerfile_state, DockerfileState::Valid);
    assert_eq!(
        status.graduation_state,
        VendorGraduationState::ExecutionUnsupported
    );
    // Even with nothing configured at all, the answer is the same one.
    assert_eq!(
        status_for(&root, "opencode", None).graduation_state,
        VendorGraduationState::ExecutionUnsupported
    );
}

/// PSS-FR-23 / PSS-FR-25 (PSS-FR-23, PSS-FR-24): a save validates before it
/// writes.
#[test]
fn a_refused_entry_never_reaches_the_table() {
    let (_dir, root) = root_with(&["docker/agent.Dockerfile"]);
    assert_eq!(
        validate_entry(
            &root,
            &ProjectVendorImage {
                image_name: "   ".into(),
                tag: None,
                dockerfile: None,
                build_context: None,
            }
        )
        .unwrap_err(),
        ERR_IMAGE_NAME_EMPTY
    );
    // An entry that configures a Dockerfile must carry an image name too.
    assert_eq!(
        validate_entry(
            &root,
            &ProjectVendorImage {
                image_name: String::new(),
                tag: None,
                dockerfile: Some("docker/agent.Dockerfile".into()),
                build_context: None,
            }
        )
        .unwrap_err(),
        ERR_IMAGE_NAME_EMPTY
    );
    assert_eq!(
        validate_entry(
            &root,
            &ProjectVendorImage {
                image_name: "acme/agent".into(),
                tag: None,
                dockerfile: Some("/etc/Dockerfile".into()),
                build_context: None,
            }
        )
        .unwrap_err(),
        ERR_DOCKERFILE_ABSOLUTE
    );
}

/// PSS-FR-23 (PSS-FR-22): one vendor's entry is written without disturbing
/// another's, or any other section of the store.
#[test]
fn writing_one_vendor_carries_every_other_section_through() {
    let mut table = toml::Table::new();
    table.insert("lineEndings".into(), toml::Value::String("crlf".into()));
    write_entry(
        &mut table,
        "claude_code",
        &ProjectVendorImage {
            image_name: "acme/claude".into(),
            tag: Some("1".into()),
            dockerfile: None,
            build_context: None,
        },
    );
    write_entry(
        &mut table,
        "codex",
        &ProjectVendorImage {
            image_name: "acme/codex".into(),
            tag: None,
            dockerfile: Some("docker/codex.Dockerfile".into()),
            build_context: None,
        },
    );

    let entries = read_entries(&table);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries["claude_code"].image_name, "acme/claude");
    assert_eq!(entries["codex"].dockerfile.as_deref(), Some("docker/codex.Dockerfile"));
    assert_eq!(
        table.get("lineEndings").and_then(|v| v.as_str()),
        Some("crlf"),
        "an unrelated section survived the write"
    );

    // Rewriting one leaves the other exactly as it was.
    write_entry(
        &mut table,
        "claude_code",
        &ProjectVendorImage {
            image_name: "acme/claude".into(),
            tag: Some("2".into()),
            dockerfile: None,
            build_context: None,
        },
    );
    let entries = read_entries(&table);
    assert_eq!(entries["claude_code"].tag.as_deref(), Some("2"));
    assert_eq!(entries["codex"].image_name, "acme/codex");
}

/// PSS-FR-26 (PSS-FR-26): the vector is `docker build --file … --tag …
/// <root>` and carries no `--push`.
#[test]
fn the_cli_build_vector_is_local_only() {
    let args = docker_build_args(
        "docker/agent.Dockerfile",
        "acme/agent:2.1",
        Path::new("/projects/acme"),
    );
    assert_eq!(
        args,
        vec![
            "build".to_string(),
            "--file".to_string(),
            "docker/agent.Dockerfile".to_string(),
            "--tag".to_string(),
            "acme/agent:2.1".to_string(),
            "/projects/acme".to_string(),
        ]
    );
    assert!(!args.iter().any(|a| a == "--push"));
    assert!(!args.iter().any(|a| a == "push" || a == "login"));
}

/// PSS-FR-29 (PSS-FR-29): one build at a time, and every terminal result
/// releases it.
#[test]
fn a_project_builds_one_image_at_a_time() {
    let registry = ImageBuildRegistry::default();
    let cancel = registry.claim("claude_code", "op-1").unwrap();
    assert_eq!(
        registry.claim("codex", "op-2").unwrap_err(),
        ERR_BUILD_ALREADY_RUNNING
    );
    assert_eq!(
        registry.in_flight(),
        Some(InFlightImageBuild {
            vendor: "claude_code".to_string(),
            operation_id: "op-1".to_string()
        })
    );

    registry.cancel("op-1");
    assert!(cancel.load(std::sync::atomic::Ordering::SeqCst));

    registry.release("op-1");
    assert_eq!(registry.in_flight(), None);
    // A later build is neither blocked nor delayed by how the last one
    // ended.
    assert!(registry.claim("codex", "op-2").is_ok());
}

/// PSS-FR-29 (PSS-FR-29): releasing a build that is not the one in flight
/// changes nothing, so a late terminal result cannot free somebody else's
/// slot.
#[test]
fn a_stale_release_does_not_free_the_running_build() {
    let registry = ImageBuildRegistry::default();
    registry.claim("claude_code", "op-1").unwrap();
    registry.release("op-0");
    assert_eq!(
        registry.in_flight(),
        Some(InFlightImageBuild {
            vendor: "claude_code".to_string(),
            operation_id: "op-1".to_string()
        })
    );
}

// -- the build orchestration (PSS-FR-26..PSS-FR-29) ------------------

#[derive(Default)]
struct CollectedEvents {
    progress: Mutex<Vec<ImageBuildProgress>>,
    finished: Mutex<Vec<ImageBuildFinished>>,
}

impl BuildEvents for CollectedEvents {
    fn progress(&self, payload: ImageBuildProgress) {
        self.progress.lock().unwrap().push(payload);
    }
    fn finished(&self, payload: ImageBuildFinished) {
        self.finished.lock().unwrap().push(payload);
    }
}

/// A builder that replays a scripted outcome and records what it was asked
/// to build, so the orchestration is exercised with no Docker anywhere.
struct ScriptedBuilder {
    outcome: BuildTerminal,
    seen: Mutex<Vec<(ImageBuildRequest, ResolvedDockerBackend)>>,
    emit: Vec<(Option<u64>, Option<u64>, String)>,
}

impl ScriptedBuilder {
    fn new(outcome: BuildTerminal) -> Self {
        Self {
            outcome,
            seen: Mutex::new(Vec::new()),
            emit: Vec::new(),
        }
    }
}

impl ImageBuilder for ScriptedBuilder {
    fn build(
        &self,
        request: &ImageBuildRequest,
        backend: &ResolvedDockerBackend,
        sink: &dyn ImageBuildSink,
    ) -> BuildTerminal {
        self.seen
            .lock()
            .unwrap()
            .push((request.clone(), backend.clone()));
        for (completed, total, message) in &self.emit {
            sink.progress(PHASE_BUILDING, *completed, *total, message);
        }
        self.outcome.clone()
    }
}

fn verified_cli_backend() -> ResolvedDockerBackend {
    ResolvedDockerBackend::Cli {
        path: "/bin/docker".to_string(),
    }
}

fn buildable_entry() -> ProjectVendorImage {
    ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: Some("2.1".into()),
        dockerfile: Some("docker/agent.Dockerfile".into()),
        build_context: None,
    }
}

/// PSS-FR-26 (PSS-FR-26): a build targets the entry's reference and the
/// project root, through the backend that was resolved for it.
#[test]
fn a_build_targets_the_entrys_reference_and_the_project_root() {
    let registry = ImageBuildRegistry::default();
    let entry = buildable_entry();
    let prepared = prepare_build(
        Some(&entry),
        Ok(verified_cli_backend()),
        PathBuf::from("/projects/acme"),
        Ok(PathBuf::from("/projects/acme")),
        "claude_code",
        "op-1",
        &registry,
    )
    .unwrap();
    assert_eq!(prepared.request.image_reference, "acme/agent:2.1");
    assert_eq!(prepared.request.dockerfile, "docker/agent.Dockerfile");
    assert_eq!(prepared.request.context_root, PathBuf::from("/projects/acme"));

    let builder = ScriptedBuilder::new(BuildTerminal::Succeeded);
    let events = CollectedEvents::default();
    assert_eq!(
        run_prepared_build(prepared, &builder, &events, &registry),
        BuildTerminal::Succeeded
    );
    let seen = builder.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1, verified_cli_backend());
}

/// PSS-FR-26 (PSS-FR-26, GSS-FR-40): an unverified backend refuses and
/// starts nothing, and the refusal does not take the project's build slot.
#[test]
fn an_unverified_backend_refuses_before_anything_starts() {
    let registry = ImageBuildRegistry::default();
    let entry = buildable_entry();
    assert_eq!(
        prepare_build(
            Some(&entry),
            Err(crate::docker::ERR_DOCKER_BACKEND_UNVERIFIED.to_string()),
            PathBuf::from("/projects/acme"),
            Ok(PathBuf::from("/projects/acme")),
            "claude_code",
            "op-1",
            &registry,
        )
        .unwrap_err(),
        crate::docker::ERR_DOCKER_BACKEND_UNVERIFIED
    );
    assert_eq!(registry.in_flight(), None, "a refusal took no build slot");
}

/// PSS-FR-26 (PSS-FR-26): a vendor with no configured Dockerfile, and one
/// with no image name, are refused for their own reasons.
#[test]
fn a_build_needs_both_a_dockerfile_and_an_image_name() {
    let registry = ImageBuildRegistry::default();
    let no_dockerfile = ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: None,
        dockerfile: None,
        build_context: None,
    };
    assert_eq!(
        prepare_build(
            Some(&no_dockerfile),
            Ok(verified_cli_backend()),
            PathBuf::from("/p"),
            Ok(PathBuf::from("/p")),
            "codex",
            "op-1",
            &registry
        )
        .unwrap_err(),
        ERR_DOCKERFILE_UNSET
    );
    let no_name = ProjectVendorImage {
        image_name: String::new(),
        tag: None,
        dockerfile: Some("Dockerfile".into()),
        build_context: None,
    };
    assert_eq!(
        prepare_build(
            Some(&no_name),
            Ok(verified_cli_backend()),
            PathBuf::from("/p"),
            Ok(PathBuf::from("/p")),
            "codex",
            "op-1",
            &registry
        )
        .unwrap_err(),
        ERR_IMAGE_NAME_EMPTY
    );
    // And a vendor with no entry at all.
    assert_eq!(
        prepare_build(
            None,
            Ok(verified_cli_backend()),
            PathBuf::from("/p"),
            Ok(PathBuf::from("/p")),
            "codex",
            "op-1",
            &registry
        )
        .unwrap_err(),
        ERR_IMAGE_NAME_EMPTY
    );
    assert_eq!(registry.in_flight(), None);
}

/// PSS-FR-27 (PSS-FR-27, PSS-FR-28): a build starts indeterminate, may
/// become determinate, and ends in exactly one terminal event naming the
/// image.
#[test]
fn a_successful_build_reports_progress_then_one_terminal_result() {
    let registry = ImageBuildRegistry::default();
    let entry = buildable_entry();
    let prepared = prepare_build(
        Some(&entry),
        Ok(verified_cli_backend()),
        PathBuf::from("/projects/acme"),
        Ok(PathBuf::from("/projects/acme")),
        "claude_code",
        "op-1",
        &registry,
    )
    .unwrap();
    let mut builder = ScriptedBuilder::new(BuildTerminal::Succeeded);
    builder.emit = vec![
        (None, None, "Sending build context".to_string()),
        (Some(1), Some(3), "Step 1/3 : FROM debian".to_string()),
        (Some(3), Some(3), "Step 3/3 : RUN true".to_string()),
    ];
    let events = CollectedEvents::default();
    run_prepared_build(prepared, &builder, &events, &registry);

    let progress = events.progress.lock().unwrap().clone();
    assert_eq!(progress[0].completed, None, "a build starts indeterminate");
    assert_eq!(progress[0].total, None);
    assert!(progress.iter().all(|p| p.vendor == "claude_code"));
    assert!(progress.iter().all(|p| p.operation_id == "op-1"));
    assert!(progress.iter().all(|p| !p.phase.is_empty()));
    assert!(progress.iter().all(|p| !p.message.is_empty()));
    assert_eq!(progress.last().unwrap().total, Some(3));

    let finished = events.finished.lock().unwrap().clone();
    assert_eq!(finished.len(), 1, "exactly one terminal result");
    assert_eq!(finished[0].outcome, ImageBuildOutcome::Succeeded);
    assert_eq!(finished[0].image_reference.as_deref(), Some("acme/agent:2.1"));
    assert_eq!(finished[0].diagnostic, None);
    assert_eq!(registry.in_flight(), None, "the slot was released");
}

/// PSS-FR-28 (PSS-FR-28, PSS-FR-29): a failure names what went wrong, and
/// the project is free for the retry the author asks for.
#[test]
fn a_failed_build_carries_a_diagnostic_and_frees_the_project() {
    let registry = ImageBuildRegistry::default();
    let entry = buildable_entry();
    let prepared = prepare_build(
        Some(&entry),
        Ok(verified_cli_backend()),
        PathBuf::from("/projects/acme"),
        Ok(PathBuf::from("/projects/acme")),
        "codex",
        "op-1",
        &registry,
    )
    .unwrap();
    let builder = ScriptedBuilder::new(BuildTerminal::Failed("no such file".into()));
    let events = CollectedEvents::default();
    run_prepared_build(prepared, &builder, &events, &registry);

    let finished = events.finished.lock().unwrap().clone();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].outcome, ImageBuildOutcome::Failed);
    assert_eq!(finished[0].diagnostic.as_deref(), Some("no such file"));
    assert_eq!(finished[0].image_reference, None);

    // Nothing retried on its own, and the next build is neither blocked nor
    // delayed by how this one ended.
    assert_eq!(builder.seen.lock().unwrap().len(), 1);
    assert!(prepare_build(
        Some(&entry),
        Ok(verified_cli_backend()),
        PathBuf::from("/projects/acme"),
        Ok(PathBuf::from("/projects/acme")),
        "codex",
        "op-2",
        &registry
    )
    .is_ok());
}

/// PSS-FR-29 (PSS-FR-28): a cancellation is explicit, and the build reads
/// it.
#[test]
fn a_cancelled_build_reports_cancelled() {
    struct CancelWatcher;
    impl ImageBuilder for CancelWatcher {
        fn build(
            &self,
            _request: &ImageBuildRequest,
            _backend: &ResolvedDockerBackend,
            sink: &dyn ImageBuildSink,
        ) -> BuildTerminal {
            if sink.cancelled() {
                BuildTerminal::Cancelled
            } else {
                BuildTerminal::Succeeded
            }
        }
    }

    let registry = ImageBuildRegistry::default();
    let entry = buildable_entry();
    let prepared = prepare_build(
        Some(&entry),
        Ok(verified_cli_backend()),
        PathBuf::from("/projects/acme"),
        Ok(PathBuf::from("/projects/acme")),
        "claude_code",
        "op-1",
        &registry,
    )
    .unwrap();
    registry.cancel("op-1");
    let events = CollectedEvents::default();
    assert_eq!(
        run_prepared_build(prepared, &CancelWatcher, &events, &registry),
        BuildTerminal::Cancelled
    );
    let finished = events.finished.lock().unwrap().clone();
    assert_eq!(finished[0].outcome, ImageBuildOutcome::Cancelled);
    assert_eq!(finished[0].diagnostic, None);
    assert_eq!(finished[0].image_reference, None);
}

/// PSS-FR-29 (PSS-FR-29): a second build while one runs is refused, and the
/// running one is untouched.
#[test]
fn a_second_build_while_one_runs_is_refused() {
    let registry = ImageBuildRegistry::default();
    let entry = buildable_entry();
    let _first = prepare_build(
        Some(&entry),
        Ok(verified_cli_backend()),
        PathBuf::from("/p"),
        Ok(PathBuf::from("/p")),
        "claude_code",
        "op-1",
        &registry,
    )
    .unwrap();
    assert_eq!(
        prepare_build(
            Some(&entry),
            Ok(verified_cli_backend()),
            PathBuf::from("/p"),
            Ok(PathBuf::from("/p")),
            "codex",
            "op-2",
            &registry
        )
        .unwrap_err(),
        ERR_BUILD_ALREADY_RUNNING
    );
    assert_eq!(
        registry.in_flight(),
        Some(InFlightImageBuild {
            vendor: "claude_code".to_string(),
            operation_id: "op-1".to_string()
        })
    );
}

/// PSS-FR-28 (PSS-FR-27): a build is indeterminate until the backend
/// reports a total.
#[test]
fn a_step_line_is_what_makes_a_build_determinate() {
    assert_eq!(
        read_build_line("Step 4/17 : RUN apt-get update"),
        (Some((4, 17)), "Step 4/17 : RUN apt-get update".to_string())
    );
    assert_eq!(read_build_line("Sending build context…").0, None);
    assert_eq!(read_build_line(" ---> Running in abc123\n").0, None);
    // A malformed position is a message rather than a bad total.
    assert_eq!(read_build_line("Step x/y : RUN").0, None);
    assert_eq!(read_build_line("Step 9/0 : RUN").0, None);
}

// -- the build context (PSS-FR-BCTX, PSS-FR-BDFX) ---------------------

fn entry_with_context(context: Option<&str>) -> ProjectVendorImage {
    ProjectVendorImage {
        image_name: "acme/agent".into(),
        tag: None,
        dockerfile: Some("docker/agent.Dockerfile".into()),
        build_context: context.map(str::to_string),
    }
}

// PSS-FR-BCTX: a relative context resolves against the project root and may
// leave it. An absolute context is taken as given.
#[test]
fn a_build_context_is_relative_or_absolute_and_may_leave_the_project() {
    let outer = tempfile::tempdir().unwrap();
    let project = outer.path().join("project");
    std::fs::create_dir_all(project.join("docker/certs")).unwrap();
    std::fs::create_dir_all(outer.path().join("certs")).unwrap();
    let root = crate::fs::RootFs::for_root(&project);

    let inside = validate_build_context(&root, "docker/certs").unwrap();
    assert!(inside.ends_with("project/docker/certs"), "{inside:?}");
    let sibling = validate_build_context(&root, "../certs").unwrap();
    assert!(sibling.ends_with("certs") && !sibling.starts_with(project.canonicalize().unwrap()));
    let absolute = outer.path().join("certs");
    let resolved = validate_build_context(&root, absolute.to_str().unwrap()).unwrap();
    assert_eq!(resolved, absolute.canonicalize().unwrap());
}

// PSS-FR-BCTX: a value that names no existing folder is refused, and a file
// is not a folder.
#[test]
fn a_build_context_must_name_an_existing_folder() {
    let (_dir, root) = root_with(&["docker/agent.Dockerfile"]);
    for bad in ["missing", "docker/agent.Dockerfile", "../does-not-exist"] {
        assert_eq!(
            validate_build_context(&root, bad).unwrap_err(),
            ERR_BUILD_CONTEXT_NOT_A_DIRECTORY,
            "{bad}"
        );
        assert_eq!(
            validate_entry(&root, &entry_with_context(Some(bad))).unwrap_err(),
            ERR_BUILD_CONTEXT_NOT_A_DIRECTORY,
            "{bad}"
        );
    }
}

// PSS-FR-BCTX: an empty or blank context is the project root, and is stored
// as absent.
#[test]
fn an_empty_build_context_is_absent() {
    let (_dir, root) = root_with(&["docker/agent.Dockerfile"]);
    for blank in [None, Some(""), Some("   ")] {
        let validated = validate_entry(&root, &entry_with_context(blank)).unwrap();
        assert_eq!(validated.build_context, None);
    }
    let validated = validate_entry(&root, &entry_with_context(Some(" docker "))).unwrap();
    assert_eq!(validated.build_context.as_deref(), Some("docker"));
}

// PSS-FR-BCTX, PSS-FR-25: the status reports the context and whether it
// still names a folder, and the context takes no part in graduation.
#[test]
fn the_status_reports_the_build_context_state() {
    let (dir, root) = root_with(&["docker/agent.Dockerfile"]);
    std::fs::create_dir_all(dir.path().join("ctx")).unwrap();

    let absent = status_for(&root, "claude_code", Some(&entry_with_context(None)));
    assert_eq!(absent.build_context_state, BuildContextState::Absent);
    assert_eq!(absent.build_context, None);

    let valid = status_for(&root, "claude_code", Some(&entry_with_context(Some("ctx"))));
    assert_eq!(valid.build_context_state, BuildContextState::Valid);
    assert_eq!(valid.build_context.as_deref(), Some("ctx"));
    assert_eq!(valid.graduation_state, VendorGraduationState::Usable);

    std::fs::remove_dir_all(dir.path().join("ctx")).unwrap();
    let invalid = status_for(&root, "claude_code", Some(&entry_with_context(Some("ctx"))));
    assert_eq!(invalid.build_context_state, BuildContextState::Invalid);
    assert_eq!(invalid.graduation_state, VendorGraduationState::Usable);
}

// PSS-FR-22: the context is committed with the entry under its camelCase key.
#[test]
fn the_build_context_round_trips_through_the_table() {
    let mut table = toml::Table::new();
    write_entry(&mut table, "codex", &entry_with_context(Some("../certs")));
    let text = toml::to_string(&table).unwrap();
    assert!(text.contains("buildContext = \"../certs\""), "{text}");
    let read = read_entries(&table);
    assert_eq!(read["codex"].build_context.as_deref(), Some("../certs"));

    write_entry(&mut table, "codex", &entry_with_context(None));
    assert!(!toml::to_string(&table).unwrap().contains("buildContext"));
}

// PSS-FR-BDFX: a Dockerfile inside the context is named relative to it, and
// one outside is placed under the unique name.
#[test]
fn the_dockerfile_placement_follows_the_context() {
    let outer = tempfile::tempdir().unwrap();
    let project = outer.path().join("project");
    std::fs::create_dir_all(project.join("docker/ctx")).unwrap();
    std::fs::create_dir_all(outer.path().join("certs")).unwrap();
    std::fs::write(project.join("docker/ctx/Dockerfile"), "FROM scratch\n").unwrap();
    std::fs::write(project.join("docker/agent.Dockerfile"), "FROM scratch\n").unwrap();

    assert_eq!(
        dockerfile_placement(&project, &project, "docker/agent.Dockerfile"),
        DockerfilePlacement::InContext("docker/agent.Dockerfile".into())
    );
    assert_eq!(
        dockerfile_placement(&project, &project.join("docker/ctx"), "docker/ctx/Dockerfile"),
        DockerfilePlacement::InContext("Dockerfile".into())
    );
    assert_eq!(
        dockerfile_placement(&project, &project.join("docker/ctx"), "docker/agent.Dockerfile"),
        DockerfilePlacement::Outside
    );
    assert_eq!(
        dockerfile_placement(&project, &outer.path().join("certs"), "docker/agent.Dockerfile"),
        DockerfilePlacement::Outside
    );
}

// PSS-FR-BDFX: a `..` segment in the Dockerfile path is folded before the
// path is compared with the context, so a file that leaves the context through
// it counts as outside.
#[test]
fn a_parent_segment_in_the_dockerfile_path_is_folded() {
    let outer = tempfile::tempdir().unwrap();
    let project = outer.path().join("project");
    std::fs::create_dir_all(project.join("docker/ctx")).unwrap();
    assert_eq!(
        dockerfile_placement(&project, &project.join("docker/ctx"), "docker/ctx/../agent.Dockerfile"),
        DockerfilePlacement::Outside
    );
    assert_eq!(
        dockerfile_placement(&project, &project.join("docker/ctx"), "docker/../docker/ctx/Dockerfile"),
        DockerfilePlacement::InContext("Dockerfile".into())
    );
}

// PSS-FR-BDFX: the Engine option names the Dockerfile relative to the context,
// or the unique name when the file lies outside it.
#[test]
fn the_engine_dockerfile_option_follows_the_placement() {
    assert_eq!(
        engine_dockerfile_option(&DockerfilePlacement::InContext("docker/a.Dockerfile".into())),
        "docker/a.Dockerfile"
    );
    assert_eq!(engine_dockerfile_option(&DockerfilePlacement::Outside), PSS_DOCKERFILE_NAME);
}

// PSS-FR-26: the CLI receives a Windows path without its verbatim prefix.
#[test]
fn the_cli_context_argument_has_no_verbatim_prefix() {
    let args = docker_build_args("Dockerfile", "a:1", Path::new(r"\\?\C:\certs"));
    assert_eq!(args.last().unwrap(), r"C:\certs");
    let args = docker_build_args("Dockerfile", "a:1", Path::new("/work/certs"));
    assert_eq!(args.last().unwrap(), "/work/certs");
}

// PSS-FR-26, PSS-FR-BCTX: a context that no longer names a folder refuses a
// build before the build slot is taken, and a good one reaches the request.
#[test]
fn a_build_refuses_a_missing_context_and_carries_a_good_one() {
    let registry = ImageBuildRegistry::default();
    let entry = buildable_entry();
    assert_eq!(
        prepare_build(
            Some(&entry),
            Ok(verified_cli_backend()),
            PathBuf::from("/projects/acme"),
            Err(ERR_BUILD_CONTEXT_NOT_A_DIRECTORY.to_string()),
            "claude_code",
            "op-1",
            &registry,
        )
        .unwrap_err(),
        ERR_BUILD_CONTEXT_NOT_A_DIRECTORY
    );
    assert_eq!(registry.in_flight(), None, "a refusal took no build slot");

    let prepared = prepare_build(
        Some(&entry),
        Ok(verified_cli_backend()),
        PathBuf::from("/projects/acme"),
        Ok(PathBuf::from("/certs")),
        "claude_code",
        "op-2",
        &registry,
    )
    .unwrap();
    assert_eq!(prepared.request.project_root, PathBuf::from("/projects/acme"));
    assert_eq!(prepared.request.context_root, PathBuf::from("/certs"));
    assert_eq!(prepared.request.placement, DockerfilePlacement::Outside);
}
