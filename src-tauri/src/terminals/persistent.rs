//! Persistent sessions in the terminal service: the start in tmux, the
//! restore on a project open, and the first client of a restored session
//! (TRS-FR-FKVW .. TRS-FR-TCBS).

use std::sync::Arc;

use tauri::{AppHandle, Runtime};

use super::gather;
use super::model::*;
use super::process::HostCommand;
use super::registry::{NewSession, MAX_SESSIONS};
use super::service::{self, announce, log_session, registry, warn};
use super::session::Session;
use super::targets::WorktreeFact;
use super::tmux::meta::{self, Listed, Meta, PaneState};
use super::tmux::supervisor::{self, Persistence, Warn};
use super::tmux::{commands, Tmux, TmuxError};

/// A `WARN` record for a failed tmux call of one session (TRS-FR-LXIE).
fn warner<R: Runtime>(app: &AppHandle<R>, terminal_id: &str) -> Warn {
    let app = app.clone();
    let id = terminal_id.to_string();
    Arc::new(move |cause| warn(&app, "terminal tmux call failed", &id, cause))
}

/// TRS-FR-BHCB, TRS-FR-QMZM: starts `session` in a new tmux session and
/// attaches its first client. An error leaves no tmux session, and the caller
/// starts the session as one that is not persistent (TRS-FR-FIIJ).
pub fn start<R: Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    tmux: Arc<Tmux>,
    project_key: &str,
    command: &HostCommand,
) -> Result<(), &'static str> {
    let spec = &session.spec;
    let record = Meta {
        project: project_key.to_string(),
        label: session.label(),
        target: spec.target.clone(),
        kind: spec.kind,
        vendor: spec.vendor,
        runs_in: spec.runs_in,
        pinned: session.pinned(),
        theme: session.theme(),
        created_at: spec.created_at.clone(),
    };
    let name = meta::session_name(project_key, &spec.id);
    let report = warner(app, &spec.id);
    let persistence =
        supervisor::create(tmux, name, command, &record, &*report).map_err(TmuxError::cause)?;
    // The session holds its tmux session before a client starts, so a gone
    // target or a close meanwhile removes it too.
    session.set_persistence(persistence.clone());
    let handle = match supervisor::supervise(
        persistence.clone(),
        service::START_COLS,
        service::START_ROWS,
        report,
    ) {
        Ok(handle) => handle,
        Err(e) => {
            warn(
                app,
                "terminal tmux call failed",
                &spec.id,
                &format!("tmux_attach_failed:{:?}", e.kind()),
            );
            let _ = persistence.kill();
            session.clear_persistence();
            return Err("tmux_attach_failed");
        }
    };
    service::run(app, session.clone(), handle);
    // A close or a gone target while the tmux session started found no tmux
    // session to remove; it goes now.
    if session.is_ended_by_module() {
        kill(app, session);
    }
    Ok(())
}

/// TRS-FR-CXSV, TRS-FR-NPDK: removes the tmux session of a persistent session.
pub fn kill<R: Runtime>(app: &AppHandle<R>, session: &Session) {
    let Some(persistence) = session.persistence().filter(|p| !p.is_gone()) else { return };
    if let Err(e) = persistence.kill() {
        warn(app, "terminal tmux call failed", &session.spec.id, e.cause());
    }
}

/// Runs `work` for a persistent session off the caller's thread, which can
/// be the thread the window draws from. A session that is not persistent
/// costs nothing.
fn off_thread<R: Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    work: impl FnOnce(&AppHandle<R>, &Session) + Send + 'static,
) {
    if session.persistence().is_none() {
        return;
    }
    let app = app.clone();
    let session = session.clone();
    crate::project_scope::context::spawn_blocking(move || work(&app, &session));
}

/// [`kill`], off the caller's thread.
pub fn kill_later<R: Runtime>(app: &AppHandle<R>, session: &Arc<Session>) {
    off_thread(app, session, |app, session| kill(app, session));
}

/// TRS-FR-QMZM: keeps a pin change in the tmux session, off the caller's thread.
/// The task writes the flag that the session holds when it runs, so of two
/// quick changes the last one is what tmux keeps.
pub fn set_pinned<R: Runtime>(app: &AppHandle<R>, session: &Arc<Session>) {
    off_thread(app, session, move |app, session| {
        let Some(persistence) = session.persistence().filter(|p| !p.is_gone()) else { return };
        if let Err(e) = persistence.write_pinned(|| session.pinned()) {
            warn(app, "terminal tmux call failed", &session.spec.id, e.cause());
        }
    });
}

/// TRS-FR-QMZM, TRS-FR-BTMT: keeps a label change in the tmux session, off
/// the caller's thread, on the terms of [`set_pinned`].
pub fn set_label<R: Runtime>(app: &AppHandle<R>, session: &Arc<Session>) {
    off_thread(app, session, move |app, session| {
        let Some(persistence) = session.persistence().filter(|p| !p.is_gone()) else { return };
        if let Err(e) = persistence.write_label(|| session.label()) {
            warn(app, "terminal tmux call failed", &session.spec.id, e.cause());
        }
    });
}

/// TRS-FR-QMZM, TRS-FR-DOMD: keeps a theme change in the tmux session, off
/// the caller's thread, on the terms of [`set_pinned`].
pub fn set_theme<R: Runtime>(app: &AppHandle<R>, session: &Arc<Session>) {
    off_thread(app, session, move |app, session| {
        let Some(persistence) = session.persistence().filter(|p| !p.is_gone()) else { return };
        if let Err(e) = persistence.write_theme(|| session.theme()) {
            warn(app, "terminal tmux call failed", &session.spec.id, e.cause());
        }
    });
}

/// TRS-FR-TCBS: [`start_restored`], off the caller's thread.
pub fn start_restored_later<R: Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    cols: u16,
    rows: u16,
) {
    let target = session.clone();
    off_thread(app, session, move |app, _| start_restored(app, &target, cols, rows));
}

/// TRS-FR-IEKU: restores the persistent sessions of the project that opens,
/// off the caller's thread.
pub fn restore_for_project<R: Runtime>(app: &AppHandle<R>) {
    if cfg!(windows) {
        return;
    }
    let app = app.clone();
    crate::project_scope::context::spawn_blocking(move || {
        restore_now(&app);
    });
}

fn restore_now<R: Runtime>(app: &AppHandle<R>) -> usize {
    let Some(tmux) = super::tmux::source(app).usable() else { return 0 };
    let Ok(facts) = gather::project(app) else { return 0 };
    let worktrees = gather::worktrees(app, facts.root.path());
    let busy = gather::busy_of_worktree(app);
    restore_with(app, tmux, &facts.project_key, worktrees.as_deref(), &busy)
}

/// The name and the branch that a restored session shows for its target.
fn describe(target: &TargetRef, worktrees: Option<&[WorktreeFact]>) -> (String, Option<String>) {
    let facts = worktrees.unwrap_or_default();
    match target {
        TargetRef::Worktree { path } => match facts.iter().find(|w| &w.path == path) {
            Some(w) => (w.name.clone(), w.branch.clone()),
            None => (
                std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone()),
                None,
            ),
        },
        TargetRef::WorkStream { stream_id } => {
            let found =
                facts.iter().find(|w| w.stream.as_ref().is_some_and(|(id, _, _)| id == stream_id));
            match found {
                Some(w) => (
                    w.stream.as_ref().map(|(_, name, _)| name.clone()).unwrap_or_default(),
                    w.branch.clone(),
                ),
                None => (stream_id.clone(), None),
            }
        }
    }
}

/// Converts tmux's line feeds to the CR LF a terminal needs, and ends the
/// text with a reset of the colours.
fn as_terminal_lines(captured: &str) -> Vec<u8> {
    if captured.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(captured.len() + captured.lines().count() + 8);
    for line in captured.split_inclusive('\n') {
        match line.strip_suffix('\n') {
            Some(body) => {
                out.extend_from_slice(body.trim_end_matches('\r').as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            None => out.extend_from_slice(line.as_bytes()),
        }
    }
    out.extend_from_slice(b"\x1b[0m");
    out
}

/// TRS-FR-HJZR, TRS-FR-QCOP: every line of the pane goes into the history.
fn keep_whole_pane<R: Runtime>(app: &AppHandle<R>, session: &Session, persistence: &Persistence) {
    match persistence.capture(false) {
        Ok(text) => session.push_output(&as_terminal_lines(&text)),
        Err(e) => warn(app, "terminal tmux call failed", &session.spec.id, e.cause()),
    }
}

/// TRS-FR-IEKU .. TRS-FR-TFAN: restores the sessions of `project_key` that
/// the registry does not hold, oldest first, while the project holds fewer
/// than 16. Returns how many it restored. `worktrees` is `None` where the
/// enumeration failed: then no target is known to be gone (TRS-FR-RKAW).
pub fn restore_with<R: Runtime>(
    app: &AppHandle<R>,
    tmux: Arc<Tmux>,
    project_key: &str,
    worktrees: Option<&[WorktreeFact]>,
    busy_of_worktree: &dyn Fn(&str) -> Option<String>,
) -> usize {
    let prefix = meta::project_prefix(project_key);
    let mut unreadable = 0;
    let mut listed: Vec<Listed> = match tmux.runner.run(&commands::list()) {
        Ok(text) => {
            let found = meta::parse_list(&text, project_key);
            let named = text.lines().filter(|l| l.starts_with(&prefix)).count();
            unreadable = named.saturating_sub(found.len());
            found
        }
        // No server runs: there is nothing to restore.
        Err(TmuxError::Status) => Vec::new(),
        Err(e) => {
            warn(app, "terminal restore failed", "", e.cause());
            return 0;
        }
    };
    listed.sort_by(|a, b| a.meta.created_at.cmp(&b.meta.created_at));
    let found = listed.len();
    let registry = registry(app);
    let mut restored = 0;
    let mut held = 0;
    for item in listed {
        if registry.contains(&item.terminal_id) {
            held += 1;
            continue;
        }
        if registry.len() >= MAX_SESSIONS {
            // TRS-FR-TFAN: the rest stay alive and unlisted.
            break;
        }
        let present = worktrees.is_none_or(|w| service::target_present(&item.meta.target, w));
        let (target_name, branch) = describe(&item.meta.target, worktrees);
        let busy_run_id = if present {
            service::target_busy(&item.meta.target, worktrees.unwrap_or_default(), busy_of_worktree)
        } else {
            None
        };
        let new = NewSession {
            target: item.meta.target.clone(),
            target_name,
            branch,
            kind: item.meta.kind,
            vendor: item.meta.vendor,
            runs_in: item.meta.runs_in,
            busy_run_id,
            initial_state: TerminalState::Running,
        };
        let session = match registry.restore(
            item.terminal_id.clone(),
            item.meta.label.clone(),
            item.meta.created_at.clone(),
            new,
        ) {
            Ok(Some(session)) => session,
            Ok(None) => continue,
            Err(_) => break,
        };
        session.set_pinned(item.meta.pinned);
        session.set_theme(item.meta.theme);
        let persistence = Arc::new(Persistence::new(tmux.clone(), item.name.clone()));
        session.set_persistence(persistence.clone());
        if !present {
            keep_whole_pane(app, &session, &persistence);
            session.target_gone();
            kill(app, &session);
        } else if let PaneState::Dead(code) = item.pane {
            keep_whole_pane(app, &session, &persistence);
            session.process_ended(code, EndReason::ProcessExited);
            kill(app, &session);
        } else {
            session.wait_for_size();
        }
        log_session(app, "terminal restored", &session);
        restored += 1;
    }
    if found > 0 || unreadable > 0 {
        // TRS-FR-TFAN, TRS-FR-LXIE: the counts say why a tmux session of the
        // project is not listed.
        crate::logging::log_info(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "terminal restore read",
            crate::log_fields! {
                "found" => found,
                "restored" => restored,
                "held_already" => held,
                "past_limit" => found - restored - held,
                "unreadable" => unreadable,
            },
        );
    }
    if restored > 0 {
        announce(app);
    }
    restored
}

/// TRS-FR-TCBS: the first size a tab asks for starts the first client of a
/// restored live session. The history gets the pane's lines above the visible
/// screen and one line feed for each row, and the client's redraw follows.
pub fn start_restored<R: Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    cols: u16,
    rows: u16,
) {
    let Some(persistence) = session.persistence() else { return };
    let id = session.spec.id.clone();
    match persistence.pane() {
        Ok(Some(PaneState::Alive)) => {}
        Ok(Some(PaneState::Dead(code))) => {
            keep_whole_pane(app, session, &persistence);
            session.process_ended(code, EndReason::ProcessExited);
            kill(app, session);
            log_session(app, "terminal ended", session);
            announce(app);
            return;
        }
        Ok(None) => {
            session.process_ended(None, EndReason::ProcessExited);
            log_session(app, "terminal ended", session);
            announce(app);
            return;
        }
        Err(e) => {
            warn(app, "terminal tmux call failed", &id, e.cause());
            session.process_ended(None, EndReason::ProcessExited);
            log_session(app, "terminal ended", session);
            announce(app);
            return;
        }
    }
    match persistence.capture(true) {
        Ok(text) => session.push_output(&as_terminal_lines(&text)),
        Err(e) => warn(app, "terminal tmux call failed", &id, e.cause()),
    }
    session.push_output(&b"\r\n".repeat(usize::from(rows)));
    match supervisor::supervise(persistence, cols, rows, warner(app, &id)) {
        Ok(handle) => service::run(app, session.clone(), handle),
        Err(_) => {
            warn(app, "terminal tmux call failed", &id, "tmux_attach_failed");
            session.process_ended(None, EndReason::ProcessExited);
            log_session(app, "terminal ended", session);
            announce(app);
        }
    }
}

#[cfg(test)]
#[path = "persistent_tests.rs"]
mod tests;
