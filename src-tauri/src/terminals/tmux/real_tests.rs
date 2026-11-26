//! The module against a real tmux, on a socket of each test's own. A machine
//! with no usable tmux fails these tests: CI installs tmux.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Once, OnceLock};
use std::time::Duration;

use super::locate::{find_tmux, version_usable};
use super::meta::{parse_list, session_name, Meta, PaneState};
use super::runner::{run_bounded, RealClients, RealTmux, CALL_TIMEOUT};
use super::supervisor::{create, supervise, Persistence, Warn};
use super::{commands, Tmux, TmuxRunner};
use crate::terminals::model::*;
use crate::terminals::process::{terminal_env, HostCommand, ProcessHandle};

const KEY: &str = "/real/project";

/// The start of the socket name of every server of these tests.
const SOCKET_PREFIX: &str = "synthesis-test-";

/// A tmux server on a socket of its own, which ends with the test, also when
/// the test fails. tmux leaves the socket file of a server that a
/// `kill-server` ends, so the drop removes it. A test that nextest stops at
/// its time limit runs no `Drop`: the next test process reaps what it left
/// (`reap_in`).
struct Server {
    tmux: Arc<Tmux>,
    runner: Arc<RealTmux>,
    dir: Option<PathBuf>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.runner.run(&["kill-server".to_string()]);
        if let Some(dir) = &self.dir {
            remove_socket(dir, &self.runner.socket);
        }
    }
}

fn server() -> Server {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let program = find_tmux().expect("tmux 3.2 or later must be installed for these tests");
    let mut version = std::process::Command::new(&program);
    version.arg("-V");
    let text = run_bounded(version, CALL_TIMEOUT).expect("tmux -V");
    assert!(version_usable(&text), "tmux 3.2 or later must be installed, found {text}");
    static SOCKET_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    let dir = SOCKET_DIR.get_or_init(|| socket_dir(&program)).clone();
    static REAPED: Once = Once::new();
    REAPED.call_once(|| {
        if let Some(dir) = &dir {
            reap_in(&program, dir, SOCKET_PREFIX);
        }
    });
    let socket =
        format!("{SOCKET_PREFIX}{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
    let runner = Arc::new(RealTmux { program: program.clone(), socket: socket.clone() });
    let tmux = Arc::new(Tmux {
        runner: runner.clone(),
        clients: Arc::new(RealClients { program, socket }),
    });
    Server { tmux, runner, dir }
}

/// The directory of the tmux sockets, as tmux names it. A server of its own
/// answers, so the path is the real one also where `/tmp` is a link.
fn socket_dir(program: &str) -> Option<PathBuf> {
    let probe = RealTmux {
        program: program.to_string(),
        socket: format!("{SOCKET_PREFIX}{}-probe", std::process::id()),
    };
    let args = commands::join(vec![
        vec!["new-session".into(), "-d".into(), "-s".into(), "probe".into()],
        vec![
            "display-message".into(),
            "-p".into(),
            "-t".into(),
            "=probe:".into(),
            "#{socket_path}".into(),
        ],
    ]);
    let path = probe.run(&args);
    let _ = probe.run(&["kill-server".to_string()]);
    let dir = PathBuf::from(path.ok()?.trim()).parent().map(PathBuf::from)?;
    remove_socket(&dir, &probe.socket);
    Some(dir)
}

/// Removes the socket file `name` in `dir`, where it is there.
fn remove_socket(dir: &std::path::Path, name: &str) {
    if let Ok(access) = crate::fs::FsAccess::builder().allow_root(dir).build() {
        let _ = access.delete_path(dir.join(name), false);
    }
}

/// The process id in a socket name of these tests, `synthesis-test-<pid>-<n>`.
fn test_socket_pid(name: &str, prefix: &str) -> Option<i32> {
    let (pid, _) = name.strip_prefix(prefix)?.split_once('-')?;
    pid.parse().ok()
}

fn process_alive(pid: i32) -> bool {
    // SAFETY: signal 0 sends nothing; `kill` only checks that the process exists.
    let found = unsafe { libc::kill(pid, 0) } == 0;
    found || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Ends each server of these tests in `dir` whose test process is gone, and
/// removes its socket. Returns the socket names it removed.
fn reap_in(program: &str, dir: &std::path::Path, prefix: &str) -> Vec<String> {
    let Ok(access) = crate::fs::FsAccess::builder().allow_root(dir).build() else {
        return Vec::new();
    };
    let Ok(entries) = access.list_dir(dir) else { return Vec::new() };
    let mut reaped = Vec::new();
    for entry in entries {
        let Some(pid) = test_socket_pid(&entry.name, prefix) else { continue };
        if process_alive(pid) {
            continue;
        }
        let stale = RealTmux { program: program.to_string(), socket: entry.name.clone() };
        let _ = stale.run(&["kill-server".to_string()]);
        let _ = access.delete_path(dir.join(&entry.name), false);
        reaped.push(entry.name);
    }
    reaped
}

/// A process id that no process has: the id of a child that ended.
fn ended_pid() -> i32 {
    let mut child = std::process::Command::new("/usr/bin/true").spawn().expect("true");
    let pid = child.id() as i32;
    child.wait().expect("true ends");
    pid
}

fn meta() -> Meta {
    Meta {
        project: KEY.into(),
        label: "Host shell 1".into(),
        target: TargetRef::Worktree { path: "/tmp".into() },
        kind: TerminalKind::HostShell,
        vendor: None,
        runs_in: RunsIn::Host,
        pinned: true,
        theme: None,
        created_at: "2026-11-22T10:00:00Z".into(),
    }
}

fn sh(script: &str) -> HostCommand {
    HostCommand {
        program: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        env: terminal_env(),
        env_remove: vec![],
    }
}

fn quiet() -> Warn {
    Arc::new(|_| {})
}

/// Reads output until it holds `marker`.
async fn read_until(handle: &mut ProcessHandle, marker: &str) -> String {
    let mut seen = Vec::new();
    while let Some(chunk) = handle.output.recv().await {
        seen.extend(chunk);
        if String::from_utf8_lossy(&seen).contains(marker) {
            return String::from_utf8_lossy(&seen).into_owned();
        }
    }
    panic!("the output ended before {marker}: {}", String::from_utf8_lossy(&seen));
}

/// Waits for the tmux session to reach a state; tmux answers in its own time.
fn wait_for(what: &str, done: impl Fn() -> bool) {
    for _ in 0..2_000 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("never reached: {what}");
}

fn listed(server: &Server) -> Vec<super::meta::Listed> {
    // exit-empty is off, so the server answers also with no session.
    parse_list(&server.runner.run(&commands::list()).expect("the server answers"), KEY)
}

// TRS-FR-BHCB, TRS-FR-KMWO, TRS-FR-QMZM, TRS-FR-OTLH, TRS-FR-IEKU,
// TRS-FR-TCBS, TRS-FR-CXSV: a session runs in tmux with the terminal
// variables, keeps running when its client ends, lists with its metadata,
// gives its lines above the screen, and goes on a kill.
#[tokio::test]
async fn a_session_outlives_its_client() {
    let server = server();
    let name = session_name(KEY, "t1");
    let script = "seq 1 60; echo \"vars:$TERM:$COLORTERM\"; echo ready; exec sleep 600";
    let persistence =
        create(server.tmux.clone(), name.clone(), &sh(script), &meta(), &|_| {}).unwrap();
    let mut handle = supervise(persistence.clone(), 80, 24, quiet()).unwrap();
    let screen = read_until(&mut handle, "ready").await;
    assert!(screen.contains("vars:xterm-256color:truecolor"), "{screen}");
    // TRS-FR-ANJK: the client uses no alternate screen.
    assert!(!screen.contains("\x1b[?1049h"), "{screen}");

    handle.control.terminate();
    while handle.output.recv().await.is_some() {}
    assert_eq!(handle.exit.await.unwrap(), None);

    let sessions = listed(&server);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].terminal_id, "t1");
    assert_eq!(sessions[0].meta, meta());
    assert_eq!(sessions[0].pane, PaneState::Alive);

    let above = persistence.capture(true).unwrap();
    assert!(above.lines().any(|l| l == "1"), "{above}");
    assert!(!above.contains("ready"), "{above}");
    let whole = persistence.capture(false).unwrap();
    assert!(whole.contains("ready"));

    persistence.kill().unwrap();
    assert!(listed(&server).is_empty());
}

// TRS-FR-KBOS: a process that ends frees its client, and the handle reports
// its exit status; the tmux session goes.
#[tokio::test]
async fn a_process_end_reports_its_status() {
    let server = server();
    let name = session_name(KEY, "t2");
    let persistence =
        create(server.tmux.clone(), name, &sh("echo bye; read line; exit 3"), &meta(), &|_| {})
            .unwrap();
    let mut handle = supervise(persistence.clone(), 80, 24, quiet()).unwrap();
    // The client is attached while the process runs, so the pane-died hook
    // frees it.
    read_until(&mut handle, "bye").await;
    handle.control.write(b"go\r").unwrap();
    while handle.output.recv().await.is_some() {}
    assert_eq!(handle.exit.await.unwrap(), Some(3));
    assert!(persistence.is_gone());
    assert!(listed(&server).is_empty());
}

// TRS-FR-WINB, TRS-FR-ANJK: with mouse off, the client gets the mouse mode of
// the program, and a wheel report of the client reaches the program unchanged.
#[tokio::test]
async fn a_wheel_report_reaches_a_mouse_aware_program() {
    let server = server();
    let script = "stty raw -echo; printf '\\033[?1000h\\033[?1006h'; echo ready; \
                  v=$(dd bs=1 count=10 2>/dev/null | od -An -tx1 | tr -d ' \\n'); \
                  echo \"got:$v:end\"; exec sleep 600";
    let persistence =
        create(server.tmux.clone(), session_name(KEY, "t7"), &sh(script), &meta(), &|_| {})
            .unwrap();
    let mut handle = supervise(persistence.clone(), 80, 24, quiet()).unwrap();
    let mut screen = read_until(&mut handle, "ready").await;
    if !screen.contains("\x1b[?1006h") {
        screen.push_str(&read_until(&mut handle, "\x1b[?1006h").await);
    }
    assert!(screen.contains("\x1b[?1000h"), "{screen:?}");
    handle.control.write(b"\x1b[<64;5;3M").unwrap();
    // The hex of ESC [ < 6 4 ; 5 ; 3 M.
    let reply = read_until(&mut handle, ":end").await;
    assert!(reply.contains("got:1b5b3c36343b353b334d:end"), "{reply:?}");
    handle.control.terminate();
    while handle.output.recv().await.is_some() {}
}

// TRS-FR-KBOS: a client that attaches after the process ended is freed at
// once, and the handle reports the exit status.
#[tokio::test]
async fn a_client_of_a_dead_pane_is_freed() {
    let server = server();
    let persistence =
        create(server.tmux.clone(), session_name(KEY, "t6"), &sh("exit 6"), &meta(), &|_| {})
            .unwrap();
    wait_for("the pane dies", || matches!(persistence.pane(), Ok(Some(PaneState::Dead(_)))));
    let mut handle = supervise(persistence.clone(), 80, 24, quiet()).unwrap();
    while handle.output.recv().await.is_some() {}
    assert_eq!(handle.exit.await.unwrap(), Some(6));
    assert!(listed(&server).is_empty());
}

// TRS-FR-QMZM, TRS-FR-IEKU: metadata with a trailing `;`, a tab, a percent
// sign, and text that is not ASCII comes back the same.
#[test]
fn odd_metadata_round_trips() {
    let server = server();
    let mut odd = meta();
    odd.label = "50%\tétiquette;".into();
    odd.target = TargetRef::Worktree { path: "/tmp/проект é;".into() };
    odd.kind = TerminalKind::AgentCli;
    odd.vendor = Some(CliVendor::Codex);
    create(server.tmux.clone(), session_name(KEY, "t7"), &sh("exec sleep 600"), &odd, &|_| {})
        .unwrap();
    let sessions = listed(&server);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].meta, odd);
}

// TRS-FR-TCBS: with no history, nothing is captured above the screen.
#[test]
fn no_history_captures_nothing() {
    let server = server();
    let persistence = create(
        server.tmux.clone(),
        session_name(KEY, "t8"),
        &sh("echo one; exec sleep 600"),
        &meta(),
        &|_| {},
    )
    .unwrap();
    wait_for("the line", || persistence.capture(false).is_ok_and(|t| t.contains("one")));
    assert_eq!(persistence.capture(true).unwrap(), "");
}

// TRS-FR-HJZR, TRS-FR-KBOS: a pane whose process ended with no client stays,
// and the listing reports it dead with its status.
#[test]
fn a_dead_pane_stays_for_the_next_restore() {
    let server = server();
    let name = session_name(KEY, "t3");
    let persistence =
        create(server.tmux.clone(), name, &sh("echo gone; exit 9"), &meta(), &|_| {}).unwrap();
    wait_for("the pane dies", || matches!(persistence.pane(), Ok(Some(PaneState::Dead(_)))));
    let sessions = listed(&server);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pane, PaneState::Dead(Some(9)));
    assert!(persistence.capture(false).unwrap().contains("gone"));
}

// TRS-FR-ANJK, TRS-FR-ZOZO, TRS-FR-ZEQC: the server of the socket has no status
// bar, no prefix key, mouse off, a history of 10,000 lines, and hyperlinks.
#[test]
fn the_server_options() {
    let server = server();
    create(server.tmux.clone(), session_name(KEY, "t4"), &sh("exec sleep 600"), &meta(), &|_| {})
        .unwrap();
    let show = |args: &[&str]| {
        server.runner.run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    };
    assert_eq!(show(&["show-options", "-gv", "status"]).trim(), "off");
    assert_eq!(show(&["show-options", "-gv", "prefix"]).trim(), "None");
    assert_eq!(show(&["show-options", "-gv", "prefix2"]).trim(), "None");
    assert_eq!(show(&["show-options", "-sv", "exit-empty"]).trim(), "off");
    assert_eq!(show(&["show-options", "-gv", "mouse"]).trim(), "off");
    assert_eq!(show(&["show-options", "-gv", "history-limit"]).trim(), "10000");
    // TRS-FR-ZEQC: the server accepts the hyperlinks feature for the client terminal.
    assert!(
        show(&["show-options", "-s", "terminal-features"]).contains("xterm-256color:hyperlinks")
    );
    // The tables that a key reaches with no prefix, and the prefix tables, are
    // empty; another table is reachable only through them.
    for table in ["prefix", "root", "copy-mode", "copy-mode-vi"] {
        let keys =
            server.runner.run(&["list-keys".into(), "-T".into(), table.into()]).unwrap_or_default();
        assert_eq!(keys.trim(), "", "{table}: {keys}");
    }
    // A second and a third create on the running server set the same
    // options again, and each one makes its session.
    create(server.tmux.clone(), session_name(KEY, "t5"), &sh("exec sleep 600"), &meta(), &|_| {})
        .unwrap();
    create(server.tmux.clone(), session_name(KEY, "t9"), &sh("exec sleep 600"), &meta(), &|_| {})
        .unwrap();
    assert_eq!(listed(&server).len(), 3);
    let gone = Persistence::new(server.tmux.clone(), session_name(KEY, "nope"));
    assert_eq!(gone.pane(), Ok(None));
    assert!(gone.kill().is_ok());
}

// TRS-FR-ZOZO, TRS-FR-CXSV, TRS-FR-KBOS: the commands of one session never
// reach a session whose name starts with its name, and the end of one pane
// frees only its own client.
#[tokio::test]
async fn sessions_are_named_exactly() {
    let server = server();
    let one = create(
        server.tmux.clone(),
        session_name(KEY, "t1"),
        &sh("echo a; read l; exit 1"),
        &meta(),
        &|_| {},
    )
    .unwrap();
    let ten = create(
        server.tmux.clone(),
        session_name(KEY, "t10"),
        &sh("exec sleep 600"),
        &meta(),
        &|_| {},
    )
    .unwrap();
    let mut first = supervise(one.clone(), 80, 24, quiet()).unwrap();
    let mut other = supervise(ten.clone(), 80, 24, quiet()).unwrap();
    read_until(&mut first, "a").await;
    first.control.write(b"x\r").unwrap();
    while first.output.recv().await.is_some() {}
    assert_eq!(first.exit.await.unwrap(), Some(1));
    assert_eq!(ten.pane(), Ok(Some(PaneState::Alive)));
    assert!(one.kill().is_ok());
    assert_eq!(listed(&server).iter().map(|l| l.terminal_id.clone()).collect::<Vec<_>>(), ["t10"]);
    other.control.terminate();
    while other.output.recv().await.is_some() {}
    assert_eq!(other.exit.await.unwrap(), None);
    assert_eq!(ten.pane(), Ok(Some(PaneState::Alive)));
}

/// The prefix of the sockets of the reaper test. No other test process reaps
/// it, so the sockets that the test leaves for its reaper stay until it runs.
const REAP_TEST_PREFIX: &str = "synthesis-reaptest-";

/// A server on a socket of the reaper test, which ends with the guard.
struct Left {
    runner: RealTmux,
    dir: PathBuf,
}

impl Left {
    fn start(program: &str, dir: &std::path::Path, pid: i32) -> Left {
        let runner = RealTmux {
            program: program.to_string(),
            socket: format!("{REAP_TEST_PREFIX}{pid}-0-{}", std::process::id()),
        };
        runner
            .run(&commands::join(
                [
                    commands::server_options(),
                    vec![vec![
                        "new-session".into(),
                        "-d".into(),
                        "-s".into(),
                        "left".into(),
                        "sleep 600".into(),
                    ]],
                ]
                .concat(),
            ))
            .expect("the left server starts");
        Left { runner, dir: dir.to_path_buf() }
    }
}

impl Drop for Left {
    fn drop(&mut self) {
        let _ = self.runner.run(&["kill-server".to_string()]);
        remove_socket(&self.dir, &self.runner.socket);
    }
}

fn socket_names(dir: &std::path::Path) -> Vec<String> {
    let access = crate::fs::FsAccess::builder().allow_root(dir).build().unwrap();
    access.list_dir(dir).unwrap().into_iter().map(|e| e.name).collect()
}

// TRS non-functional requirement: a server that a test process left, as when
// nextest stops a test at its time limit, ends with its socket at the next
// run. The server of a live test process stays.
#[test]
fn a_server_of_an_ended_test_process_goes_with_its_socket() {
    let program = find_tmux().expect("tmux 3.2 or later must be installed for these tests");
    let dir = socket_dir(&program).expect("the socket directory");
    let left = Left::start(&program, &dir, ended_pid());
    let live = Left::start(&program, &dir, std::process::id() as i32);
    assert!(socket_names(&dir).contains(&left.runner.socket));

    let reaped = reap_in(&program, &dir, REAP_TEST_PREFIX);
    assert!(reaped.contains(&left.runner.socket), "{reaped:?}");
    assert!(!reaped.contains(&live.runner.socket), "{reaped:?}");
    assert!(left.runner.run(&commands::list()).is_err(), "the left server still answers");
    assert!(!socket_names(&dir).contains(&left.runner.socket), "the socket is still there");
    assert!(live.runner.run(&commands::list()).is_ok(), "the live server ended");
    assert!(socket_names(&dir).contains(&live.runner.socket));
}

// TRS non-functional requirement: the reaper reads only the socket names of
// these tests, with a process id.
#[test]
fn the_reaper_reads_only_test_socket_names() {
    let pid = |name: &str| test_socket_pid(name, SOCKET_PREFIX);
    assert_eq!(pid("synthesis-test-123-0"), Some(123));
    assert_eq!(pid("synthesis-test-123-probe"), Some(123));
    // Signal 0 to pid 0 reaches the caller's group, so it reads as alive.
    assert_eq!(pid("synthesis-test-0-1"), Some(0));
    assert!(process_alive(0));
    for name in [
        "default",
        "synthesis",
        "synthesis-test-",
        "synthesis-test-x-1",
        "synthesis-test-12",
        "synthesis-test-99999999999-1",
        "syn-exp-94935",
    ] {
        assert_eq!(pid(name), None, "{name}");
    }
}

// TRS non-functional requirement: the server of a test that fails ends with
// its socket too.
#[test]
fn the_server_of_a_failed_test_ends_with_its_socket() {
    let (dir, socket, runner) = {
        let server = server();
        let runner = RealTmux {
            program: server.runner.program.clone(),
            socket: server.runner.socket.clone(),
        };
        (server.dir.clone().expect("the socket directory"), server.runner.socket.clone(), runner)
    };
    let failed = std::panic::catch_unwind(|| {
        let server = server();
        server
            .runner
            .run(&commands::join(vec![vec![
                "new-session".into(),
                "-d".into(),
                "-s".into(),
                "s".into(),
                "sleep 600".into(),
            ]]))
            .expect("the server starts");
        let socket = server.runner.socket.clone();
        assert!(socket_names(server.dir.as_ref().unwrap()).contains(&socket));
        std::panic::panic_any(socket);
    });
    let socket_of_failed = *failed.expect_err("the test fails").downcast::<String>().unwrap();
    let ended = RealTmux { program: runner.program.clone(), socket: socket_of_failed.clone() };
    assert!(ended.run(&commands::list()).is_err(), "the server of the failed test still answers");
    assert!(!socket_names(&dir).contains(&socket_of_failed), "the socket is still there");
    assert!(!socket_names(&dir).contains(&socket));
}

// TRS non-functional requirement: the server of a test ends with the test and
// leaves no socket.
#[test]
fn a_test_server_ends_with_its_socket() {
    let server = server();
    let dir = server.dir.clone().expect("the socket directory");
    let socket = server.runner.socket.clone();
    let runner = RealTmux { program: server.runner.program.clone(), socket: socket.clone() };
    runner
        .run(&commands::join(vec![vec![
            "new-session".into(),
            "-d".into(),
            "-s".into(),
            "s".into(),
            "sleep 600".into(),
        ]]))
        .expect("the server starts");
    let access = crate::fs::FsAccess::builder().allow_root(&dir).build().unwrap();
    let has_socket = || access.list_dir(&dir).unwrap().iter().any(|e| e.name == socket);
    assert!(has_socket());
    drop(server);
    assert!(runner.run(&commands::list()).is_err(), "the server still answers");
    assert!(!has_socket(), "the socket is still there");
}
