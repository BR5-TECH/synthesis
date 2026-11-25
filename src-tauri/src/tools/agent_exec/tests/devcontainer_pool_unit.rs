use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Up(String),
    Remove(String, bool),
    EndTurn(String, String),
}

/// Records every call in order and answers `up` from a queue.
#[derive(Default)]
struct FakeRuntime {
    calls: StdMutex<Vec<Call>>,
    answers: StdMutex<VecDeque<Result<RunningDevcontainer, String>>>,
    started: std::sync::atomic::AtomicUsize,
    /// How many removals fail before one succeeds.
    failing_removals: std::sync::atomic::AtomicUsize,
}

impl FakeRuntime {
    fn answering(answers: Vec<Result<RunningDevcontainer, String>>) -> Arc<Self> {
        Arc::new(FakeRuntime { answers: StdMutex::new(answers.into()), ..Default::default() })
    }
    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

fn running(id: &str) -> RunningDevcontainer {
    RunningDevcontainer {
        container_id: id.to_string(),
        remote_user: "vscode".to_string(),
        remote_workspace_folder: "/w".to_string(),
        probed_env: vec![],
    }
}

impl DevcontainerRuntime for FakeRuntime {
    fn up<'a>(
        &'a self,
        start: &'a DevcontainerStart,
        _cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<RunningDevcontainer, String>> {
        self.calls.lock().unwrap().push(Call::Up(start.scope_key.clone()));
        let n = self.started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let answer = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(running(&format!("c{n}"))));
        Box::pin(async move { answer })
    }
    fn end_turn<'a>(
        &'a self,
        _: &'a str,
        container: &'a str,
        turn: &'a str,
    ) -> BoxFuture<'a, bool> {
        self.calls.lock().unwrap().push(Call::EndTurn(container.to_string(), turn.to_string()));
        Box::pin(async { true })
    }
    fn remove<'a>(
        &'a self,
        _: &'a str,
        scope_key: &'a str,
        compose: bool,
    ) -> BoxFuture<'a, Result<(), String>> {
        self.calls.lock().unwrap().push(Call::Remove(scope_key.to_string(), compose));
        let fails = self
            .failing_removals
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |n| n.checked_sub(1),
            )
            .is_ok();
        Box::pin(async move {
            if fails {
                Err("docker rm exited with code 1".into())
            } else {
                Ok(())
            }
        })
    }
}

fn start(scope: &str, project: &str) -> DevcontainerStart {
    DevcontainerStart {
        scope_key: scope.to_string(),
        project_key: project.to_string(),
        cli: "/bin/devcontainer".to_string(),
        docker: "/bin/docker".to_string(),
        worktree: "/work/tree".to_string(),
        config: PathBuf::from("/p/.devcontainer/devcontainer.json"),
        config_hash: "h1".to_string(),
        override_path: PathBuf::from("/data/devcontainers/x/devcontainer.json"),
        override_text: "{}".to_string(),
        compose: false,
    }
}

fn up(scope: &str) -> Call {
    Call::Up(scope.to_string())
}
fn remove(scope: &str) -> Call {
    Call::Remove(scope.to_string(), false)
}

async fn turn_in(
    pool: &DevcontainerPool,
    start: &DevcontainerStart,
) -> Result<String, StartFailed> {
    pool.with_container(start, &CancellationToken::new(), |r| async move { r.container_id }).await
}

// DVC-FR-QGUT / DVC-FR-GNOK: the arguments of `devcontainer up` and of the
// environment probe.
#[test]
fn up_and_probe_arguments_are_exact() {
    let s = start("S", "P");
    assert_eq!(
        s.up_args(),
        vec![
            "up",
            "--workspace-folder",
            "/work/tree",
            "--config",
            "/p/.devcontainer/devcontainer.json",
            "--override-config",
            "/data/devcontainers/x/devcontainer.json",
            "--id-label",
            "synthesis.scope=S",
            "--id-label",
            "synthesis.project=P",
            "--docker-path",
            "/bin/docker",
            "--mount-workspace-git-root=false",
            "--log-format",
            "text",
        ]
    );
    let probe = s.env_probe_args();
    assert_eq!(probe[0], "exec");
    assert_eq!(&probe[1..13], &s.config_args()[..]);
    assert_eq!(&probe[13..], &["sh", "-c", "env -0 2>/dev/null || env"]);
}

// DVC-FR-BUXC: the result of `devcontainer up` is read, also after log lines;
// a failure gives its message, and a result with no container id fails.
#[test]
fn the_up_result_is_read() {
    let ok = "log line\n{\"outcome\":\"success\",\"containerId\":\"abc\",\"remoteUser\":\"vscode\",\"remoteWorkspaceFolder\":\"/w\"}\n";
    assert_eq!(parse_up_result(ok), Ok(("abc".into(), "vscode".into(), "/w".into())));
    assert_eq!(parse_up_result("{\"outcome\":\"error\",\"message\":\"m\"}"), Err("m".into()));
    assert_eq!(parse_up_result("{\"outcome\":\"error\",\"description\":\"d\"}"), Err("d".into()));
    assert_eq!(parse_up_result("{\"outcome\":\"error\"}"), Err(String::new()));
    assert!(parse_up_result("{\"outcome\":\"success\"}").is_err());
    assert!(parse_up_result("no json").is_err());
}

// DVC-FR-BUXC: the probed environment keeps values whole, drops the probe's
// own shell variables and names that are not names.
#[test]
fn the_probed_environment_is_parsed() {
    let nul = "PATH=/a:/b\0EQ=a=b\0EMPTY=\0_=/bin/env\0PWD=/w\0SHLVL=1\0MULTI=one\ntwo\0BASH_FUNC_x%%=() {}\0";
    assert_eq!(
        parse_probed_env(nul),
        vec![
            ("PATH".to_string(), "/a:/b".to_string()),
            ("EQ".to_string(), "a=b".to_string()),
            ("EMPTY".to_string(), String::new()),
            ("MULTI".to_string(), "one\ntwo".to_string()),
        ]
    );
    let lines = "PATH=/x\nHOSTNAME=h\nHOME=/home/u\n";
    assert_eq!(
        parse_probed_env(lines),
        vec![("PATH".to_string(), "/x".to_string()), ("HOME".to_string(), "/home/u".to_string())]
    );
}

// DVC-FR-SFWW: the end-of-turn script finds the turn by its marker.
#[test]
fn the_end_turn_script_finds_the_marker() {
    assert!(end_turn_script().contains("SYNTHESIS_TURN=$1"));
    // DVC-FR-TQHM: a process of the turn that is still left after the passes
    // is reported by a failed exit.
    assert!(end_turn_script().contains("exit 0"));
    assert!(end_turn_script().starts_with("command -v tr"), "no tools, no confirmation");
    assert!(end_turn_script().ends_with("exit 1"));
}

// DVC-FR-OJEH / DVC-FR-LKRY: the first launch removes what the labels name and
// starts the container; a later equal launch reuses it.
#[tokio::test]
async fn a_scope_starts_once_and_is_reused() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    let s = start("S", "P");
    assert_eq!(turn_in(&pool, &s).await.unwrap(), "c0");
    assert_eq!(turn_in(&pool, &s).await.unwrap(), "c0");
    assert_eq!(runtime.calls(), vec![remove("S"), up("S")]);
    assert!(pool.holds("S"));
}

// DVC-FR-DELL: a changed spec replaces the container.
#[tokio::test]
async fn a_changed_spec_replaces_the_container() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    let s = start("S", "P");
    turn_in(&pool, &s).await.unwrap();
    for changed in [
        DevcontainerStart { config_hash: "h2".into(), ..s.clone() },
        DevcontainerStart { worktree: "/other".into(), ..s.clone() },
        DevcontainerStart { override_text: "{\"x\":1}".into(), ..s.clone() },
    ] {
        let before = runtime.calls().len();
        turn_in(&pool, &changed).await.unwrap();
        assert_eq!(&runtime.calls()[before..], &[remove("S"), up("S")]);
    }
}

// DVC-FR-LKRY: two scopes have two containers, and a stop names its own.
#[tokio::test]
async fn two_scopes_never_share_a_container() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    let a = turn_in(&pool, &start("A", "P")).await.unwrap();
    let b = turn_in(&pool, &start("B", "P")).await.unwrap();
    assert_ne!(a, b);
    pool.stop("A").await.unwrap();
    assert_eq!(runtime.calls().last(), Some(&remove("A")));
    assert!(pool.holds("B"));
}

// DVC-FR-ATEO / DVC-FR-OJEH: a failed start runs no turn, and the next launch
// removes what the failed start can have left before it starts again.
#[tokio::test]
async fn a_failed_start_is_removed_before_the_next() {
    let runtime = FakeRuntime::answering(vec![Err("boom".into())]);
    let pool = DevcontainerPool::new(runtime.clone());
    let s = start("S", "P");
    let mut ran = false;
    let failed = pool
        .with_container(&s, &CancellationToken::new(), |_| {
            ran = true;
            async {}
        })
        .await;
    assert_eq!(failed, Err(StartFailed("boom".into())));
    assert!(!ran);
    turn_in(&pool, &DevcontainerStart { config_hash: "fixed".into(), ..s }).await.unwrap();
    assert_eq!(runtime.calls(), vec![remove("S"), up("S"), remove("S"), up("S")]);
}

// DVC-FR-DTWX: a failed start is still removed by a stop.
#[tokio::test]
async fn a_failed_start_is_removed_by_a_stop() {
    let runtime = FakeRuntime::answering(vec![Err("boom".into())]);
    let pool = DevcontainerPool::new(runtime.clone());
    let _ = turn_in(&pool, &start("S", "P")).await;
    pool.stop("S").await.unwrap();
    assert_eq!(runtime.calls().last(), Some(&remove("S")));
    assert!(!pool.holds("S"));
}

// DVC-FR-OHMC: a launch with no scope starts, runs, and is removed, also when
// its start fails.
#[tokio::test]
async fn a_launch_with_no_scope_is_removed_after() {
    let runtime = FakeRuntime::answering(vec![Ok(running("once")), Err("boom".into())]);
    let pool = DevcontainerPool::new(runtime.clone());
    let s = start("P-once-1", "P");
    let id = pool
        .with_container_once(&s, &CancellationToken::new(), |r| async move { r.container_id })
        .await
        .unwrap();
    assert_eq!(id, "once");
    assert_eq!(runtime.calls(), vec![up("P-once-1"), remove("P-once-1")]);
    assert!(!pool.holds("P-once-1"));
    let failed = pool.with_container_once(&s, &CancellationToken::new(), |_| async {}).await;
    assert!(failed.is_err());
    assert_eq!(runtime.calls().last(), Some(&remove("P-once-1")));
}

// DVC-FR-DTWX: after a stop the next launch starts a new container; a stop of
// an unknown scope does nothing.
#[tokio::test]
async fn a_stop_removes_and_the_next_launch_starts_again() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    let s = start("S", "P");
    turn_in(&pool, &s).await.unwrap();
    pool.stop("unknown").await.unwrap();
    pool.stop("S").await.unwrap();
    assert!(!pool.holds("S"));
    assert_eq!(turn_in(&pool, &s).await.unwrap(), "c1");
    assert_eq!(runtime.calls(), vec![remove("S"), up("S"), remove("S"), remove("S"), up("S")]);
}

// DVC-FR-DTWX / EAC-FR-LQRI: a stop waits for a turn that holds the scope.
#[tokio::test]
async fn a_stop_waits_for_the_running_turn() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = Arc::new(DevcontainerPool::new(runtime.clone()));
    let s = start("S", "P");
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let (entered, in_turn) = tokio::sync::oneshot::channel::<()>();
    let running_turn = {
        let pool = pool.clone();
        let s = s.clone();
        tokio::spawn(async move {
            pool.with_container(&s, &CancellationToken::new(), |_| async move {
                let _ = entered.send(());
                let _ = released.await;
            })
            .await
        })
    };
    in_turn.await.unwrap();
    let stop = {
        let pool = pool.clone();
        tokio::spawn(async move { pool.stop("S").await.unwrap() })
    };
    tokio::task::yield_now().await;
    assert_eq!(runtime.calls(), vec![remove("S"), up("S")], "the stop waits");
    release.send(()).unwrap();
    running_turn.await.unwrap().unwrap();
    stop.await.unwrap();
    assert_eq!(runtime.calls().last(), Some(&remove("S")));
}

// DVC-FR-DTWX / EAC-FR-LQRI: a queue with work again keeps its container.
#[tokio::test]
async fn a_queue_with_work_keeps_its_container() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    let s = start("S", "P");
    turn_in(&pool, &s).await.unwrap();
    assert!(!pool.stop_when("S", || false).await.unwrap());
    assert_eq!(turn_in(&pool, &s).await.unwrap(), "c0");
    assert!(pool.stop_when("S", || true).await.unwrap());
    assert_eq!(runtime.calls(), vec![remove("S"), up("S"), remove("S")]);
}

// DVC-FR-DTWX: a project close removes only that project's containers.
#[tokio::test]
async fn a_project_close_removes_only_its_containers() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    turn_in(&pool, &start("A", "P1")).await.unwrap();
    turn_in(&pool, &start("B", "P2")).await.unwrap();
    assert!(pool.stop_project("P1").await.is_empty());
    assert!(!pool.holds("A"));
    assert!(pool.holds("B"));
    assert_eq!(runtime.calls().last(), Some(&remove("A")));
}

// DVC-FR-DTWX: an exit removes every container, also one whose turn runs, and
// does not wait for that turn.
#[tokio::test]
async fn an_exit_removes_every_container() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = Arc::new(DevcontainerPool::new(runtime.clone()));
    turn_in(&pool, &start("idle", "P")).await.unwrap();
    let (entered, in_turn) = tokio::sync::oneshot::channel::<()>();
    let (_hold, held) = tokio::sync::oneshot::channel::<()>();
    let busy = {
        let pool = pool.clone();
        tokio::spawn(async move {
            pool.with_container(&start("busy", "P"), &CancellationToken::new(), |_| async move {
                let _ = entered.send(());
                let _ = held.await;
            })
            .await
        })
    };
    in_turn.await.unwrap();
    assert!(pool.stop_all().await.is_empty());
    let mut removed: Vec<Call> =
        runtime.calls().into_iter().filter(|c| matches!(c, Call::Remove(..))).skip(2).collect();
    removed.sort_by_key(|c| format!("{c:?}"));
    assert_eq!(removed, vec![remove("busy"), remove("idle")]);
    busy.abort();
}

// DVC-FR-JXEA: a compose scope is removed as compose.
#[tokio::test]
async fn a_compose_scope_is_removed_as_compose() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    turn_in(&pool, &DevcontainerStart { compose: true, ..start("S", "P") }).await.unwrap();
    pool.stop("S").await.unwrap();
    assert_eq!(runtime.calls().last(), Some(&Call::Remove("S".into(), true)));
}

#[cfg(unix)]
fn script(dir: &std::path::Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().to_string()
}

// DVC-FR-BUXC: the CLI runtime starts the container and probes the
// environment of its remote user.
#[cfg(unix)]
#[tokio::test]
async fn the_cli_runtime_reads_up_and_the_probed_environment() {
    let dir = tempfile::tempdir().unwrap();
    let cli = script(
        dir.path(),
        "devcontainer",
        "case \"$1\" in up) echo log >&2; echo '{\"outcome\":\"success\",\"containerId\":\"cid\",\"remoteUser\":\"vscode\",\"remoteWorkspaceFolder\":\"/w\"}';; exec) printf 'PATH=/a\\0SHLVL=2\\0HOME=/home/vscode\\0';; esac",
    );
    let s = DevcontainerStart { cli, ..start("S", "P") };
    let running = CliDevcontainerRuntime.up(&s, &CancellationToken::new()).await.unwrap();
    assert_eq!(running.container_id, "cid");
    assert_eq!(running.remote_user, "vscode");
    assert_eq!(running.remote_workspace_folder, "/w");
    assert_eq!(
        running.probed_env,
        vec![("PATH".into(), "/a".into()), ("HOME".into(), "/home/vscode".into())]
    );
}

// DVC-FR-ATEO: a failed start gives the CLI's message, or its last line.
#[cfg(unix)]
#[tokio::test]
async fn the_cli_runtime_reports_a_failed_start() {
    let dir = tempfile::tempdir().unwrap();
    let message =
        script(dir.path(), "message", "echo '{\"outcome\":\"error\",\"message\":\"m\"}'; exit 1");
    let silent = script(dir.path(), "silent", "echo first >&2; echo last >&2; exit 1");
    let probe_fails = script(
        dir.path(),
        "probe",
        "case \"$1\" in up) echo '{\"outcome\":\"success\",\"containerId\":\"c\"}';; exec) echo nope >&2; exit 1;; esac",
    );
    let run = |cli: String| {
        let s = DevcontainerStart { cli, ..start("S", "P") };
        async move { CliDevcontainerRuntime.up(&s, &CancellationToken::new()).await }
    };
    assert_eq!(run(message).await.unwrap_err(), "m");
    assert_eq!(run(silent).await.unwrap_err(), "last");
    assert_eq!(run(probe_fails).await.unwrap_err(), "nope");
}

// DVC-FR-JXEA: removal lists the scope's containers, removes them, and for a
// compose config takes the compose projects down.
#[cfg(unix)]
#[tokio::test]
async fn the_cli_runtime_removes_by_label() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let docker = script(
        dir.path(),
        "docker",
        &format!(
            "echo \"$*\" >> {}; case \"$1\" in ps) echo a; echo b;; inspect) echo proj; echo proj;; esac",
            log.display()
        ),
    );
    CliDevcontainerRuntime.remove(&docker, "S", true).await.unwrap();
    let calls: Vec<String> =
        std::fs::read_to_string(&log).unwrap().lines().map(str::to_string).collect();
    assert_eq!(calls[0], "ps -aq --filter label=synthesis.scope=S");
    assert!(calls[1].starts_with("inspect --format"));
    assert_eq!(calls[2], "rm -f a b");
    assert_eq!(calls[3], "compose -p proj down");
    assert_eq!(calls.len(), 4);

    let quiet = script(dir.path(), "quiet", &format!("echo \"$*\" >> {}.2", log.display()));
    CliDevcontainerRuntime.remove(&quiet, "S", false).await.unwrap();
    let calls = std::fs::read_to_string(format!("{}.2", log.display())).unwrap();
    assert_eq!(calls.lines().count(), 1, "no container, no rm: {calls}");
}

// DVC-FR-DTWX: a removal that fails keeps the scope tracked, and a later stop
// tries it again.
#[tokio::test]
async fn a_failed_removal_is_tried_again() {
    let runtime = FakeRuntime::answering(vec![]);
    let pool = DevcontainerPool::new(runtime.clone());
    turn_in(&pool, &start("S", "P")).await.unwrap();
    runtime.failing_removals.store(1, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(pool.stop("S").await, Err("docker rm exited with code 1".into()));
    assert!(pool.holds("S"));
    let failures = pool.stop_all().await;
    assert!(failures.is_empty(), "{failures:?}");
    assert!(!pool.holds("S"));
    runtime.failing_removals.store(1, std::sync::atomic::Ordering::SeqCst);
    turn_in(&pool, &start("T", "P")).await.unwrap();
    runtime.failing_removals.store(1, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        pool.stop_project("P").await,
        vec![("T".to_string(), "docker rm exited with code 1".to_string())]
    );
    assert!(pool.holds("T"));
}

// DVC-FR-JXEA: a `docker ps` that fails names its step.
#[cfg(unix)]
#[tokio::test]
async fn a_failed_listing_names_its_step() {
    let dir = tempfile::tempdir().unwrap();
    let docker = script(dir.path(), "docker", "exit 3");
    assert_eq!(
        CliDevcontainerRuntime.remove(&docker, "S", false).await,
        Err("docker ps exited with code 3".to_string())
    );
}

// DVC-FR-SFWW, DVC-FR-TQHM: the script ends every process that carries the
// turn's marker, a child of one included, leaves the processes of another
// turn, and exits 0 once a scan finds none of the turn left.
#[cfg(target_os = "linux")]
#[test]
fn the_end_turn_script_ends_the_turns_processes_and_no_other() {
    use std::process::Command;
    let spawn = |turn: &str, script: &str| {
        Command::new("sh").args(["-c", script]).env("SYNTHESIS_TURN", turn).spawn().unwrap()
    };
    let mut own = spawn("t1", "sleep 300");
    let mut parent = spawn("t1", "sh -c 'sleep 300'");
    let mut other = spawn("t2", "sleep 300");
    let status = Command::new("sh").args(["-c", end_turn_script(), "sh", "t1"]).status().unwrap();
    assert!(status.success(), "no process of the turn is left: {status:?}");
    assert!(own.wait().is_ok() && parent.wait().is_ok(), "the turn's processes ended");
    assert!(other.try_wait().unwrap().is_none(), "the other turn's process still runs");
    let _ = other.kill();
    let _ = other.wait();
}
