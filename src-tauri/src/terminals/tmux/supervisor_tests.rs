use super::*;
use crate::terminals::model::*;
use crate::terminals::tmux::fake::{fake, Fake};

fn persistence(f: &Fake) -> Arc<Persistence> {
    Arc::new(Persistence::new(f.tmux.clone(), "syn-abc-t1".into()))
}

type Warnings = Arc<Mutex<Vec<&'static str>>>;

fn recorder() -> (Warn, Warnings) {
    let seen: Warnings = Arc::default();
    let into = seen.clone();
    (Arc::new(move |cause| into.lock().unwrap().push(cause)), seen)
}

fn meta() -> Meta {
    Meta {
        project: "/p".into(),
        label: "Host shell 1".into(),
        target: TargetRef::Worktree { path: "/p".into() },
        kind: TerminalKind::HostShell,
        vendor: None,
        runs_in: RunsIn::Host,
        pinned: false,
        theme: None,
        created_at: "2026-11-22T10:00:00Z".into(),
    }
}

fn command() -> HostCommand {
    HostCommand {
        program: "/bin/sh".into(),
        args: vec![],
        cwd: "/p".into(),
        env: crate::terminals::process::terminal_env(),
        env_remove: vec![],
    }
}

/// Waits until the fake has spawned `n` clients.
async fn clients(f: &Fake, n: usize) -> Vec<Arc<crate::terminals::tmux::fake::FakeClient>> {
    loop {
        if f.clients.count() >= n {
            return f.clients.all();
        }
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

async fn drain(handle: &mut ProcessHandle) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(chunk) = handle.output.recv().await {
        out.extend(chunk);
    }
    out
}

// TRS-FR-BHCB, TRS-FR-CKIC: a client that ends while its pane lives is replaced
// at the last size, input and size go to the client of the moment, and the
// output of both clients reaches the one handle in order.
#[tokio::test]
async fn a_client_that_ends_is_replaced() {
    let f = fake();
    f.runner.always("has-session", Ok("0\t\n".into()));
    let (warn, warnings) = recorder();
    let mut handle = supervise(persistence(&f), 80, 24, warn).unwrap();
    let first = clients(&f, 1).await[0].clone();
    assert_eq!(first.session, "syn-abc-t1");
    assert_eq!(first.start_size, (80, 24));
    handle.control.write(b"a").unwrap();
    handle.control.resize(120, 40).unwrap();
    first.output(b"one ").await;
    first.end();
    let second = clients(&f, 2).await[1].clone();
    assert_eq!(second.start_size, (120, 40));
    // The new client becomes the current one just after it starts.
    while handle.control.write(b"b").is_err() {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    second.output(b"two").await;
    // The pane is dead at the second end: the handle ends with its status.
    f.runner.reply("has-session", Ok("1\t5\n".into()));
    second.end();
    assert_eq!(drain(&mut handle).await, b"one two");
    assert_eq!(handle.exit.await.unwrap(), Some(5));
    assert_eq!(first.writes.lock().unwrap().as_slice(), &[b"a".to_vec()]);
    assert_eq!(first.sizes.lock().unwrap().as_slice(), &[(120, 40)]);
    assert_eq!(second.writes.lock().unwrap().as_slice(), &[b"b".to_vec()]);
    assert!(warnings.lock().unwrap().is_empty());
}

// TRS-FR-KBOS: a dead pane removes the tmux session, and the handle reports
// the exit status.
#[tokio::test]
async fn a_dead_pane_removes_the_session_and_reports_its_status() {
    let f = fake();
    f.runner.reply("has-session", Ok("1\t3\n".into()));
    let (warn, _) = recorder();
    let p = persistence(&f);
    let mut handle = supervise(p.clone(), 80, 24, warn).unwrap();
    clients(&f, 1).await[0].end();
    drain(&mut handle).await;
    assert_eq!(handle.exit.await.unwrap(), Some(3));
    assert_eq!(f.runner.calls_of("kill-session"), vec![commands::kill("syn-abc-t1")]);
    assert_eq!(f.clients.count(), 1);
    assert!(p.is_gone());
}

// TRS-FR-KBOS: a tmux session that is gone ends the handle with no exit code,
// and nothing is removed.
#[tokio::test]
async fn a_gone_session_ends_with_no_code() {
    let f = fake();
    f.runner.reply("has-session", Err(TmuxError::Status));
    let (warn, warnings) = recorder();
    let p = persistence(&f);
    let mut handle = supervise(p.clone(), 80, 24, warn).unwrap();
    clients(&f, 1).await[0].end();
    drain(&mut handle).await;
    assert_eq!(handle.exit.await.unwrap(), None);
    assert!(f.runner.calls_of("kill-session").is_empty());
    assert!(warnings.lock().unwrap().is_empty());
    assert!(p.is_gone());
}

// TRS-FR-CKIC: after 3 clients end while the pane lives, the handle ends with
// no exit code and a warning, and the tmux session stays.
#[tokio::test]
async fn three_client_ends_stop_the_attaches() {
    let f = fake();
    f.runner.always("has-session", Ok("0\t\n".into()));
    let (warn, warnings) = recorder();
    let mut handle = supervise(persistence(&f), 80, 24, warn).unwrap();
    for n in 1..=MAX_CLIENT_ENDS {
        clients(&f, n).await[n - 1].end();
    }
    drain(&mut handle).await;
    assert_eq!(handle.exit.await.unwrap(), None);
    assert_eq!(f.clients.count(), MAX_CLIENT_ENDS);
    assert!(f.runner.calls_of("kill-session").is_empty());
    assert_eq!(warnings.lock().unwrap().as_slice(), &["tmux_client_ends_again"]);
}

// TRS-FR-OTLH: a terminate ends the client alone: no new client attaches, the
// tmux session is not removed, and no pane state is read.
#[tokio::test]
async fn terminate_ends_the_client_alone() {
    let f = fake();
    let (warn, _) = recorder();
    let mut handle = supervise(persistence(&f), 80, 24, warn).unwrap();
    let first = clients(&f, 1).await[0].clone();
    handle.control.terminate();
    drain(&mut handle).await;
    assert_eq!(handle.exit.await.unwrap(), None);
    assert_eq!(first.terminated.load(Ordering::SeqCst), 1);
    assert_eq!(f.clients.count(), 1);
    assert!(f.runner.calls().is_empty());
    assert!(handle.control.write(b"x").is_err());
}

// TRS-FR-CKIC: a new client that does not start ends the handle with a warning.
#[tokio::test]
async fn a_failed_attach_ends_the_handle() {
    let f = fake();
    f.runner.always("has-session", Ok("0\t\n".into()));
    let (warn, warnings) = recorder();
    let mut handle = supervise(persistence(&f), 80, 24, warn).unwrap();
    f.clients.fail.store(true, Ordering::SeqCst);
    clients(&f, 1).await[0].end();
    drain(&mut handle).await;
    assert_eq!(handle.exit.await.unwrap(), None);
    assert_eq!(warnings.lock().unwrap().as_slice(), &["tmux_attach_failed"]);
}

// TRS-FR-LXIE: a pane state that cannot be read ends the handle with a warning
// that names the cause.
#[tokio::test]
async fn an_unreadable_pane_state_warns() {
    let f = fake();
    f.runner.reply("has-session", Err(TmuxError::Timeout));
    let (warn, warnings) = recorder();
    let mut handle = supervise(persistence(&f), 80, 24, warn).unwrap();
    clients(&f, 1).await[0].end();
    drain(&mut handle).await;
    assert_eq!(handle.exit.await.unwrap(), None);
    assert_eq!(warnings.lock().unwrap().as_slice(), &["tmux_timeout"]);
}

// TRS-FR-CKIC: ends older than the window do not count.
#[test]
fn only_recent_client_ends_count() {
    let base = Instant::now();
    let mut ends = VecDeque::new();
    assert!(!record_end(&mut ends, base));
    assert!(!record_end(&mut ends, base + Duration::from_secs(1)));
    assert!(!record_end(&mut ends, base + Duration::from_secs(12)));
    assert!(!record_end(&mut ends, base + Duration::from_secs(13)));
    assert!(record_end(&mut ends, base + Duration::from_secs(14)));
}

// TRS-FR-KBOS, TRS-FR-LXIE: a pane state that tmux gives in a form the module
// cannot read is an error, not a gone session.
#[test]
fn an_unreadable_pane_answer_is_an_error() {
    let f = fake();
    f.runner.reply("has-session", Ok(String::new()));
    let p = persistence(&f);
    assert_eq!(p.pane(), Err(TmuxError::Output));
    assert!(!p.is_gone());
}

// TRS-FR-TCBS: with no history, the capture above the screen reads nothing.
#[test]
fn no_history_captures_nothing_above_the_screen() {
    let f = fake();
    f.runner.reply("display-message", Ok("0\n".into()));
    let p = persistence(&f);
    assert_eq!(p.capture(true).unwrap(), "");
    assert!(f.runner.calls_of("capture-pane").is_empty());
    f.runner.reply("display-message", Ok("12\n".into()));
    f.runner.reply("capture-pane", Ok("old\n".into()));
    assert_eq!(p.capture(true).unwrap(), "old\n");
}

// TRS-FR-FIIJ: a first client that does not start is an error.
#[test]
fn a_first_client_that_does_not_start_is_an_error() {
    let f = fake();
    f.clients.fail.store(true, Ordering::SeqCst);
    let (warn, _) = recorder();
    assert!(supervise(persistence(&f), 80, 24, warn).is_err());
}

// TRS-FR-BHCB, TRS-FR-QMZM: a create runs one command list that makes the
// session, and then hides the dead-pane line where tmux can.
#[test]
fn create_runs_one_command_list() {
    let f = fake();
    let p = create(f.tmux.clone(), "syn-abc-t1".into(), &command(), &meta(), &|_| {}).unwrap();
    assert_eq!(p.name, "syn-abc-t1");
    let calls = f.runner.calls();
    assert_eq!(calls.len(), 2 + commands::unbind_all().len());
    assert_eq!(calls[0], commands::create("syn-abc-t1", &command(), &meta()));
    assert_eq!(&calls[1..calls.len() - 1], commands::unbind_all().as_slice());
    assert_eq!(calls.last(), Some(&commands::quiet_dead_pane()));
}

// TRS-FR-FIIJ: a create that fails removes the tmux session it may have made.
#[test]
fn a_failed_create_removes_the_session() {
    let f = fake();
    f.runner.reply("start-server", Err(TmuxError::Status));
    let err =
        create(f.tmux.clone(), "syn-abc-t1".into(), &command(), &meta(), &|_| {}).err().unwrap();
    assert_eq!(err, TmuxError::Status);
    assert_eq!(f.runner.calls_of("kill-session"), vec![commands::kill("syn-abc-t1")]);
}

// TRS-FR-QMZM, TRS-FR-BTMT, TRS-FR-DOMD: a label write sets the encoded
// label, and a theme write sets the theme id, or an empty value for none.
#[test]
fn label_and_theme_writes() {
    let f = fake();
    let p = persistence(&f);
    p.write_label(|| "a\tb;c".into()).unwrap();
    p.write_theme(|| Some(TerminalTheme::Mist)).unwrap();
    p.write_theme(|| None).unwrap();
    assert_eq!(
        f.runner.calls_of("set-option"),
        vec![
            commands::set_option("syn-abc-t1", "@synthesis_label", "a%09b%3Bc"),
            commands::set_option("syn-abc-t1", "@synthesis_theme", "mist"),
            commands::set_option("syn-abc-t1", "@synthesis_theme", ""),
        ]
    );
}

// TRS-FR-QMZM, TRS-FR-CXSV: a pin change sets the option, and a kill of a
// session that is gone already is no failure.
#[test]
fn pin_and_kill() {
    let f = fake();
    let p = persistence(&f);
    p.set_pinned(true).unwrap();
    assert_eq!(
        f.runner.calls_of("set-option"),
        vec![commands::set_option("syn-abc-t1", "@synthesis_pinned", "1")]
    );
    f.runner.reply("kill-session", Err(TmuxError::Status));
    assert!(p.kill().is_ok());
    assert!(p.is_gone());
    f.runner.reply("kill-session", Err(TmuxError::Timeout));
    assert_eq!(p.kill(), Err(TmuxError::Timeout));
}
