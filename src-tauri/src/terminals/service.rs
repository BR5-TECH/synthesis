//! The operations of the terminal session module, apart from their Tauri
//! command wrappers (TRS contract surface).

use std::sync::Arc;

use tauri::{AppHandle, Runtime};

use super::gather::{self, ProjectFacts};
use super::input::{InputQueue, InputRefusal};
use super::model::*;
use super::process::ProcessHandle;
use super::registry::{NewSession, Registry};
use super::session::{pump, Session};
use super::targets;
use crate::log_fields;
use crate::logging::{self, Domain};
use crate::project_scope::events::EmitToProject;
use crate::project_scope::ProjectStores;
use crate::tools::agent_exec::interactive::{
    launch_interactive, vm_running, InteractiveLaunchRequest, InteractivePlace, InteractiveProgram,
};
use crate::tools::agent_exec::sandbox::SandboxScope;

/// TRS-FR-EHXS: the event every reader reloads its list on.
pub const TERMINALS_CHANGED: &str = "terminals-changed";

/// The terminal size a process starts at, before its tab fits it.
pub(super) const START_COLS: u16 = 80;
pub(super) const START_ROWS: u16 = 24;

#[derive(Clone, serde::Serialize)]
struct ChangedPayload {}

/// TRS-FR-EHXS.
pub fn announce<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit_project(TERMINALS_CHANGED, ChangedPayload {});
}

/// The registry of one project, as a project store (PWN-FR-ISOL).
#[derive(Default)]
pub struct ProjectTerminals(Arc<Registry>);

pub fn registry<R: Runtime>(app: &AppHandle<R>) -> Arc<Registry> {
    // An application with no project scopes and no managed registry is a test
    // binding of another module. It holds no session, so an empty registry is
    // the true answer for it.
    app.try_project_store::<ProjectTerminals>()
        .map(|terminals| terminals.0.clone())
        .unwrap_or_default()
}

pub(super) fn log_session(app: &AppHandle<impl Runtime>, message: &str, session: &Session) {
    let summary = session.summary();
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        message,
        log_fields! {
            "terminal_id" => summary.id,
            "kind" => format!("{:?}", summary.kind),
            "vendor" => summary.vendor.map(|v| v.id()).unwrap_or(""),
            "runs_in" => format!("{:?}", summary.runs_in),
            "state" => summary.state.code(),
        },
    );
}

pub(super) fn note(app: &AppHandle<impl Runtime>, message: &str, terminal_id: &str, cause: &str) {
    logging::log_info(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        message,
        log_fields! { "terminal_id" => terminal_id, "cause" => cause },
    );
}

pub(super) fn warn(app: &AppHandle<impl Runtime>, message: &str, terminal_id: &str, cause: &str) {
    logging::log_warn(
        app,
        &logging::BUFFER,
        &[Domain::Backend],
        message,
        log_fields! { "terminal_id" => terminal_id, "cause" => cause },
    );
}

fn platform_check() -> Result<(), TerminalError> {
    if cfg!(windows) {
        return Err(TerminalError::new("platform_unsupported"));
    }
    Ok(())
}

/// TRS-FR-ZCPH.
pub fn list_targets<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<TerminalTarget>, TerminalError> {
    platform_check()?;
    let facts = gather::project(app)?;
    let env = gather::environment(app, &facts);
    let worktrees = gather::worktrees(app, facts.root.path()).unwrap_or_default();
    let busy = gather::busy_of_worktree(app);
    Ok(targets::build_targets(&worktrees, busy, |target| {
        let scope = SandboxScope { queue_key: target.queue_scope() };
        let running = env.effective_microsandbox && vm_running(app, &scope);
        targets::kinds_for(&env, running)
    }))
}

/// A terminal id: unique in the process, and safe in a container name.
fn new_terminal_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("t{nanos:x}{sequence:x}")
}

/// TRS-FR-YYER: every session of the project, in creation order.
pub fn list<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<TerminalSummary>, TerminalError> {
    platform_check()?;
    Ok(registry(app).summaries())
}

/// The path of a target's checkout.
fn checkout_path(target: &TargetRef, worktrees: &[targets::WorktreeFact]) -> Option<String> {
    match target {
        TargetRef::Worktree { path } => Some(path.clone()),
        TargetRef::WorkStream { stream_id } => worktrees
            .iter()
            .find(|w| w.stream.as_ref().is_some_and(|(id, _, _)| id == stream_id))
            .map(|w| w.path.clone()),
    }
}

/// TRS-FR-YYER .. TRS-FR-XXPE: starts a session. TRS-FR-ZZZN: a Host shell
/// can start in a folder inside the target's directory.
pub fn create<R: Runtime>(
    app: &AppHandle<R>,
    target: TargetRef,
    kind: TerminalKind,
    vendor: Option<CliVendor>,
    start_folder: Option<String>,
) -> Result<TerminalSummary, TerminalError> {
    platform_check()?;
    let start_folder = start_folder.filter(|folder| !folder.is_empty());
    if start_folder.is_some() && kind != TerminalKind::HostShell {
        return Err(TerminalError::new("start_folder_unsupported"));
    }
    let registry = registry(app);
    let facts = gather::project(app)?;
    let all = list_targets(app)?;
    let Some(found) = all.iter().find(|t| t.target == target) else {
        // TRS-FR-EJWM: a target whose sessions saw it go is gone, not unknown.
        let gone =
            registry.for_target(&target).iter().any(|s| s.state() == TerminalState::TargetGone);
        return Err(TerminalError::new(if gone { "target_gone" } else { "unknown_target" }));
    };
    let vendor = if kind == TerminalKind::AgentCli {
        Some(vendor.ok_or_else(|| TerminalError::new("unknown_target"))?)
    } else {
        None
    };
    let availability = found
        .kinds
        .iter()
        .find(|k| k.kind == kind && k.vendor == vendor)
        .cloned()
        .ok_or_else(|| TerminalError::new("unknown_target"))?;
    if let Some(reason) = availability.reason.filter(|_| !availability.available) {
        return Err(TerminalError::kind_unavailable(reason));
    }
    let worktrees = gather::worktrees(app, facts.root.path()).unwrap_or_default();
    let checkout =
        checkout_path(&target, &worktrees).ok_or_else(|| TerminalError::new("unknown_target"))?;
    // TRS-FR-ZZZN: a refused start folder starts nothing and records no session.
    let working_dir = super::host::working_dir(kind, &checkout, start_folder.as_deref())?;
    let runs_in = availability.runs_in;
    let session = registry.add(
        new_terminal_id(),
        crate::notes::now_rfc3339(),
        NewSession {
            target: target.clone(),
            target_name: found.name.clone(),
            branch: found.branch.clone(),
            kind,
            vendor,
            runs_in,
            busy_run_id: found.busy_run_id.clone(),
            initial_state: if runs_in == RunsIn::Host {
                TerminalState::Running
            } else {
                TerminalState::Starting
            },
        },
    )?;
    if runs_in == RunsIn::Host {
        if let Err(err) = start_host(app, &session, &facts, &working_dir) {
            // TRS-FR-VGGS: the session stays, exited with its failed start.
            warn(app, "terminal start failed", &session.spec.id, err.code);
            session.start_failed(&err.to_string());
        }
    } else {
        start_in_place(app, session.clone(), checkout, target);
    }
    log_session(app, "terminal created", &session);
    announce(app);
    Ok(session.summary())
}

fn start_host<R: Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    facts: &ProjectFacts,
    checkout: &str,
) -> Result<(), TerminalError> {
    #[cfg(unix)]
    {
        let command = match (session.spec.kind, session.spec.vendor) {
            (TerminalKind::AgentCli, Some(vendor)) => {
                let env = gather::environment(app, facts);
                let executable = env.verified_vendors.get(&vendor).cloned().ok_or_else(|| {
                    TerminalError::kind_unavailable(UnavailableReason::VendorUnverified)
                })?;
                super::host::host_agent_command(vendor, &executable, checkout)
            }
            _ => {
                let shell_var = std::env::var("SHELL").ok();
                let shell = super::host::login_shell(shell_var.as_deref(), |p| {
                    crate::fs::probe_entry_is_dir(std::path::Path::new(p)) == Some(false)
                });
                super::host::host_shell_command(&shell, checkout)
            }
        };
        if persistent_kind(session.spec.kind, session.spec.runs_in) {
            match super::tmux::source(app).probe() {
                Ok(tmux) => {
                    match super::persistent::start(app, session, tmux, &facts.project_key, &command)
                    {
                        Ok(()) => return Ok(()),
                        // TRS-FR-FIIJ: the session starts as one that is not persistent.
                        Err(cause) => {
                            warn(app, "terminal tmux call failed", &session.spec.id, cause)
                        }
                    }
                }
                // TRS-FR-OMYR: the record says why the session is not persistent.
                Err(cause) => note(app, "terminal not persistent", &session.spec.id, cause),
            }
        }
        let handle = super::process::spawn_host(&command, START_COLS, START_ROWS, None, None)
            .map_err(|e| TerminalError::with_message("start_failed", e.to_string()))?;
        run(app, session.clone(), handle);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (app, session, facts, checkout);
        Err(TerminalError::new("platform_unsupported"))
    }
}

/// TRS-FR-FKVW: the kinds that run in tmux where tmux is usable.
pub fn persistent_kind(kind: TerminalKind, runs_in: RunsIn) -> bool {
    runs_in == RunsIn::Host && matches!(kind, TerminalKind::HostShell | TerminalKind::AgentCli)
}

/// TRS-FR-HWNA: a VM process with no exit code ended with its VM.
pub fn end_reason_for(microsandbox: bool, code: Option<i32>) -> EndReason {
    match code {
        None if microsandbox => EndReason::VmStopped,
        _ => EndReason::ProcessExited,
    }
}

/// Starts the pump of a running process, and records its end.
pub(super) fn run<R: Runtime>(app: &AppHandle<R>, session: Arc<Session>, handle: ProcessHandle) {
    let environment_name = handle.environment_name.clone();
    if !session.started(handle.control.clone(), environment_name) {
        handle.control.terminate();
        return;
    }
    let microsandbox = session.spec.runs_in == RunsIn::Microsandbox;
    let app = app.clone();
    crate::project_scope::context::spawn(async move {
        let ended = session.clone();
        let notify = app.clone();
        pump(
            session,
            handle,
            move |code| end_reason_for(microsandbox, code),
            move || {
                log_session(&notify, "terminal ended", &ended);
                announce(&notify);
            },
        )
        .await;
    });
}

/// TRS-FR-HXRL, TRS-FR-XXCY, TRS-FR-VGGS: a launch through EAC's interactive path.
fn start_in_place<R: Runtime>(
    app: &AppHandle<R>,
    session: Arc<Session>,
    checkout: String,
    target: TargetRef,
) {
    let place = match session.spec.runs_in {
        RunsIn::Docker => InteractivePlace::Docker,
        RunsIn::Devcontainer => InteractivePlace::Devcontainer,
        _ => InteractivePlace::Microsandbox,
    };
    let program = match (session.spec.kind, session.spec.vendor) {
        (TerminalKind::AgentCli, Some(vendor)) => InteractiveProgram::AgentCli(vendor),
        _ => InteractiveProgram::Shell,
    };
    let sandbox_scope = (session.spec.runs_in == RunsIn::Microsandbox)
        .then(|| SandboxScope { queue_key: target.queue_scope() });
    let progress_session = session.clone();
    let request = InteractiveLaunchRequest {
        execution_directory: std::path::PathBuf::from(checkout),
        program,
        place,
        sandbox_scope,
        terminal_id: session.spec.id.clone(),
        cols: START_COLS,
        rows: START_ROWS,
        progress: Some(Arc::new(move |bytes: &[u8]| progress_session.push_output(bytes))),
    };
    let app = app.clone();
    crate::project_scope::context::spawn(async move {
        match launch_interactive(&app, request).await {
            Ok(handle) => run(&app, session.clone(), handle),
            Err(line) => {
                session.start_failed(&line);
                warn(&app, "terminal start failed", &session.spec.id, "launch_failed");
            }
        }
        announce(&app);
    });
}

fn ensure_platform() -> Result<(), TerminalError> {
    platform_check()
}

/// TRS-FR-HIIK.
pub fn attach<R: Runtime>(
    app: &AppHandle<R>,
    terminal_id: &str,
    sink: Arc<dyn super::session::OutputSink>,
) -> Result<TerminalAttachment, TerminalError> {
    ensure_platform()?;
    Ok(registry(app).get(terminal_id)?.attach(sink))
}

/// TRS-FR-NULR.
pub fn detach<R: Runtime>(app: &AppHandle<R>, terminal_id: &str) -> Result<(), TerminalError> {
    ensure_platform()?;
    let session = registry(app).get(terminal_id)?;
    let had = session.has_history();
    session.detach();
    if had != session.has_history() {
        announce(app);
    }
    Ok(())
}

/// TRS-FR-AVNH: the most input one call carries.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;

/// TRS-FR-AVNH, TRS-FR-GSYR, TRS-FR-DUME: queues the input and returns
/// without waiting for the process.
pub fn write_input<R: Runtime>(
    app: &AppHandle<R>,
    terminal_id: &str,
    data: &[u8],
) -> Result<(), TerminalError> {
    ensure_platform()?;
    if data.len() > MAX_INPUT_BYTES {
        return Err(TerminalError::new("input_too_large"));
    }
    let session = registry(app).get(terminal_id)?;
    let result = session.queue_input(data, |control| {
        let app = app.clone();
        let id = terminal_id.to_string();
        InputQueue::spawn(control, move |e| {
            warn(&app, "terminal write failed", &id, &e.kind().to_string());
        })
    });
    match result {
        Ok(()) => Ok(()),
        Err(InputRefusal::NotRunning) => Err(TerminalError::new("terminal_not_running")),
        Err(InputRefusal::BacklogFull { first }) => {
            if first {
                warn(app, "terminal input backlog full", terminal_id, "input_backlog_full");
            }
            Err(TerminalError::new("input_backlog_full"))
        }
    }
}

/// TRS-FR-UNIL: the accepted sizes.
pub fn valid_size(cols: u16, rows: u16) -> bool {
    (2..=1000).contains(&cols) && (1..=500).contains(&rows)
}

/// TRS-FR-UNIL.
pub fn resize<R: Runtime>(
    app: &AppHandle<R>,
    terminal_id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), TerminalError> {
    ensure_platform()?;
    if !valid_size(cols, rows) {
        return Err(TerminalError::new("invalid_size"));
    }
    let session = registry(app).get(terminal_id)?;
    // TRS-FR-UNIL: a session that is starting gets the size when it runs.
    session.set_size(cols, rows);
    if session.take_wait_for_size() {
        super::persistent::start_restored_later(app, &session, cols, rows);
        return Ok(());
    }
    if let Some(control) = session.control() {
        control
            .resize(cols, rows)
            .map_err(|e| TerminalError::with_message("terminal_not_running", e.to_string()))?;
    }
    Ok(())
}

/// TRS-FR-VYIB.
pub fn acknowledge<R: Runtime>(
    app: &AppHandle<R>,
    terminal_id: &str,
    bytes: usize,
) -> Result<(), TerminalError> {
    ensure_platform()?;
    registry(app).get(terminal_id)?.acknowledge(bytes);
    Ok(())
}

/// TRS-FR-KOYL.
pub fn set_pinned<R: Runtime>(
    app: &AppHandle<R>,
    terminal_id: &str,
    pinned: bool,
) -> Result<TerminalSummary, TerminalError> {
    ensure_platform()?;
    let session = registry(app).get(terminal_id)?;
    session.set_pinned(pinned);
    super::persistent::set_pinned(app, &session);
    announce(app);
    Ok(session.summary())
}

/// TRS-FR-BTMT.
pub fn set_label<R: Runtime>(
    app: &AppHandle<R>,
    terminal_id: &str,
    label: &str,
) -> Result<TerminalSummary, TerminalError> {
    ensure_platform()?;
    let session = registry(app).get(terminal_id)?;
    let label = clean_label(label)?;
    session.set_label(label);
    super::persistent::set_label(app, &session);
    announce(app);
    Ok(session.summary())
}

/// TRS-FR-DOMD.
pub fn set_theme<R: Runtime>(
    app: &AppHandle<R>,
    terminal_id: &str,
    theme: Option<&str>,
) -> Result<TerminalSummary, TerminalError> {
    ensure_platform()?;
    let session = registry(app).get(terminal_id)?;
    let theme = parse_theme(theme)?;
    session.set_theme(theme);
    super::persistent::set_theme(app, &session);
    announce(app);
    Ok(session.summary())
}

/// TRS-FR-CXSV.
pub fn close<R: Runtime>(app: &AppHandle<R>, terminal_id: &str) -> Result<(), TerminalError> {
    ensure_platform()?;
    let registry = registry(app);
    let session = registry.get(terminal_id)?;
    end_session(&session);
    super::persistent::kill_later(app, &session);
    registry.remove(terminal_id);
    log_session(app, "terminal closed", &session);
    announce(app);
    Ok(())
}

fn end_session(session: &Session) {
    if let Some(control) = session.close() {
        control.terminate();
    }
}

/// TRS-FR-RKAW, TRS-FR-NPDK: every session of a gone target ends and stays.
pub fn end_for_target<R: Runtime>(app: &AppHandle<R>, target: &TargetRef) {
    let sessions = registry(app).for_target(target);
    let mut changed = false;
    for session in sessions {
        if session.state() == TerminalState::TargetGone {
            continue;
        }
        if let Some(control) = session.target_gone() {
            control.terminate();
        }
        super::persistent::kill_later(app, &session);
        log_session(app, "terminal target gone", &session);
        changed = true;
    }
    if changed {
        announce(app);
    }
}

/// TRS-FR-OTLH: a project close ends every session of the project. For a
/// persistent session, it ends the tmux client alone.
pub fn end_for_project<R: Runtime>(app: &AppHandle<R>) {
    for session in registry(app).drain() {
        end_session(&session);
        log_session(app, "terminal closed", &session);
    }
}

/// Whether a target is present in the enumeration (TRS-FR-RKAW).
pub fn target_present(target: &TargetRef, worktrees: &[targets::WorktreeFact]) -> bool {
    match target {
        TargetRef::Worktree { path } => worktrees.iter().any(|w| &w.path == path && !w.is_missing),
        TargetRef::WorkStream { stream_id } => worktrees
            .iter()
            .any(|w| !w.is_missing && w.stream.as_ref().is_some_and(|(id, _, _)| id == stream_id)),
    }
}

/// TRS-FR-LUUX: the run that holds a target's queue now.
pub fn target_busy(
    target: &TargetRef,
    worktrees: &[targets::WorktreeFact],
    busy_of_worktree: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    match target {
        TargetRef::Worktree { path } => busy_of_worktree(path),
        TargetRef::WorkStream { stream_id } => worktrees
            .iter()
            .find_map(|w| w.stream.as_ref().filter(|(id, _, _)| id == stream_id))
            .and_then(|(_, _, busy)| busy.clone()),
    }
}

/// TRS-FR-RKAW: the targets of `sessions` that are gone now. A target whose
/// sessions all saw it go already is not gone again.
pub fn gone_targets(
    sessions: &[(TargetRef, TerminalState)],
    worktrees: &[targets::WorktreeFact],
) -> Vec<TargetRef> {
    let mut gone: Vec<TargetRef> = Vec::new();
    for (target, state) in sessions {
        if *state != TerminalState::TargetGone
            && !target_present(target, worktrees)
            && !gone.contains(target)
        {
            gone.push(target.clone());
        }
    }
    gone
}

/// TRS-FR-RKAW, TRS-FR-LUUX: one pass of the watch. A target whose directory
/// is gone ends its sessions; a change of a run state is announced.
pub fn check_targets<R: Runtime>(app: &AppHandle<R>) {
    let registry = registry(app);
    let sessions = registry.all();
    if sessions.is_empty() {
        return;
    }
    let Ok(facts) = gather::project(app) else { return };
    // TRS-FR-RKAW: an enumeration that fails says nothing about any target.
    let Some(worktrees) = gather::worktrees(app, facts.root.path()) else { return };
    let busy = gather::busy_of_worktree(app);
    let states: Vec<(TargetRef, TerminalState)> =
        sessions.iter().map(|s| (s.spec.target.clone(), s.state())).collect();
    let gone = gone_targets(&states, &worktrees);
    let mut changed = false;
    for session in &sessions {
        if session.state().is_live() && !gone.contains(&session.spec.target) {
            changed |=
                session.set_busy_run_id(target_busy(&session.spec.target, &worktrees, &busy));
        }
    }
    for target in gone {
        end_for_target(app, &target);
    }
    if changed {
        announce(app);
    }
}

/// TRS-FR-RKAW: the period of the directory check.
pub const CHECK_PERIOD: std::time::Duration = std::time::Duration::from_secs(5);

/// Starts the watch of TRS-FR-RKAW once for the process. It runs for every open
/// project, in that project's context.
pub fn start_watch<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(CHECK_PERIOD).await;
            crate::project_scope::for_each_project(&app, || check_targets(&app));
        }
    });
}

/// TRS-FR-OTLH: an application exit ends every session of every project.
pub fn end_all<R: Runtime>(app: &AppHandle<R>) {
    crate::project_scope::for_each_project(app, || end_for_project(app));
    // The container removals run on threads and tasks of their own. The exit
    // waits for them, with a bound, so no container outlives the process.
    if !crate::tools::agent_exec::interactive::removals::wait_all(EXIT_REMOVAL_BOUND) {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Backend],
            "terminal container removal did not end before exit",
            log_fields! { "bound_seconds" => EXIT_REMOVAL_BOUND.as_secs() },
        );
    }
}

/// TRS-FR-OTLH: the longest time an exit waits for container removals.
const EXIT_REMOVAL_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(
        path: &str,
        missing: bool,
        stream: Option<(&str, Option<&str>)>,
    ) -> targets::WorktreeFact {
        targets::WorktreeFact {
            path: path.into(),
            name: path.into(),
            branch: None,
            is_active: false,
            is_missing: missing,
            stream: stream.map(|(id, busy)| (id.into(), id.into(), busy.map(str::to_string))),
        }
    }

    // TRS-FR-RKAW: a missing worktree, a worktree Git no longer lists, and a
    // stream with no working copy are gone; a target already gone is not.
    #[test]
    fn finds_the_gone_targets() {
        let worktrees = vec![
            fact("/here", false, None),
            fact("/missing", true, None),
            fact("/s", false, Some(("s-1", None))),
        ];
        let wt = |p: &str| TargetRef::Worktree { path: p.into() };
        let st = |id: &str| TargetRef::WorkStream { stream_id: id.into() };
        let sessions = vec![
            (wt("/here"), TerminalState::Running),
            (wt("/missing"), TerminalState::Running),
            (wt("/unlisted"), TerminalState::Exited),
            (st("s-1"), TerminalState::Running),
            (st("s-2"), TerminalState::Starting),
            (wt("/old"), TerminalState::TargetGone),
            (wt("/missing"), TerminalState::Running),
        ];
        assert_eq!(
            gone_targets(&sessions, &worktrees),
            vec![wt("/missing"), wt("/unlisted"), st("s-2")]
        );
    }

    // TRS-FR-LUUX: a worktree's busy run comes from its queue, a stream's from its record.
    #[test]
    fn reads_the_busy_run_of_a_target() {
        let worktrees = vec![fact("/s", false, Some(("s-1", Some("run-s"))))];
        let busy = |path: &str| (path == "/w").then(|| "run-w".to_string());
        assert_eq!(
            target_busy(&TargetRef::Worktree { path: "/w".into() }, &worktrees, &busy).as_deref(),
            Some("run-w")
        );
        assert_eq!(
            target_busy(&TargetRef::WorkStream { stream_id: "s-1".into() }, &worktrees, &busy)
                .as_deref(),
            Some("run-s")
        );
        assert_eq!(
            target_busy(&TargetRef::WorkStream { stream_id: "s-9".into() }, &worktrees, &busy),
            None
        );
    }

    // TRS-FR-UNIL: 2 to 1000 columns and 1 to 500 rows.
    #[test]
    fn accepts_only_the_stated_sizes() {
        assert!(valid_size(2, 1));
        assert!(valid_size(1000, 500));
        assert!(!valid_size(1, 24));
        assert!(!valid_size(1001, 24));
        assert!(!valid_size(80, 0));
        assert!(!valid_size(80, 501));
    }
}
