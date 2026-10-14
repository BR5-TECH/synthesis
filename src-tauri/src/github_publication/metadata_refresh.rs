//! The background updater of the metadata cache (GHP-FR-IQTK through
//! GHP-FR-SIMK).
//!
//! The rules are pure functions of [`super::metadata_cache`]. This file is the
//! glue: it reads GitHub off the window thread, bounds each refresh with a
//! timeout, runs the 30-minute timer, emits the two events, and answers the
//! two commands of the publication chooser.

use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::oneshot;

use super::metadata_cache::*;
use super::records::*;
use super::{flow, issues_client, project_token, resolve_publication_repository_with};
use crate::log_fields;
use crate::logging::{self, Domain};
use crate::project::ProjectState;

/// GHP-FR-JIAU: the cache changed. Kebab-case, because Tauri rejects a space
/// in an event name at `emit`.
pub const PUBLICATION_METADATA_CHANGED: &str = "publication-metadata-changed";

/// GHP-FR-ISJI: the startup refresh failed.
pub const PUBLICATION_METADATA_STARTUP_FAILED: &str = "publication-metadata-startup-failed";

/// GHP-FR-RVCL: the time from the start of one refresh to the next.
pub const REFRESH_PERIOD: Duration = Duration::from_secs(30 * 60);

/// GHP-FR-MSLY: a refresh that does not end in this time fails.
pub const REFRESH_TIMEOUT: Duration = Duration::from_secs(120);

const MSG_REFRESH: &str = "publication metadata refresh";

type Waiter = oneshot::Sender<RefreshOutcome>;

/// GHP-FR-IQTK: the cache that every window of the application shares.
#[derive(Default)]
pub struct PublicationMetadataCache {
    slot: Mutex<CacheSlot<Waiter>>,
    /// GHP-FR-JIAU / GHP-FR-ROBR: held from a change of the cache to its
    /// event, so the events of the changes come in the order of the changes.
    /// It is taken before the slot lock, and never inside it.
    emit_lock: Mutex<()>,
}

impl PublicationMetadataCache {
    fn slot(&self) -> std::sync::MutexGuard<'_, CacheSlot<Waiter>> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn emit_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.emit_lock.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// GHP-FR-SIMK: the answer of `refresh_publication_metadata`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationMetadataRefresh {
    pub outcome: RefreshOutcomeKind,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub remote_name: Option<String>,
    pub metadata: Option<PublicationMetadata>,
}

/// GHP-FR-JIAU: the payload of `"publication metadata changed"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationMetadataCacheChanged {
    pub remote_name: Option<String>,
    pub metadata: Option<PublicationMetadata>,
}

/// GHP-FR-ISJI: the payload of `"publication metadata startup failed"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationMetadataStartupFailed {
    pub error_code: String,
    pub error: String,
}

/// GHP-FR-SIMK: the displayable text of a refresh failure. A fixed text for
/// each code, never a passed-through message (GHP-FR-DHXK).
pub fn refresh_error_text(code: &str) -> String {
    match code {
        ERR_PARENT_ISSUES_UNREADABLE | ERR_ISSUE_TYPES_UNREADABLE | ERR_MILESTONES_UNREADABLE => {
            metadata_error_text(code).to_string()
        }
        ERR_GITHUB_PROJECT_NOT_SELECTED => {
            "No GitHub Project is selected in Project settings.".to_string()
        }
        ERR_INVALID_PUBLICATION_SETTINGS => {
            "The GitHub publication settings of this project could not be read.".to_string()
        }
        ERR_NO_PROJECT => "No project is open.".to_string(),
        ERR_GITHUB_UNREACHABLE => "GitHub could not be reached.".to_string(),
        _ => super::remotes::refusal_reason(code),
    }
}

fn cache<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::State<'_, PublicationMetadataCache>> {
    app.try_state::<PublicationMetadataCache>()
}

/// The open project and its active worktree, or `None` where none is open.
fn current_key<R: Runtime>(app: &AppHandle<R>) -> Option<CacheKey> {
    let project = app.try_state::<ProjectState>()?;
    let root = project.require_root().ok()?;
    Some(CacheKey { project: project.slot_key(), worktree: root.path().to_path_buf() })
}

/// GHP-FR-HGVL: whether the polling settings state that no GitHub Project is
/// selected. Settings that cannot be read state nothing, so a refresh still
/// runs and fails with `invalid_publication_settings` (GHP-FR-ISJI).
fn no_github_project_selected<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(project) = app.try_state::<ProjectState>() else {
        return false;
    };
    let Ok(root) = project.require_root() else {
        return false;
    };
    crate::project_settings::load_github_polling_settings_from(&root)
        .is_ok_and(|settings| settings.project_node_id.is_none())
}

fn log<R: Runtime>(
    app: &AppHandle<R>,
    level: logging::LogLevel,
    message: &str,
    fields: logging::Fields,
) {
    logging::log(app, &logging::BUFFER, level, &[Domain::Backend, Domain::Remote], message, fields);
}

/// Empty the cache and tell every window. `clear` is the change of the slot:
/// a reset for a new key, or an empty cache for the same key. The waiters of a
/// refresh that it discards are dropped, so each of them answers `failed`
/// (GHP-FR-ROBR).
fn clear<R: Runtime>(
    app: &AppHandle<R>,
    cause: &str,
    clear: impl FnOnce(&mut CacheSlot<Waiter>) -> Vec<Waiter>,
) {
    let Some(state) = cache(app) else {
        return;
    };
    log(
        app,
        logging::LogLevel::Debug,
        "publication metadata cache emptied",
        log_fields! {
            "cause" => cause,
        },
    );
    let _order = state.emit_lock();
    let had_lists = {
        let mut slot = state.slot();
        let had = slot.lists().is_some();
        drop(clear(&mut slot));
        had
    };
    if had_lists {
        let _ = app.emit(
            PUBLICATION_METADATA_CHANGED,
            PublicationMetadataCacheChanged { remote_name: None, metadata: None },
        );
    }
}

/// GHP-FR-ROBR / GHP-FR-ISJI: empty the cache and a kept startup failure for
/// a new key.
fn reset<R: Runtime>(app: &AppHandle<R>, key: Option<CacheKey>, cause: &str) {
    clear(app, cause, |slot| slot.reset(key));
}

/// GHP-FR-HGVL: start a refresh where the run condition can hold. Otherwise
/// log the idle updater and arm the timer, so a later tick checks again.
fn start_if_selected<R: Runtime>(app: &AppHandle<R>, trigger: RefreshTrigger) {
    if no_github_project_selected(app) {
        log(
            app,
            logging::LogLevel::Debug,
            "publication metadata updater idle",
            log_fields! {
                "trigger" => trigger.as_str(),
                "reason" => ERR_GITHUB_PROJECT_NOT_SELECTED,
            },
        );
        arm_idle_timer(app);
    } else {
        start(app, trigger, None);
    }
}

/// GHP-FR-ISJI: a project opened. The cache and a kept startup failure start
/// empty, and one startup refresh runs where a GitHub Project is selected.
pub fn on_project_opened<R: Runtime>(app: &AppHandle<R>) {
    reset(app, current_key(app), "project_opened");
    start_if_selected(app, RefreshTrigger::Startup);
}

/// GHP-FR-ROBR / GHP-FR-JDMU: the active worktree changed. The cache empties
/// and one refresh starts at once where a GitHub Project is selected.
pub fn on_worktree_changed<R: Runtime>(app: &AppHandle<R>) {
    reset(app, current_key(app), "worktree_changed");
    start_if_selected(app, RefreshTrigger::Invalidation);
}

/// GHP-FR-ROBR: the project closed. The cache empties and the timer stops.
pub fn on_project_closed<R: Runtime>(app: &AppHandle<R>) {
    reset(app, None, "project_closed");
}

/// GHP-FR-JDMU: a value that the lists depend on changed. One refresh starts
/// at once and the timer restarts. A change that leaves no GitHub Project
/// selected empties the cache and runs no refresh (GHP-FR-HGVL).
pub fn invalidate<R: Runtime>(app: &AppHandle<R>, cause: &str) {
    if current_key(app).is_none() {
        return;
    }
    log(
        app,
        logging::LogLevel::Debug,
        "publication metadata invalidated",
        log_fields! {
            "cause" => cause,
        },
    );
    if no_github_project_selected(app) {
        clear(app, ERR_GITHUB_PROJECT_NOT_SELECTED, CacheSlot::empty);
    }
    start_if_selected(app, RefreshTrigger::Invalidation);
}

/// GHP-FR-JDMU: the project token changed (`"github tokens changed"`).
pub fn listen_for_token_changes<R: Runtime>(app: &AppHandle<R>) {
    use tauri::Listener as _;
    let handle = app.clone();
    app.listen_any(crate::github_tokens::GITHUB_TOKENS_CHANGED, move |_| {
        invalidate(&handle, "github_token");
    });
}

/// GHP-FR-AWEZ: start a refresh, or join the running one.
fn start<R: Runtime>(
    app: &AppHandle<R>,
    trigger: RefreshTrigger,
    waiter: Option<Waiter>,
) -> Option<RefreshStart> {
    let state = cache(app)?;
    let (start, epoch) = {
        let mut slot = state.slot();
        let start = slot.begin(trigger, waiter)?;
        let epoch = matches!(start, RefreshStart::Started(_)).then(|| slot.arm_timer());
        (start, epoch)
    };
    if let Some(epoch) = epoch {
        spawn_timer(app, epoch);
    }
    if let RefreshStart::Started(ticket) = &start {
        log(
            app,
            logging::LogLevel::Debug,
            "publication metadata refresh started",
            log_fields! {
                "trigger" => ticket.trigger.as_str(),
            },
        );
        spawn_refresh(app, ticket.clone());
    }
    Some(start)
}

/// GHP-FR-RVCL: the timer when no refresh starts, so a later tick still checks
/// the run condition.
fn arm_idle_timer<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = cache(app) else {
        return;
    };
    let epoch = state.slot().arm_timer();
    spawn_timer(app, epoch);
}

fn spawn_timer<R: Runtime>(app: &AppHandle<R>, epoch: u64) {
    tauri::async_runtime::spawn(periodic_tick(app.clone(), epoch));
}

/// GHP-FR-RVCL: one periodic refresh 30 minutes after `epoch` was armed,
/// unless a refresh or a reset re-armed the timer since then. A tick while no
/// GitHub Project is selected starts none and arms the timer again.
pub(super) async fn periodic_tick<R: Runtime>(app: AppHandle<R>, epoch: u64) {
    tokio::time::sleep(REFRESH_PERIOD).await;
    let current = cache(&app).is_some_and(|state| state.slot().is_current_timer(epoch));
    if current {
        start_if_selected(&app, RefreshTrigger::Periodic);
    }
}

/// GHP-FR-AWEZ: read the lists off the window thread, then settle.
fn spawn_refresh<R: Runtime>(app: &AppHandle<R>, ticket: RefreshTicket) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let reader = app.clone();
        let work = tauri::async_runtime::spawn_blocking(move || read_fresh(&reader));
        settle_within_timeout(&app, &ticket, work).await;
    });
}

/// GHP-FR-MSLY: wait for `work` at most [`REFRESH_TIMEOUT`], then settle. A
/// read that does not end in time, or whose task fails, settles as failed and
/// keeps every cached list.
pub(super) async fn settle_within_timeout<R: Runtime, E>(
    app: &AppHandle<R>,
    ticket: &RefreshTicket,
    work: impl Future<Output = Result<Result<CachedLists, String>, E>>,
) -> Option<RefreshOutcome> {
    let result = match tokio::time::timeout(REFRESH_TIMEOUT, work).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
            log(
                app,
                logging::LogLevel::Warn,
                "publication metadata refresh stopped",
                log_fields! {
                    "trigger" => ticket.trigger.as_str(),
                    "reason" => "read_task_failed",
                },
            );
            Err(ERR_GITHUB_UNREACHABLE.to_string())
        }
        Err(_) => {
            log(
                app,
                logging::LogLevel::Warn,
                "publication metadata refresh timed out",
                log_fields! {
                    "trigger" => ticket.trigger.as_str(),
                    "timeout_secs" => REFRESH_TIMEOUT.as_secs(),
                },
            );
            Err(ERR_GITHUB_UNREACHABLE.to_string())
        }
    };
    finish(app, ticket, result)
}

/// GHP-FR-HGVL / GHP-FR-AWEZ: one read of the three lists for the resolved
/// publication remote, through the read of GHP-FR-MDLD.
pub(super) fn read_fresh<R: Runtime>(app: &AppHandle<R>) -> Result<CachedLists, String> {
    let project = app.state::<ProjectState>();
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT.to_string())?;
    let polling = crate::project_settings::load_github_polling_settings_from(&root)
        .map_err(|_| ERR_INVALID_PUBLICATION_SETTINGS.to_string())?;
    if polling.project_node_id.is_none() {
        return Err(ERR_GITHUB_PROJECT_NOT_SELECTED.to_string());
    }
    let token = project_token(app, &project.slot_key());
    let repository = resolve_publication_repository_with(app, token.as_ref())?;
    let token = token.ok_or_else(|| ERR_TOKEN_UNAVAILABLE.to_string())?;
    let settings = crate::project_settings::load_github_publication_settings_from(&root)
        .map_err(|_| ERR_INVALID_PUBLICATION_SETTINGS.to_string())?;
    let metadata = flow::load_metadata(
        issues_client(app, &token.host).as_ref(),
        &token.secret,
        &repository.repository_owner,
        &repository.repository_name,
        settings,
    );
    Ok(CachedLists {
        remote_name: repository.remote_name,
        repository_host: repository.repository_host,
        repository_owner: repository.repository_owner,
        repository_name: repository.repository_name,
        parents: metadata.parents,
        issue_types: metadata.issue_types,
        milestones: metadata.milestones,
    })
}

/// GHP-FR-JIAU / GHP-FR-ISJI / GHP-FR-MSLY: settle one refresh, emit what is
/// due, log the outcome, and answer every waiter.
pub(super) fn finish<R: Runtime>(
    app: &AppHandle<R>,
    ticket: &RefreshTicket,
    result: Result<CachedLists, String>,
) -> Option<RefreshOutcome> {
    let state = cache(app)?;
    let settled = state.slot().settle(ticket, result);
    let Some((settlement, waiters)) = settled else {
        log(
            app,
            logging::LogLevel::Debug,
            "publication metadata refresh discarded",
            log_fields! {
                "trigger" => ticket.trigger.as_str(),
            },
        );
        return None;
    };
    if settlement.changed {
        // GHP-FR-ROBR: a reset after this settlement already told every window
        // that the cache is empty, so the lists of this refresh stay unsent.
        // The emit lock keeps a later reset from emitting before this event.
        let _order = state.emit_lock();
        let (remote_name, metadata) = cached_view(app);
        if state.slot().generation() == ticket.generation {
            let _ = app.emit(
                PUBLICATION_METADATA_CHANGED,
                PublicationMetadataCacheChanged { remote_name, metadata },
            );
        }
    }
    report(app, ticket, &settlement);
    for waiter in waiters {
        let _ = waiter.send(settlement.outcome.clone());
    }
    // GHP-FR-JDMU: a value changed while this refresh ran, so it may hold the
    // old lists. One more refresh reads them with the new values.
    if settlement.rerun {
        start(app, RefreshTrigger::Invalidation, None);
    }
    Some(settlement.outcome)
}

/// GHP-FR-MSLY / GHP-FR-HGVL / GHP-FR-ISJI: the log record of one settled
/// refresh, and the startup event where the startup refresh failed.
fn report<R: Runtime>(app: &AppHandle<R>, ticket: &RefreshTicket, settlement: &Settlement) {
    let trigger = ticket.trigger.as_str();
    let code = settlement.outcome.error_code.as_deref();
    match (settlement.outcome.kind, code) {
        // GHP-FR-HGVL: no GitHub Project selected is not a failure.
        (RefreshOutcomeKind::Failed, Some(ERR_GITHUB_PROJECT_NOT_SELECTED)) => {
            log(
                app,
                logging::LogLevel::Debug,
                "publication metadata updater idle",
                log_fields! {
                    "trigger" => trigger,
                    "reason" => ERR_GITHUB_PROJECT_NOT_SELECTED,
                    "cache_emptied" => settlement.changed,
                },
            );
        }
        (RefreshOutcomeKind::Failed, code) => {
            let code = code.unwrap_or(ERR_GITHUB_UNREACHABLE);
            log(
                app,
                logging::LogLevel::Warn,
                MSG_REFRESH,
                log_fields! {
                    "trigger" => trigger,
                    "outcome" => "failed",
                    "error_code" => code,
                    "cache_changed" => settlement.changed,
                },
            );
            // GHP-FR-KPXF: the settlement kept the failure before this event,
            // so a window that takes it when the event comes finds it.
            if ticket.trigger == RefreshTrigger::Startup {
                let _ = app.emit(
                    PUBLICATION_METADATA_STARTUP_FAILED,
                    PublicationMetadataStartupFailed {
                        error_code: code.to_string(),
                        error: refresh_error_text(code),
                    },
                );
            }
        }
        (kind, _) => {
            let level = if kind == RefreshOutcomeKind::Changed {
                logging::LogLevel::Info
            } else {
                logging::LogLevel::Debug
            };
            log(
                app,
                level,
                MSG_REFRESH,
                log_fields! {
                    "trigger" => trigger,
                    "outcome" => if kind == RefreshOutcomeKind::Changed { "changed" } else { "unchanged" },
                },
            );
        }
    }
}

/// The cached lists as the chooser renders them: with the current settings and
/// the Types they resolve to (GHP-FR-JTYL).
fn compose<R: Runtime>(app: &AppHandle<R>, lists: &CachedLists) -> PublicationMetadata {
    let settings = app
        .try_state::<ProjectState>()
        .and_then(|project| project.require_root().ok())
        .and_then(|root| crate::project_settings::load_github_publication_settings_from(&root).ok())
        .unwrap_or_default();
    PublicationMetadata {
        repository_owner: lists.repository_owner.clone(),
        repository_name: lists.repository_name.clone(),
        sub_issue_type: flow::resolve_sub_issue_type(&settings, &lists.issue_types),
        root_issue_type: flow::resolve_root_issue_type(&settings, &lists.issue_types),
        settings,
        parents: lists.parents.clone(),
        issue_types: lists.issue_types.clone(),
        milestones: lists.milestones.clone(),
    }
}

/// The cached remote and its composed metadata, or two `None` values.
fn cached_view<R: Runtime>(app: &AppHandle<R>) -> (Option<String>, Option<PublicationMetadata>) {
    let lists = cache(app).and_then(|state| state.slot().lists().cloned());
    match lists {
        Some(lists) => (Some(lists.remote_name.clone()), Some(compose(app, &lists))),
        None => (None, None),
    }
}

/// A test's access to the slot, so it can open the cache and settle a refresh
/// by hand.
#[cfg(test)]
pub(super) fn with_slot_for_test<R: Runtime, T>(
    app: &AppHandle<R>,
    f: impl FnOnce(&mut CacheSlot<Waiter>) -> T,
) -> T {
    let state = cache(app).expect("the cache is managed");
    let mut slot = state.slot();
    f(&mut slot)
}

/// A test's way to key the cache to the open project without a refresh.
#[cfg(test)]
pub(crate) fn open_for_test<R: Runtime>(app: &AppHandle<R>) {
    reset(app, current_key(app), "test");
}

/// A test's view of the slot: the refresh count, the last trigger, and the
/// timer epoch.
#[cfg(test)]
pub(crate) fn starts_for_test<R: Runtime>(
    app: &AppHandle<R>,
) -> (u64, Option<RefreshTrigger>, u64) {
    let state = cache(app).expect("the cache is managed");
    let slot = state.slot();
    (slot.starts(), slot.last_trigger(), slot.timer_epoch())
}

/// GHP-FR-JTYL: the cached lists for `remote_name`, or `None`.
pub(super) fn cached_metadata_for<R: Runtime>(
    app: &AppHandle<R>,
    remote_name: &str,
) -> Option<PublicationMetadata> {
    let lists = cache(app)?.slot().lists().filter(|l| l.remote_name == remote_name).cloned()?;
    Some(compose(app, &lists))
}

/// GHP-FR-JTYL: the cached lists of `remote_name`. Contacts no network.
#[tauri::command]
pub fn get_publication_metadata_cache(
    remote_name: String,
    app: AppHandle,
) -> Result<Option<PublicationMetadata>, String> {
    if current_key(&app).is_none() {
        return Err(ERR_NO_PROJECT.to_string());
    }
    let metadata = cached_metadata_for(&app, &remote_name);
    log(
        &app,
        logging::LogLevel::Debug,
        "publication metadata cache read",
        log_fields! {
            "hit" => metadata.is_some(),
        },
    );
    Ok(metadata)
}

/// GHP-FR-KPXF: the kept startup failure, which this call discards. Contacts
/// no network.
#[tauri::command]
pub fn take_publication_metadata_startup_failure(
    app: AppHandle,
) -> Option<PublicationMetadataStartupFailed> {
    take_startup_failure(&app)
}

pub(super) fn take_startup_failure<R: Runtime>(
    app: &AppHandle<R>,
) -> Option<PublicationMetadataStartupFailed> {
    let failure = cache(app)?.slot().take_startup_failure().map(|code| {
        PublicationMetadataStartupFailed { error: refresh_error_text(&code), error_code: code }
    });
    log(
        app,
        logging::LogLevel::Debug,
        "publication metadata startup failure taken",
        log_fields! {
            "found" => failure.is_some(),
        },
    );
    failure
}

/// GHP-FR-SIMK: start a refresh, or join the running one, and answer when it
/// ends. It runs in the background and blocks no other command.
#[tauri::command]
pub async fn refresh_publication_metadata(
    app: AppHandle,
) -> Result<PublicationMetadataRefresh, String> {
    refresh_and_wait(&app).await
}

/// How many times a request joins a new refresh after a reset discarded the
/// one it waited on.
const REQUEST_ATTEMPTS: usize = 3;

/// GHP-FR-SIMK: the outcome of the refresh that a request starts or joins. A
/// reset that discards that refresh (a worktree change, or a change that
/// leaves no GitHub Project selected) drops the waiter. The request then
/// answers for the new state, or joins the refresh of the new worktree.
async fn await_refresh<R: Runtime>(app: &AppHandle<R>) -> RefreshOutcome {
    for _ in 0..REQUEST_ATTEMPTS {
        if current_key(app).is_none() {
            return RefreshOutcome::failed(ERR_NO_PROJECT);
        }
        if no_github_project_selected(app) {
            return RefreshOutcome::failed(ERR_GITHUB_PROJECT_NOT_SELECTED);
        }
        let (sender, receiver) = oneshot::channel();
        if start(app, RefreshTrigger::Request, Some(sender)).is_none() {
            return RefreshOutcome::failed(ERR_NO_PROJECT);
        }
        if let Ok(outcome) = receiver.await {
            return outcome;
        }
        log(
            app,
            logging::LogLevel::Debug,
            "publication metadata request superseded",
            log_fields! {
                "reason" => "cache_reset",
            },
        );
    }
    RefreshOutcome::failed(ERR_GITHUB_UNREACHABLE)
}

pub(super) async fn refresh_and_wait<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<PublicationMetadataRefresh, String> {
    if current_key(app).is_none() {
        return Err(ERR_NO_PROJECT.to_string());
    }
    let outcome = await_refresh(app).await;
    let (remote_name, metadata) = cached_view(app);
    Ok(PublicationMetadataRefresh {
        outcome: outcome.kind,
        error: outcome.error_code.as_deref().map(refresh_error_text),
        error_code: outcome.error_code,
        remote_name,
        metadata,
    })
}
