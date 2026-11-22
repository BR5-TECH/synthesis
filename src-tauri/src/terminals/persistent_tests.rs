use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{Listener, Manager};

use crate::project_scope::{context, ProjectScopes};
use crate::terminals::service::{close, end_for_project, end_for_target, resize, set_pinned};
use crate::terminals::tmux::fake::{fake, Fake};
use crate::terminals::tmux::meta::{encode, project_prefix, session_name};

const KEY: &str = "/repo";

struct Fixture {
    app: tauri::App<tauri::test::MockRuntime>,
    announces: Arc<AtomicUsize>,
    f: Fake,
}

fn fixture() -> Fixture {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let announces = Arc::new(AtomicUsize::new(0));
    let counter = announces.clone();
    app.listen_any(service::TERMINALS_CHANGED, move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    Fixture { app, announces, f: fake() }
}

fn worktree(path: &str) -> TargetRef {
    TargetRef::Worktree { path: path.into() }
}

fn fact(path: &str) -> WorktreeFact {
    WorktreeFact {
        path: path.into(),
        name: "repo".into(),
        branch: Some("main".into()),
        is_active: true,
        is_missing: false,
        stream: None,
    }
}

fn record(_id: &str, label: &str, created_at: &str, pinned: bool) -> Meta {
    Meta {
        project: KEY.into(),
        label: label.into(),
        target: worktree("/repo"),
        kind: TerminalKind::HostShell,
        vendor: None,
        runs_in: RunsIn::Host,
        pinned,
        created_at: created_at.into(),
    }
}

/// One line of `list-sessions` for a session of `key`.
fn line(key: &str, id: &str, meta: &Meta, pane: &str) -> String {
    let target = match &meta.target {
        TargetRef::Worktree { path } => format!("worktree:{path}"),
        TargetRef::WorkStream { stream_id } => format!("work_stream:{stream_id}"),
    };
    [
        session_name(key, id),
        encode(&meta.project),
        encode(&meta.label),
        encode(&target),
        "host_shell".into(),
        String::new(),
        "host".into(),
        if meta.pinned { "1".into() } else { "0".into() },
        encode(&meta.created_at),
        pane.to_string(),
    ]
    .join("\t")
}

fn no_busy(_: &str) -> Option<String> {
    None
}

fn restore(fx: &Fixture, worktrees: Option<&[WorktreeFact]>) -> usize {
    restore_with(fx.app.handle(), fx.f.tmux.clone(), KEY, worktrees, &no_busy)
}

fn history_of(session: &Session) -> String {
    let sink = Arc::new(Recorder::default());
    session.attach(sink.clone());
    let bytes = sink.0.lock().unwrap().concat();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<Vec<u8>>>);

impl crate::terminals::session::OutputSink for Recorder {
    fn send(&self, frame: Vec<u8>) -> Result<(), crate::terminals::session::SinkClosed> {
        self.0.lock().unwrap().push(frame);
        Ok(())
    }
}

/// Waits for a state that a task of the async runtime reaches.
fn wait_until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..20_000 {
        if done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("never reached: {what}");
}

// TRS-FR-IEKU, TRS-FR-QMZM, TRS-FR-TEHT: the live sessions of the project come
// back with their id, label, target, pinned flag, and creation time, as
// running and persistent, with one announce; another project's are left out.
#[test]
fn restores_the_live_sessions_of_the_project() {
    let fx = fixture();
    let listing = [
        line(KEY, "t2", &record("t2", "Host shell 2", "2026-11-22T10:00:02Z", false), "0\t"),
        line(KEY, "t1", &record("t1", "Host shell 1", "2026-11-22T10:00:01Z", true), "0\t"),
        line("/other", "t3", &record("t3", "Host shell 1", "2026-11-22T10:00:00Z", false), "0\t"),
    ]
    .join("\n");
    fx.f.runner.reply("list-sessions", Ok(listing));
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        assert_eq!(restore(&fx, Some(&[fact("/repo")])), 2);
        let summaries = registry(&handle).summaries();
        assert_eq!(summaries.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["t1", "t2"]);
        let first = &summaries[0];
        assert_eq!(first.label, "Host shell 1");
        assert_eq!(first.target, worktree("/repo"));
        assert_eq!(first.target_name, "repo");
        assert_eq!(first.branch.as_deref(), Some("main"));
        assert_eq!(first.state, TerminalState::Running);
        assert!(first.persistent && first.pinned);
        assert!(!summaries[1].pinned);
        assert_eq!(first.created_at, "2026-11-22T10:00:01Z");
        assert!(fx.f.runner.calls_of("kill-session").is_empty());
        assert_eq!(fx.f.clients.count(), 0);
    });
    assert_eq!(fx.announces.load(Ordering::SeqCst), 1);
}

// TRS-FR-IEKU: a second restore adds nothing and announces nothing.
#[test]
fn a_second_restore_adds_nothing() {
    let fx = fixture();
    let listing = line(KEY, "t1", &record("t1", "Host shell 1", "2026", false), "0\t");
    fx.f.runner.always("list-sessions", Ok(listing));
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        assert_eq!(restore(&fx, Some(&[fact("/repo")])), 1);
        assert_eq!(restore(&fx, Some(&[fact("/repo")])), 0);
        assert_eq!(registry(&handle).len(), 1);
    });
    assert_eq!(fx.announces.load(Ordering::SeqCst), 1);
}

// TRS-FR-HJZR, TRS-FR-KBOS: a session whose process ended comes back exited
// with its status and every line of the pane, and its tmux session goes.
#[test]
fn a_dead_pane_comes_back_exited_with_its_lines() {
    let fx = fixture();
    fx.f.runner.reply(
        "list-sessions",
        Ok(line(KEY, "t1", &record("t1", "Host shell 1", "2026", false), "1\t7")),
    );
    fx.f.runner.reply("capture-pane", Ok("one\ntwo\n".into()));
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        restore(&fx, Some(&[fact("/repo")]));
        let session = registry(&handle).get("t1").unwrap();
        let summary = session.summary();
        assert_eq!(summary.state, TerminalState::Exited);
        assert_eq!(summary.exit_code, Some(7));
        assert_eq!(summary.end_reason, Some(EndReason::ProcessExited));
        assert!(!summary.persistent);
        assert_eq!(
            fx.f.runner.calls_of("capture-pane"),
            vec![commands::capture(&session_name(KEY, "t1"), false)]
        );
        assert_eq!(history_of(&session), "one\r\ntwo\r\n\x1b[0m");
        assert_eq!(
            fx.f.runner.calls_of("kill-session"),
            vec![commands::kill(&session_name(KEY, "t1"))]
        );
    });
}

// TRS-FR-QCOP, TRS-FR-NPDK: a session whose target is gone comes back as
// target gone with its lines, and its tmux session goes.
#[test]
fn a_gone_target_comes_back_as_target_gone() {
    let fx = fixture();
    fx.f.runner.reply(
        "list-sessions",
        Ok(line(KEY, "t1", &record("t1", "Host shell 1", "2026", false), "0\t")),
    );
    fx.f.runner.reply("capture-pane", Ok("kept\n".into()));
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        restore(&fx, Some(&[fact("/elsewhere")]));
        let session = registry(&handle).get("t1").unwrap();
        let summary = session.summary();
        assert_eq!(summary.state, TerminalState::TargetGone);
        assert_eq!(summary.target_name, "repo");
        assert!(history_of(&session).starts_with("kept\r\n"));
        assert_eq!(fx.f.runner.calls_of("kill-session").len(), 1);
    });
}

// TRS-FR-RKAW, TRS-FR-IEKU: where the worktrees cannot be read, no target is
// gone, and the session comes back running.
#[test]
fn an_unread_enumeration_makes_no_target_gone() {
    let fx = fixture();
    fx.f.runner.reply(
        "list-sessions",
        Ok(line(KEY, "t1", &record("t1", "Host shell 1", "2026", false), "0\t")),
    );
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        restore(&fx, None);
        assert_eq!(registry(&handle).get("t1").unwrap().state(), TerminalState::Running);
        assert!(fx.f.runner.calls_of("kill-session").is_empty());
    });
}

// TRS-FR-TFAN, TRS-FR-AYCY: the oldest sessions come back while the project
// holds fewer than 16; the rest stay alive and unlisted.
#[test]
fn restores_the_oldest_up_to_the_limit() {
    let fx = fixture();
    let listing: Vec<String> = (0..18)
        .rev()
        .map(|i| {
            let id = format!("t{i:02}");
            line(
                KEY,
                &id,
                &record(
                    &id,
                    &format!("Host shell {i}"),
                    &format!("2026-11-22T10:00:{i:02}Z"),
                    false,
                ),
                "0\t",
            )
        })
        .collect();
    fx.f.runner.always("list-sessions", Ok(listing.join("\n")));
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        assert_eq!(restore(&fx, Some(&[fact("/repo")])), MAX_SESSIONS);
        let ids: Vec<String> = registry(&handle).summaries().into_iter().map(|s| s.id).collect();
        assert_eq!(ids.first().map(String::as_str), Some("t00"));
        assert_eq!(ids.last().map(String::as_str), Some("t15"));
        assert!(fx.f.runner.calls_of("kill-session").is_empty());
        // A session that leaves the registry while its tmux session lives
        // comes back first, as it is the oldest.
        registry(&handle).remove("t03");
        assert_eq!(restore(&fx, Some(&[fact("/repo")])), 1);
        assert!(registry(&handle).contains("t03"));
        assert!(!registry(&handle).contains("t16"));
    });
    let last = records("terminal restore read").pop().unwrap();
    assert_eq!(last.fields.get("found").and_then(|v| v.as_u64()), Some(18));
    assert_eq!(last.fields.get("restored").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(last.fields.get("held_already").and_then(|v| v.as_u64()), Some(15));
    assert_eq!(last.fields.get("past_limit").and_then(|v| v.as_u64()), Some(2));
}

// TRS-FR-IEKU, TRS-FR-LXIE: no server means nothing to restore and no warning;
// a listing that fails otherwise emits a WARN record.
#[test]
fn a_listing_that_fails() {
    let fx = fixture();
    fx.f.runner.reply("list-sessions", Err(TmuxError::Status));
    fx.f.runner.reply("list-sessions", Err(TmuxError::Timeout));
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        let before = warnings("terminal restore failed");
        assert_eq!(restore(&fx, None), 0);
        assert_eq!(warnings("terminal restore failed"), before);
        assert_eq!(restore(&fx, None), 0);
        assert_eq!(warnings("terminal restore failed"), before + 1);
        assert!(registry(&handle).is_empty());
    });
    assert_eq!(fx.announces.load(Ordering::SeqCst), 0);
}

fn records(message: &str) -> Vec<crate::logging::LogRecord> {
    crate::logging::BUFFER
        .query(&crate::logging::LogFilter::default(), None, 10_000)
        .unwrap()
        .records
        .into_iter()
        .filter(|r| r.message == message)
        .collect()
}

fn warnings(message: &str) -> usize {
    records(message).into_iter().filter(|r| r.level == crate::logging::LogLevel::Warn).count()
}

// TRS-FR-LXIE, TRS-FR-CLDC: each restored session emits one INFO record that
// names the session and holds no path, label, or output.
#[test]
fn a_restore_is_logged_without_a_path() {
    let fx = fixture();
    let mut meta = record("tlog", "Secret label", "2026", false);
    meta.target = worktree("/secret/checkout");
    fx.f.runner.reply("list-sessions", Ok(line(KEY, "tlog", &meta, "1\t0")));
    fx.f.runner.reply("capture-pane", Ok("hidden-output\n".into()));
    context::run_in("p", || {
        restore(&fx, Some(&[fact("/secret/checkout")]));
    });
    let restored: Vec<_> = records("terminal restored")
        .into_iter()
        .filter(|r| r.fields.get("terminal_id").and_then(|v| v.as_str()) == Some("tlog"))
        .collect();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].level, crate::logging::LogLevel::Info);
    let text = serde_json::to_string(&restored[0].fields).unwrap();
    assert!(!text.contains("/secret/checkout"));
    assert!(!text.contains("hidden-output"));
    assert!(!text.contains("Secret label"));
}

/// Restores one live session `t1` and gives it.
fn restored_live(fx: &Fixture) -> Arc<Session> {
    fx.f.runner.reply(
        "list-sessions",
        Ok(line(KEY, "t1", &record("t1", "Host shell 1", "2026", false), "0\t")),
    );
    restore(fx, Some(&[fact("/repo")]));
    registry(fx.app.handle()).get("t1").unwrap()
}

// TRS-FR-TCBS: the first size starts the client at that size, after the
// pane's lines above the screen and one line feed for each row; a later size
// only resizes.
#[test]
fn the_first_size_starts_the_client_after_the_scrollback() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        let session = restored_live(&fx);
        fx.f.runner.reply("has-session", Ok("0\t\n".into()));
        fx.f.runner.reply("display-message", Ok("2\n".into()));
        fx.f.runner.reply("capture-pane", Ok("\x1b[31mred\x1b[0m\nplain\n".into()));
        assert!(service::write_input(&handle, "t1", b"early").is_err());
        resize(&handle, "t1", 100, 3).unwrap();
        wait_until("the client starts", || session.control().is_some());
        assert_eq!(
            fx.f.runner.calls_of("capture-pane"),
            vec![commands::capture(&session_name(KEY, "t1"), true)]
        );
        assert_eq!(history_of(&session), "\x1b[31mred\x1b[0m\r\nplain\r\n\x1b[0m\r\n\r\n\r\n");
        assert_eq!(fx.f.clients.count(), 1);
        assert_eq!(fx.f.clients.all()[0].start_size, (100, 3));
        resize(&handle, "t1", 120, 40).unwrap();
        assert_eq!(fx.f.clients.count(), 1);
        assert_eq!(fx.f.clients.all()[0].sizes.lock().unwrap().last(), Some(&(120, 40)));
        assert_eq!(fx.f.runner.calls_of("capture-pane").len(), 1);
        assert!(service::write_input(&handle, "t1", b"ls\n").is_ok());
        assert_eq!(fx.f.clients.all()[0].writes.lock().unwrap().as_slice(), &[b"ls\n".to_vec()]);
    });
}

// TRS-FR-TCBS, TRS-FR-HJZR: a pane that died before the first size ends the
// session with its status and lines, and no client starts.
#[test]
fn a_pane_dead_at_the_first_size_ends_the_session() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        let session = restored_live(&fx);
        fx.f.runner.reply("has-session", Ok("1\t2\n".into()));
        fx.f.runner.reply("capture-pane", Ok("last\n".into()));
        resize(&handle, "t1", 80, 24).unwrap();
        wait_until("the kill", || !fx.f.runner.calls_of("kill-session").is_empty());
        wait_until("the session ends", || session.state() == TerminalState::Exited);
        assert_eq!(session.summary().exit_code, Some(2));
        assert_eq!(fx.f.clients.count(), 0);
        assert_eq!(fx.f.runner.calls_of("kill-session").len(), 1);
        assert!(history_of(&session).starts_with("last\r\n"));
    });
}

// TRS-FR-CXSV, TRS-FR-IEKU: a close of a restored session that has no client
// yet removes its tmux session and starts no client.
#[test]
fn a_close_before_the_first_size_removes_the_session() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        restored_live(&fx);
        close(&handle, "t1").unwrap();
        wait_until("the kill", || !fx.f.runner.calls_of("kill-session").is_empty());
        assert_eq!(
            fx.f.runner.calls_of("kill-session"),
            vec![commands::kill(&session_name(KEY, "t1"))]
        );
        assert_eq!(resize(&handle, "t1", 80, 24).unwrap_err().code, "unknown_terminal");
        assert!(registry(&handle).is_empty());
        assert_eq!(fx.f.clients.count(), 0);
    });
}

fn command() -> HostCommand {
    HostCommand {
        program: "/bin/zsh".into(),
        args: vec!["-l".into(), "-i".into()],
        cwd: "/repo".into(),
        env: crate::terminals::process::terminal_env(),
        env_remove: vec![],
    }
}

/// Starts a new persistent Host shell `id` on the fake.
fn started(fx: &Fixture, id: &str) -> Arc<Session> {
    let handle = fx.app.handle();
    let session = registry(handle)
        .add(
            id.into(),
            "2026-11-22T10:00:00Z".into(),
            NewSession {
                target: worktree("/repo"),
                target_name: "repo".into(),
                branch: None,
                kind: TerminalKind::HostShell,
                vendor: None,
                runs_in: RunsIn::Host,
                busy_run_id: None,
                initial_state: TerminalState::Running,
            },
        )
        .unwrap();
    start(handle, &session, fx.f.tmux.clone(), KEY, &command()).unwrap();
    session
}

// TRS-FR-FKVW, TRS-FR-BHCB, TRS-FR-QMZM: a start creates the tmux session with
// its metadata and attaches a client; the session is persistent.
#[test]
fn a_start_creates_the_tmux_session_and_a_client() {
    let fx = fixture();
    context::run_in("p", || {
        let session = started(&fx, "t1");
        let creates = fx.f.runner.calls_of("start-server");
        assert_eq!(creates.len(), 1);
        let name = session_name(KEY, "t1");
        assert!(creates[0].contains(&name));
        assert!(creates[0].contains(&"/repo".to_string()));
        assert!(creates[0].contains(&"@synthesis_label".to_string()));
        assert!(creates[0].contains(&"Host shell 1".to_string()));
        assert!(session.summary().persistent);
        assert_eq!(fx.f.clients.count(), 1);
        assert_eq!(fx.f.clients.all()[0].session, name);
        assert!(name.starts_with(&project_prefix(KEY)));
    });
}

// TRS-FR-FIIJ: a start whose tmux call fails leaves no tmux session and no
// persistence, so the caller starts a plain session.
#[test]
fn a_failed_start_leaves_nothing() {
    let fx = fixture();
    fx.f.runner.reply("start-server", Err(TmuxError::Timeout));
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        let session = registry(&handle)
            .add(
                "t1".into(),
                String::new(),
                NewSession {
                    target: worktree("/repo"),
                    target_name: "repo".into(),
                    branch: None,
                    kind: TerminalKind::HostShell,
                    vendor: None,
                    runs_in: RunsIn::Host,
                    busy_run_id: None,
                    initial_state: TerminalState::Running,
                },
            )
            .unwrap();
        assert_eq!(
            start(&handle, &session, fx.f.tmux.clone(), KEY, &command()),
            Err("tmux_timeout")
        );
        assert!(!session.summary().persistent);
        assert_eq!(fx.f.runner.calls_of("kill-session").len(), 1);
        assert_eq!(fx.f.clients.count(), 0);
        fx.f.clients.fail.store(true, Ordering::SeqCst);
        // The cleanup kill fails too: the session is still not persistent.
        fx.f.runner.reply("kill-session", Err(TmuxError::Timeout));
        assert_eq!(
            start(&handle, &session, fx.f.tmux.clone(), KEY, &command()),
            Err("tmux_attach_failed")
        );
        assert!(!session.summary().persistent);
        assert!(session.persistence().is_none());
    });
}

// TRS-FR-CXSV: a close ends the client and removes the tmux session.
#[test]
fn a_close_removes_the_tmux_session() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        started(&fx, "t1");
        close(&handle, "t1").unwrap();
        assert_eq!(fx.f.clients.all()[0].terminated.load(Ordering::SeqCst), 1);
        wait_until("the kill", || !fx.f.runner.calls_of("kill-session").is_empty());
        assert_eq!(
            fx.f.runner.calls_of("kill-session"),
            vec![commands::kill(&session_name(KEY, "t1"))]
        );
        assert!(registry(&handle).is_empty());
    });
}

// TRS-FR-OTLH, TRS-FR-KOYL: a project close ends the client of every
// persistent session, a pinned one too, and removes no tmux session.
#[test]
fn a_project_close_keeps_the_tmux_sessions() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        let pinned = started(&fx, "t1");
        set_pinned(&handle, "t1", true).unwrap();
        assert!(pinned.pinned());
        started(&fx, "t2");
        end_for_project(&handle);
        for client in fx.f.clients.all() {
            assert_eq!(client.terminated.load(Ordering::SeqCst), 1);
        }
        assert!(fx.f.runner.calls_of("kill-session").is_empty());
        assert!(registry(&handle).is_empty());
    });
}

// TRS-FR-NPDK: a gone target ends the client and removes the tmux session;
// the session stays with its history.
#[test]
fn a_gone_target_removes_the_tmux_session() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        let session = started(&fx, "t1");
        end_for_target(&handle, &worktree("/repo"));
        assert_eq!(session.state(), TerminalState::TargetGone);
        wait_until("the kill", || !session.summary().persistent);
        assert_eq!(fx.f.runner.calls_of("kill-session").len(), 1);
        assert_eq!(fx.f.clients.all()[0].terminated.load(Ordering::SeqCst), 1);
        assert!(registry(&handle).contains("t1"));
    });
}

// TRS-FR-KOYL, TRS-FR-QMZM: a pin change of a persistent session also sets
// its tmux option.
#[test]
fn a_pin_change_sets_the_option() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        started(&fx, "t1");
        set_pinned(&handle, "t1", true).unwrap();
        wait_until("the first option", || pins(&fx).len() == 1);
        set_pinned(&handle, "t1", false).unwrap();
        wait_until("the second option", || pins(&fx).len() == 2);
        let name = session_name(KEY, "t1");
        assert_eq!(
            pins(&fx),
            vec![
                commands::set_option(&name, "@synthesis_pinned", "1"),
                commands::set_option(&name, "@synthesis_pinned", "0"),
            ]
        );
    });
}

/// The pin options that the fake received.
fn pins(fx: &Fixture) -> Vec<Vec<String>> {
    fx.f.runner
        .calls_of("set-option")
        .into_iter()
        .filter(|c| c.contains(&"@synthesis_pinned".to_string()))
        .collect()
}

// TRS-FR-KBOS, TRS-FR-DVBF: a process that ends while the IDE runs ends the
// session with its exit status, and the session is then not persistent.
#[test]
fn a_process_end_reports_its_status() {
    let fx = fixture();
    fx.f.runner.reply("has-session", Ok("1\t4\n".into()));
    context::run_in("p", || {
        let session = started(&fx, "t1");
        fx.f.clients.all()[0].end();
        wait_until("the session ends", || session.state() == TerminalState::Exited);
        assert_eq!(session.summary().exit_code, Some(4));
        assert!(!session.summary().persistent);
        assert_eq!(fx.f.runner.calls_of("kill-session").len(), 1);
    });
}

// TRS-FR-FKVW: only a Host shell and an Agent CLI on the host are persistent.
#[test]
fn the_persistent_kinds() {
    use crate::terminals::service::persistent_kind;
    assert!(persistent_kind(TerminalKind::HostShell, RunsIn::Host));
    assert!(persistent_kind(TerminalKind::AgentCli, RunsIn::Host));
    assert!(!persistent_kind(TerminalKind::AgentCli, RunsIn::Docker));
    assert!(!persistent_kind(TerminalKind::AgentCli, RunsIn::Devcontainer));
    assert!(!persistent_kind(TerminalKind::ContainerShell, RunsIn::Docker));
    assert!(!persistent_kind(TerminalKind::MicrosandboxShell, RunsIn::Microsandbox));
}

// TRS-FR-OMYR: where tmux is not usable, a restore reads nothing; a test binary
// never reaches the user's own tmux server.
#[test]
fn no_usable_tmux_restores_nothing() {
    let fx = fixture();
    fx.app.manage(crate::terminals::tmux::ManagedTmuxSource(Arc::new(
        crate::terminals::tmux::fake::FakeSource(None),
    )));
    assert!(crate::terminals::tmux::source(fx.app.handle()).usable().is_none());
    assert!(crate::terminals::tmux::source(tauri::test::mock_app().handle()).usable().is_none());
    context::run_in("p", || assert_eq!(restore_now(fx.app.handle()), 0));
}

// TRS-FR-CXSV, TRS-FR-NPDK: a session that the module ended while its tmux
// session started loses that tmux session at once.
#[test]
fn a_session_ended_during_its_start_loses_its_tmux_session() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        let session = registry(&handle)
            .add(
                "t1".into(),
                String::new(),
                NewSession {
                    target: worktree("/repo"),
                    target_name: "repo".into(),
                    branch: None,
                    kind: TerminalKind::HostShell,
                    vendor: None,
                    runs_in: RunsIn::Host,
                    busy_run_id: None,
                    initial_state: TerminalState::Running,
                },
            )
            .unwrap();
        session.close();
        start(&handle, &session, fx.f.tmux.clone(), KEY, &command()).unwrap();
        assert_eq!(
            fx.f.runner.calls_of("kill-session"),
            vec![commands::kill(&session_name(KEY, "t1"))]
        );
        assert_eq!(fx.f.clients.all()[0].terminated.load(Ordering::SeqCst), 1);
    });
}

// TRS-FR-QMZM: the pin task writes the flag that the session holds when it
// runs, so the last change is what tmux keeps.
#[test]
fn the_last_pin_change_wins() {
    let fx = fixture();
    let handle = fx.app.handle().clone();
    context::run_in("p", || {
        started(&fx, "t1");
        set_pinned(&handle, "t1", true).unwrap();
        set_pinned(&handle, "t1", false).unwrap();
        wait_until("both options", || pins(&fx).len() == 2);
        assert!(pins(&fx)[1].ends_with(&["0".to_string()]));
    });
}
