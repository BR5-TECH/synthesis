//! The module against a real tmux, on a socket of each test's own. A machine
//! with no usable tmux fails these tests: CI installs tmux.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::locate::{find_tmux, version_usable};
use super::meta::{parse_list, session_name, Meta, PaneState};
use super::runner::{run_bounded, RealClients, RealTmux, CALL_TIMEOUT};
use super::supervisor::{create, supervise, Persistence, Warn};
use super::{commands, Tmux, TmuxRunner};
use crate::terminals::model::*;
use crate::terminals::process::{terminal_env, HostCommand, ProcessHandle};

const KEY: &str = "/real/project";

/// A tmux server on a socket of its own, which ends with the test.
struct Server {
    tmux: Arc<Tmux>,
    runner: Arc<RealTmux>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.runner.run(&["kill-server".to_string()]);
    }
}

fn server() -> Server {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let program = find_tmux().expect("tmux 3.2 or later must be installed for these tests");
    let mut version = std::process::Command::new(&program);
    version.arg("-V");
    let text = run_bounded(version, CALL_TIMEOUT).expect("tmux -V");
    assert!(version_usable(&text), "tmux 3.2 or later must be installed, found {text}");
    let socket =
        format!("synthesis-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
    let runner = Arc::new(RealTmux { program: program.clone(), socket: socket.clone() });
    let tmux = Arc::new(Tmux {
        runner: runner.clone(),
        clients: Arc::new(RealClients { program, socket }),
    });
    Server { tmux, runner }
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

// TRS-FR-ANJK, TRS-FR-ZOZO: the server of the socket has no status bar, no
// prefix key, mouse off, and a history of 10,000 lines.
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
