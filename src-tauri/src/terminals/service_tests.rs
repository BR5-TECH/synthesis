use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{Listener, Manager};

use crate::project_scope::{context, ProjectScopes};

#[derive(Default)]
struct Control {
    terminated: AtomicUsize,
}

impl ProcessControl for Control {
    fn write(&self, _data: &[u8]) -> std::io::Result<()> {
        Ok(())
    }
    fn resize(&self, _cols: u16, _rows: u16) -> std::io::Result<()> {
        Ok(())
    }
    fn terminate(&self) {
        self.terminated.fetch_add(1, Ordering::SeqCst);
    }
}

struct Fixture {
    app: tauri::App<tauri::test::MockRuntime>,
    announces: Arc<AtomicUsize>,
}

fn fixture() -> Fixture {
    let app = tauri::test::mock_app();
    app.manage(ProjectScopes::default());
    let announces = Arc::new(AtomicUsize::new(0));
    let counter = announces.clone();
    app.listen_any(TERMINALS_CHANGED, move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    Fixture { app, announces }
}

fn worktree(path: &str) -> TargetRef {
    TargetRef::Worktree { path: path.into() }
}

/// Adds a session of `target` to the project's registry, running on a fake control.
fn add(
    handle: &AppHandle<tauri::test::MockRuntime>,
    id: &str,
    target: TargetRef,
    pinned: bool,
) -> Arc<Control> {
    let session = registry(handle)
        .add(
            id.into(),
            String::new(),
            NewSession {
                target,
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
    let control = Arc::new(Control::default());
    assert!(session.started(control.clone(), None));
    session.push_output(b"history");
    session.set_pinned(pinned);
    control
}

// TRS-FR-CXSV, TRS-FR-EHXS: a close terminates the process once, removes the
// session and its history, and announces once; the id is then unknown.
#[test]
fn close_ends_and_removes_the_session() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        let control = add(&handle, "a", worktree("/r"), true);
        close(&handle, "a").unwrap();
        assert_eq!(control.terminated.load(Ordering::SeqCst), 1);
        assert!(registry(&handle).all().is_empty());
        assert_eq!(close(&handle, "a").unwrap_err().code, "unknown_terminal");
    });
    assert_eq!(f.announces.load(Ordering::SeqCst), 1);
}

// TRS-FR-BTMT, TRS-FR-EHXS: a rename trims the label, changes nothing else,
// and announces once; a bad label or an unknown id changes nothing.
#[test]
fn a_rename_sets_the_trimmed_label() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        add(&handle, "a", worktree("/r"), true);
        let before = registry(&handle).get("a").unwrap().summary();
        let summary = set_label(&handle, "a", "  my shell ").unwrap();
        assert_eq!(summary.label, "my shell");
        assert_eq!(TerminalSummary { label: before.label.clone(), ..summary.clone() }, before);
        assert_eq!(set_label(&handle, "a", "   ").unwrap_err().code, "invalid_label");
        assert_eq!(set_label(&handle, "a", &"x".repeat(65)).unwrap_err().code, "invalid_label");
        assert_eq!(set_label(&handle, "zz", "name").unwrap_err().code, "unknown_terminal");
        assert_eq!(registry(&handle).get("a").unwrap().summary().label, "my shell");
    });
    assert_eq!(f.announces.load(Ordering::SeqCst), 1);
}

// TRS-FR-BTMT: two sessions can hold the same label.
#[test]
fn two_sessions_can_share_a_label() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        add(&handle, "a", worktree("/r"), false);
        add(&handle, "b", worktree("/r"), false);
        set_label(&handle, "a", "same").unwrap();
        assert_eq!(set_label(&handle, "b", "same").unwrap().label, "same");
    });
}

// TRS-FR-DOMD, TRS-FR-EHXS: a theme change sets or clears the override, and
// announces once for each change; another value changes nothing.
#[test]
fn a_theme_change_sets_and_clears_the_override() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        add(&handle, "a", worktree("/r"), false);
        assert_eq!(registry(&handle).get("a").unwrap().summary().theme, None);
        assert_eq!(set_theme(&handle, "a", Some("sand")).unwrap().theme, Some(TerminalTheme::Sand));
        assert_eq!(set_theme(&handle, "a", Some("dark")).unwrap_err().code, "invalid_theme");
        assert_eq!(registry(&handle).get("a").unwrap().summary().theme, Some(TerminalTheme::Sand));
        assert_eq!(set_theme(&handle, "a", None).unwrap().theme, None);
        assert_eq!(set_theme(&handle, "zz", None).unwrap_err().code, "unknown_terminal");
    });
    assert_eq!(f.announces.load(Ordering::SeqCst), 2);
}

// TRS-FR-RKAW, TRS-FR-NPDK: every session of a gone target, live or ended,
// goes to target_gone with its history; another target stays as it is; a
// second call changes nothing.
#[test]
fn a_gone_target_ends_its_sessions_and_keeps_them() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        let live = add(&handle, "a", worktree("/gone"), false);
        let ended = add(&handle, "b", worktree("/gone"), false);
        registry(&handle).get("b").unwrap().process_ended(Some(0), EndReason::ProcessExited);
        let other = add(&handle, "c", worktree("/here"), false);
        end_for_target(&handle, &worktree("/gone"));
        let summaries = registry(&handle).summaries();
        let state = |id: &str| summaries.iter().find(|s| s.id == id).unwrap().clone();
        assert_eq!(state("a").state, TerminalState::TargetGone);
        assert_eq!(state("b").state, TerminalState::TargetGone);
        assert!(state("a").has_history && state("b").has_history);
        assert_eq!(state("c").state, TerminalState::Running);
        assert_eq!(live.terminated.load(Ordering::SeqCst), 1);
        assert_eq!(ended.terminated.load(Ordering::SeqCst), 0);
        assert_eq!(other.terminated.load(Ordering::SeqCst), 0);
        end_for_target(&handle, &worktree("/gone"));
        assert_eq!(live.terminated.load(Ordering::SeqCst), 1);
    });
    assert_eq!(f.announces.load(Ordering::SeqCst), 1);
}

// TRS-FR-OTLH, TRS-FR-KOYL: a project close ends every session, a pinned one too.
#[test]
fn a_project_close_ends_every_session() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        let pinned = add(&handle, "a", worktree("/r"), true);
        let plain = add(&handle, "b", worktree("/r"), false);
        end_for_project(&handle);
        assert_eq!(pinned.terminated.load(Ordering::SeqCst), 1);
        assert_eq!(plain.terminated.load(Ordering::SeqCst), 1);
        assert!(registry(&handle).all().is_empty());
    });
}

// PWN-FR-ISOL, TRS-FR-VTYD: the sessions of one project are not another's.
#[test]
fn projects_hold_their_own_sessions() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        add(&handle, "a", worktree("/r"), false);
    });
    context::run_in("q", || {
        assert!(registry(&handle).all().is_empty());
        assert_eq!(close(&handle, "a").unwrap_err().code, "unknown_terminal");
    });
}

// TRS-FR-AVNH: more than 64 KiB is refused, and a session that is not running
// receives nothing.
#[test]
fn input_rules() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        add(&handle, "a", worktree("/r"), false);
        let big = vec![b'x'; MAX_INPUT_BYTES + 1];
        assert_eq!(write_input(&handle, "a", &big).unwrap_err().code, "input_too_large");
        assert!(write_input(&handle, "a", b"ls\n").is_ok());
        registry(&handle).get("a").unwrap().process_ended(Some(0), EndReason::ProcessExited);
        assert_eq!(write_input(&handle, "a", b"ls\n").unwrap_err().code, "terminal_not_running");
    });
}

// TRS-FR-UNIL: a size outside the bounds is refused.
#[test]
fn resize_refuses_a_size_outside_the_bounds() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        add(&handle, "a", worktree("/r"), false);
        assert_eq!(resize(&handle, "a", 1, 24).unwrap_err().code, "invalid_size");
        assert!(resize(&handle, "a", 120, 40).is_ok());
    });
}

// TRS-FR-HWNA: a VM process that ends with no exit code ended with its VM.
#[test]
fn the_end_reason_of_a_vm_process() {
    assert_eq!(end_reason_for(true, None), EndReason::VmStopped);
    assert_eq!(end_reason_for(true, Some(0)), EndReason::ProcessExited);
    assert_eq!(end_reason_for(false, None), EndReason::ProcessExited);
}

// TRS-FR-BGDL: the queue scope of a worktree is the graduation queue key.
#[test]
fn the_queue_scope_is_the_graduation_queue_key() {
    assert_eq!(
        worktree("/repo/wt").queue_scope(),
        crate::graduation::worktree_queue_key("/repo/wt")
    );
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

// TRS-FR-LXIE, TRS-FR-CLDC: a close emits one INFO record that names the
// session, its kind, and its state, and holds no path, input, or output.
#[test]
fn a_close_is_logged_without_a_path_or_output() {
    let f = fixture();
    let handle = f.app.handle().clone();
    context::run_in("p", || {
        add(&handle, "logged", worktree("/secret/checkout"), false);
        write_input(&handle, "logged", b"echo hidden-input\n").unwrap();
        close(&handle, "logged").unwrap();
    });
    let closed: Vec<_> = records("terminal closed")
        .into_iter()
        .filter(|r| r.fields.get("terminal_id").and_then(|v| v.as_str()) == Some("logged"))
        .collect();
    assert_eq!(closed.len(), 1);
    let record = &closed[0];
    assert_eq!(record.level, crate::logging::LogLevel::Info);
    for key in ["terminal_id", "kind", "vendor", "runs_in", "state"] {
        assert!(record.fields.contains_key(key), "missing {key}");
    }
    let text = serde_json::to_string(&record.fields).unwrap();
    assert!(!text.contains("/secret/checkout"));
    assert!(!text.contains("hidden-input"));
    assert!(!text.contains("history"));
}

// TRS-FR-YZMR: the platform check refuses on Windows alone.
#[test]
fn the_platform_check() {
    let result = list(&tauri::test::mock_app().handle().clone());
    if cfg!(windows) {
        assert_eq!(result.unwrap_err().code, "platform_unsupported");
    } else {
        assert!(result.is_ok());
    }
}
