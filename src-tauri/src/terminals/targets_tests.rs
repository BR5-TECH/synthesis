use super::*;

fn vendor_images_env() -> Environment {
    Environment {
        docker_verified: true,
        work_vendor: Some(CliVendor::ClaudeCode),
        vendor_images: [CliVendor::ClaudeCode].into_iter().collect(),
        verified_vendors: [(CliVendor::ClaudeCode, "/bin/claude".to_string())]
            .into_iter()
            .collect(),
        ..Environment::default()
    }
}

fn find(
    kinds: &[KindAvailability],
    kind: TerminalKind,
    vendor: Option<CliVendor>,
) -> KindAvailability {
    kinds.iter().find(|k| k.kind == kind && k.vendor == vendor).cloned().expect("kind listed")
}

// TRS-FR-QHKU: a Host shell is available everywhere.
#[test]
fn host_shell_is_always_available() {
    let kinds = kinds_for(&Environment::default(), false);
    let host = find(&kinds, TerminalKind::HostShell, None);
    assert!(host.available);
    assert_eq!(host.runs_in, RunsIn::Host);
}

// TRS-FR-IRBL: a Container shell needs a verified backend and the work vendor's image.
#[test]
fn container_shell_needs_backend_and_image() {
    let ok = find(&kinds_for(&vendor_images_env(), false), TerminalKind::ContainerShell, None);
    assert!(ok.available);
    assert_eq!(ok.runs_in, RunsIn::Docker);

    let mut env = vendor_images_env();
    env.docker_verified = false;
    let no_docker = find(&kinds_for(&env, false), TerminalKind::ContainerShell, None);
    assert_eq!(no_docker.reason, Some(UnavailableReason::DockerUnverified));

    let mut env = vendor_images_env();
    env.work_vendor = Some(CliVendor::Codex);
    let no_image = find(&kinds_for(&env, false), TerminalKind::ContainerShell, None);
    assert_eq!(no_image.reason, Some(UnavailableReason::VendorImageUnconfigured));
}

// TRS-FR-IRBL: in a dev container project, the Container shell needs a ready dev container.
#[test]
fn container_shell_in_a_devcontainer_project() {
    let mut env = Environment { devcontainer_project: true, ..Environment::default() };
    let not_ready = find(&kinds_for(&env, false), TerminalKind::ContainerShell, None);
    assert_eq!(not_ready.reason, Some(UnavailableReason::DevcontainerNotReady));
    assert_eq!(not_ready.runs_in, RunsIn::Devcontainer);
    env.devcontainer_ready = true;
    assert!(find(&kinds_for(&env, false), TerminalKind::ContainerShell, None).available);
}

// TRS-FR-KJYH, TRS-FR-XXPE: a Microsandbox shell needs microsandbox
// isolation and a running VM, and is never offered in a dev container project.
#[test]
fn microsandbox_shell_rules() {
    let mut env = vendor_images_env();
    let shell =
        |env: &Environment, vm| find(&kinds_for(env, vm), TerminalKind::MicrosandboxShell, None);
    assert_eq!(shell(&env, true).reason, Some(UnavailableReason::IsolationNotMicrosandbox));
    env.effective_microsandbox = true;
    assert_eq!(shell(&env, false).reason, Some(UnavailableReason::NoVmRunning));
    assert!(shell(&env, true).available);
    env.devcontainer_project = true;
    assert_eq!(shell(&env, true).reason, Some(UnavailableReason::DevcontainerProject));
}

// TRS-FR-RCGL: an Agent CLI runs in a dev container, in Docker where its image
// is configured, and on the host otherwise.
#[test]
fn agent_cli_place() {
    let env = vendor_images_env();
    assert_eq!(agent_runs_in(&env, CliVendor::ClaudeCode), RunsIn::Docker);
    assert_eq!(agent_runs_in(&env, CliVendor::Codex), RunsIn::Host);
    let mut no_docker = vendor_images_env();
    no_docker.docker_verified = false;
    assert_eq!(agent_runs_in(&no_docker, CliVendor::ClaudeCode), RunsIn::Host);
    let dev = Environment { devcontainer_project: true, ..Environment::default() };
    assert_eq!(agent_runs_in(&dev, CliVendor::Opencode), RunsIn::Devcontainer);
}

// TRS-FR-RCGL, TRS-FR-XXPE: an unverified vendor is listed and unavailable.
#[test]
fn agent_cli_needs_a_verified_vendor() {
    let kinds = kinds_for(&vendor_images_env(), false);
    assert!(find(&kinds, TerminalKind::AgentCli, Some(CliVendor::ClaudeCode)).available);
    let codex = find(&kinds, TerminalKind::AgentCli, Some(CliVendor::Codex));
    assert_eq!(codex.reason, Some(UnavailableReason::VendorUnverified));
    assert_eq!(kinds.len(), 6);
}

fn worktree(path: &str, active: bool) -> WorktreeFact {
    WorktreeFact {
        path: path.into(),
        name: path.trim_start_matches('/').into(),
        branch: Some("main".into()),
        is_active: active,
        is_missing: false,
        stream: None,
    }
}

// TRS-FR-ZCPH, TRS-FR-LUUX: the active worktree first, no missing worktree,
// a stream's working copy listed once as the stream, each with its busy run.
#[test]
fn builds_the_target_list() {
    let mut missing = worktree("/gone", false);
    missing.is_missing = true;
    let mut stream = worktree("/stream", false);
    stream.stream = Some(("s-1".into(), "auth".into(), Some("run-9".into())));
    let facts = vec![worktree("/b", false), worktree("/a", true), missing, stream];
    let targets = build_targets(
        &facts,
        |path| (path == "/b").then(|| "run-direct".to_string()),
        |_| Vec::new(),
    );
    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "auth"]);
    assert!(targets[0].is_active_worktree);
    assert_eq!(targets[1].busy_run_id.as_deref(), Some("run-direct"));
    assert_eq!(targets[2].target, TargetRef::WorkStream { stream_id: "s-1".into() });
    assert_eq!(targets[2].busy_run_id.as_deref(), Some("run-9"));
}
