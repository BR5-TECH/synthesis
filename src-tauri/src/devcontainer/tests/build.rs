use std::sync::Mutex;

use super::*;

#[derive(Default)]
struct Recorded {
    progress: Mutex<Vec<DevcontainerBuildProgress>>,
}

impl BuildEvents for Recorded {
    fn progress(&self, payload: DevcontainerBuildProgress) {
        self.progress.lock().unwrap().push(payload);
    }
    fn finished(&self, _payload: DevcontainerBuildFinished) {}
}

fn spec(cli: &str, no_cache: bool) -> BuildSpec {
    BuildSpec {
        cli: cli.to_string(),
        docker: "/usr/bin/docker".to_string(),
        workspace: PathBuf::from("/p"),
        config: PathBuf::from("/p/.devcontainer/devcontainer.json"),
        no_cache,
        image_name: "synthesis-devcontainer-x:latest".to_string(),
    }
}

// DVC-FR-OXCZ / DVC-FR-GNOK: the arguments name the workspace, the config, the
// Docker CLI and the image, `--no-cache` only where asked, and never a push.
#[test]
fn build_arguments_are_exact() {
    let args = build_args(&spec("devcontainer", false));
    assert_eq!(
        args,
        vec![
            "build",
            "--workspace-folder",
            "/p",
            "--config",
            "/p/.devcontainer/devcontainer.json",
            "--docker-path",
            "/usr/bin/docker",
            "--image-name",
            "synthesis-devcontainer-x:latest",
            "--log-format",
            "text",
        ]
    );
    let args = build_args(&spec("devcontainer", true));
    assert_eq!(args.last().map(String::as_str), Some("--no-cache"));
    assert!(!args.iter().any(|a| a.contains("push")));
}

// DVC-FR-OXCZ: the image name is stable for a root and differs between roots.
#[test]
fn the_image_name_belongs_to_the_project() {
    let a = image_name_for(Path::new("/a"));
    assert_eq!(a, image_name_for(Path::new("/a")));
    assert_ne!(a, image_name_for(Path::new("/b")));
    assert!(a.starts_with("synthesis-devcontainer-") && a.ends_with(":latest"), "{a}");
}

// DVC-FR-OXCZ: the image comes from the last JSON line of a success.
#[test]
fn the_built_image_is_read_from_the_result() {
    let fallback = "fallback:1";
    assert_eq!(
        built_image("log\n{\"outcome\":\"success\",\"imageName\":[\"a:1\",\"b:1\"]}\n", fallback),
        Some("a:1".to_string())
    );
    assert_eq!(
        built_image("{\"outcome\":\"success\",\"imageName\":\"c:1\"}", fallback),
        Some("c:1".to_string())
    );
    assert_eq!(built_image("{\"outcome\":\"success\"}", fallback), Some(fallback.to_string()));
    assert_eq!(built_image("{\"outcome\":\"error\",\"message\":\"m\"}", fallback), None);
    assert_eq!(built_image("no json at all", fallback), None);
}

// DVC-FR-EKAO: the probe has no network and runs `command -v` in a login shell.
#[test]
fn probe_arguments_are_exact() {
    assert_eq!(
        probe_args("img:1", "claude"),
        vec![
            "run",
            "--rm",
            "--network",
            "none",
            "--entrypoint",
            "sh",
            "img:1",
            "-lc",
            "command -v claude"
        ]
    );
}

// DVC-FR-OXCZ / DVC-FR-LFQQ: one build for each project, the lines it kept,
// and the slot free again after the release.
#[test]
fn the_registry_holds_one_build_for_each_project() {
    let registry = DevcontainerBuildRegistry::default();
    let a = Path::new("/a");
    assert_eq!(registry.in_flight(a), None);
    let token = registry.claim(a, "op-1").unwrap();
    assert_eq!(registry.claim(a, "op-2").unwrap_err(), ERR_BUILD_IN_FLIGHT);
    assert!(registry.claim(Path::new("/b"), "op-3").is_ok());
    for index in 0..(KEPT_LINES + 5) {
        registry.push_line(a, &format!("line {index}"));
    }
    let running = registry.in_flight(a).unwrap();
    assert_eq!(running.operation_id, "op-1");
    assert_eq!(running.lines.len(), KEPT_LINES);
    assert_eq!(running.lines.last().map(String::as_str), Some("line 504"));
    registry.cancel("unknown");
    assert!(!token.is_cancelled());
    registry.cancel("op-1");
    assert!(token.is_cancelled());
    registry.release(a);
    assert_eq!(registry.in_flight(a), None);
    assert!(registry.claim(a, "op-4").is_ok());
}

#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().to_string()
}

// DVC-FR-RTBA: each line is emitted and kept, and a success names its image.
#[cfg(unix)]
#[tokio::test]
async fn a_build_streams_its_lines_and_names_the_image() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("args");
    let cli = script(
        dir.path(),
        "devcontainer",
        &format!(
            "printf '%s\\n' \"$@\" > {}; echo step one; echo step two >&2; echo '{{\"outcome\":\"success\",\"imageName\":[\"img:1\"]}}'",
            log.display()
        ),
    );
    let registry = DevcontainerBuildRegistry::default();
    let root = Path::new("/project");
    let cancel = registry.claim(root, "op").unwrap();
    let events = Recorded::default();
    let finished = run_build(&spec(&cli, true), root, "op", &cancel, &events, &registry).await;
    assert_eq!(finished.outcome, BuildOutcome::Succeeded);
    assert_eq!(finished.image.as_deref(), Some("img:1"));
    let lines: Vec<String> =
        events.progress.lock().unwrap().iter().map(|p| p.message.clone()).collect();
    assert!(lines.contains(&"step one".to_string()) && lines.contains(&"step two".to_string()));
    assert!(events.progress.lock().unwrap().iter().all(|p| p.operation_id == "op"));
    assert!(registry.in_flight(root).unwrap().lines.contains(&"step one".to_string()));
    let args = std::fs::read_to_string(log).unwrap();
    assert!(args.lines().any(|l| l == "--no-cache"), "{args}");
}

// DVC-FR-RTBA: a failure carries the last error line, cut to its limit.
#[cfg(unix)]
#[tokio::test]
async fn a_failed_build_names_the_last_error_line() {
    let dir = tempfile::tempdir().unwrap();
    let long = "é".repeat(400);
    let cli =
        script(dir.path(), "devcontainer", &format!("echo first >&2; echo '{long}' >&2; exit 1"));
    let registry = DevcontainerBuildRegistry::default();
    let finished = run_build(
        &spec(&cli, false),
        Path::new("/p"),
        "op",
        &CancellationToken::new(),
        &Recorded::default(),
        &registry,
    )
    .await;
    assert_eq!(finished.outcome, BuildOutcome::Failed);
    let error = finished.error.unwrap();
    assert_eq!(error.chars().count(), ERROR_LINE_LIMIT);
    assert!(error.starts_with('é'));

    let ok_exit_error_result =
        script(dir.path(), "error-result", "echo '{\"outcome\":\"error\",\"message\":\"m\"}'");
    let finished = run_build(
        &spec(&ok_exit_error_result, false),
        Path::new("/p"),
        "op",
        &CancellationToken::new(),
        &Recorded::default(),
        &registry,
    )
    .await;
    assert_eq!(finished.outcome, BuildOutcome::Failed);
}

// DVC-FR-RTBA: a cancelled build ends as cancelled, with no image and no error.
#[cfg(unix)]
#[tokio::test]
async fn a_cancelled_build_is_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let cli = script(dir.path(), "devcontainer", "exec sleep 30");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let finished = run_build(
        &spec(&cli, false),
        Path::new("/p"),
        "op",
        &cancel,
        &Recorded::default(),
        &DevcontainerBuildRegistry::default(),
    )
    .await;
    assert_eq!(finished.outcome, BuildOutcome::Cancelled);
    assert_eq!((finished.image, finished.error), (None, None));
}

// DVC-FR-EKAO: a path is found, an empty answer or exit 1 is missing, and a
// probe that Docker could not run stores nothing.
#[cfg(unix)]
#[tokio::test]
async fn a_probe_reads_found_missing_or_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let found = script(dir.path(), "found", "echo /usr/local/bin/claude");
    let empty = script(dir.path(), "empty", "exit 0");
    let missing = script(dir.path(), "missing", "exit 1");
    let broken = script(dir.path(), "broken", "exit 125");
    assert_eq!(probe_image(&found, "img", "claude").await, Some(ProbeFinding::Found));
    assert_eq!(probe_image(&empty, "img", "claude").await, Some(ProbeFinding::Missing));
    assert_eq!(probe_image(&missing, "img", "claude").await, Some(ProbeFinding::Missing));
    assert_eq!(probe_image(&broken, "img", "claude").await, None);
    let absent = dir.path().join("absent").to_string_lossy().to_string();
    assert_eq!(probe_image(&absent, "img", "claude").await, None);
}
