//! GitHub polling — ready GitHub Tasks, and claiming one
//! (`../../specifications/core/GPP-github-polling.md`).
//!
//! The module polls the repository GitHub publication resolves (GPP-FR-RGNM),
//! keeps the open Tasks whose `Status` in one selected GitHub Project is
//! exactly `Ready`, and on a claim moves the issue to `In Progress`, records a
//! pending claim on this machine, and creates an immutable GitHub-shadow draft
//! that the graduation-start dialog then graduates.
//!
//! The rules live in pure submodules — [`eligibility`], [`session`],
//! [`claims`], and [`view`] — that a test drives against a fake client. This
//! file holds the Tauri commands: it resolves the project, the token, and the
//! repository, runs the network work off the window thread, and reports every
//! failure through the log. Every body is generic over the runtime, so a test
//! drives it through a mock application.
//!
//! Nothing here sets an issue to Done, closes an issue, or edits its title,
//! body, labels, or comments (GPP-FR-CRWY). The only write to GitHub is the
//! `Ready` to `In Progress` status update of a claim (GPP-FR-ILGG).

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::fs::RootFs;
use crate::github_tokens::{self, GithubTokens, ProjectToken};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Domain, Fields, LogBuffer};
use crate::project::ProjectState;
use crate::project_settings::{
    load_github_pending_claims_from, load_github_polling_settings_from,
    save_github_polling_settings_to, GITHUB_POLLING_INTERVALS,
};

pub mod claims;
pub mod client;
pub mod eligibility;
pub mod records;
pub mod session;
pub mod view;

pub use client::{GithubProjects, GithubProjectsSeam};
pub use records::*;

use eligibility::{PollFailure, PollSuccess};
use session::{ClaimGuards, PollStart, PollTicket, SessionKey, SessionSlot, Settled};

/// GPP-FR-DATH / GPP-FR-RAQP: the managed polling state.
pub struct GithubPollingState {
    slot: Mutex<SessionSlot>,
    claims: ClaimGuards,
    /// The log buffer this module reports into — the session buffer, or a
    /// test's own.
    buffer: &'static LogBuffer,
}

impl Default for GithubPollingState {
    fn default() -> Self {
        Self::with_buffer(&logging::BUFFER)
    }
}

impl GithubPollingState {
    pub fn with_buffer(buffer: &'static LogBuffer) -> Self {
        Self { slot: Mutex::new(SessionSlot::default()), claims: ClaimGuards::default(), buffer }
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, SessionSlot> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// GPP-FR-DATH: the project closed, so the polling session ends.
pub fn end_session<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<GithubPollingState>() {
        state.slot().end();
    }
}

fn state<R: Runtime>(app: &AppHandle<R>) -> tauri::State<'_, GithubPollingState> {
    app.state::<GithubPollingState>()
}

/// The open project's root, its session key, and its slot key.
struct Context {
    root: RootFs,
    key: SessionKey,
    project_key: String,
}

fn context<R: Runtime>(app: &AppHandle<R>) -> Result<Context, String> {
    let project = app.state::<ProjectState>();
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT.to_string())?;
    let project_key = project.slot_key();
    let key = SessionKey { project: project_key.clone(), worktree: root.path().to_path_buf() };
    Ok(Context { root, key, project_key })
}

/// GPP-FR-DATH / GPP-FR-ELWS: whether `key` is still the open project and
/// worktree. Work that started under another key changes no session.
fn is_live<R: Runtime>(app: &AppHandle<R>, key: &SessionKey) -> bool {
    context(app).is_ok_and(|ctx| &ctx.key == key)
}

/// A test's stand-in for the keychain-held token.
#[cfg(test)]
pub(crate) struct TestSecret(pub String);

/// GPP-FR-WOIM: the project's token and its host, or `None`. The secret is
/// passed to the client and never stored, returned, or logged.
fn project_token<R: Runtime>(app: &AppHandle<R>, project_key: &str) -> Option<ProjectToken> {
    #[cfg(test)]
    if let Some(secret) = app.try_state::<TestSecret>() {
        return Some(ProjectToken {
            secret: secret.0.clone(),
            host: github_tokens::DEFAULT_HOST.to_string(),
        });
    }
    let store = app.state::<GlobalSettingsStore>();
    let tokens = app.state::<GithubTokens>();
    github_tokens::resolve_project_token(&store, &tokens, project_key).ok()
}

fn require_token<R: Runtime>(app: &AppHandle<R>, project_key: &str) -> Result<ProjectToken, String> {
    project_token(app, project_key)
        .ok_or_else(|| crate::github_publication::ERR_TOKEN_UNAVAILABLE.to_string())
}

/// The Projects client for the GraphQL API of `host` (GPP-FR-HSTD).
fn projects_client<R: Runtime>(
    app: &AppHandle<R>,
    host: &str,
) -> std::sync::Arc<dyn GithubProjects> {
    let client = app.state::<GithubProjectsSeam>().0.clone();
    client.for_host(host).unwrap_or(client)
}

/// GPP-FR-RGNM: the polling repository, through GitHub publication.
fn resolve_repository<R: Runtime>(
    app: &AppHandle<R>,
    token: Option<&ProjectToken>,
) -> Result<RepositoryRef, String> {
    let resolved = crate::github_publication::resolve_publication_repository_with(app, token)?;
    Ok(RepositoryRef {
        host: resolved.repository_host,
        owner: resolved.repository_owner,
        name: resolved.repository_name,
    })
}

/// The repository the session already resolved, or a fresh resolution that
/// the session remembers only while it is still the live one.
fn session_repository<R: Runtime>(
    app: &AppHandle<R>,
    ctx: &Context,
    token: Option<&ProjectToken>,
) -> Result<RepositoryRef, String> {
    let known = state(app).slot().session_for(&ctx.key).and_then(|s| s.repository.clone());
    if let Some(repository) = known {
        return Ok(repository);
    }
    let repository = resolve_repository(app, token)?;
    state(app).slot().remember_repository(&ctx.key, &repository);
    Ok(repository)
}

/// GPP-FR-TYOV: announce a change of the view.
fn announce<R: Runtime>(app: &AppHandle<R>, new_issues: Vec<NewIssue>) {
    let _ = app.emit(GITHUB_POLLING_CHANGED, GithubPollingChanged { new_issues });
}

fn log_to<R: Runtime>(app: &AppHandle<R>, level: logging::LogLevel, message: &str, fields: Fields) {
    let buffer = state(app).buffer;
    logging::log(app, buffer, level, &[Domain::Backend, Domain::Remote], message, fields);
}

/// GPP-FR-WKZF / GPP-FR-PUXT: one log record per failure, naming the
/// operation, the code, and the issue where one applies. No token, header,
/// body, or credentialed URL is ever a field.
fn report<R: Runtime>(app: &AppHandle<R>, operation: &str, code: &str, issue: Option<u64>) -> String {
    let mut fields = log_fields! { "operation" => operation, "code" => code };
    if let Some(number) = issue {
        fields.insert("issue".to_string(), serde_json::json!(number));
    }
    log_to(app, logging::LogLevel::Warn, "github polling operation failed", fields);
    code.to_string()
}

/// GPP-FR-HZDD: an interval is unset or one of the five allowed values.
pub fn validate_interval(interval_minutes: Option<u32>) -> Result<(), String> {
    match interval_minutes {
        Some(minutes) if !GITHUB_POLLING_INTERVALS.contains(&minutes) => {
            Err(ERR_INVALID_INTERVAL.to_string())
        }
        _ => Ok(()),
    }
}

/// The pending claims on disk, and those held in memory that the disk does
/// not hold (GPP-FR-IGER).
fn all_pending_claims(root: &RootFs, held: Vec<GithubPendingClaim>) -> Vec<GithubPendingClaim> {
    let mut claims = load_github_pending_claims_from(root);
    for claim in held {
        let on_disk = claims
            .iter()
            .any(|c| c.names(
                &claim.repository_host,
                &claim.repository_owner,
                &claim.repository_name,
                claim.issue_number,
            ));
        if !on_disk {
            claims.push(claim);
        }
    }
    claims
}

/// "get github polling state": the view of the live project, from memory and
/// disk alone. It always reads the live key, so a caller holding an older
/// context can never reset the session back to it.
fn current_view<R: Runtime>(app: &AppHandle<R>) -> Result<GithubPollingView, String> {
    let ctx = context(app)?;
    let settings = load_github_polling_settings_from(&ctx.root)?;
    // GPP-FR-UBDE: read from the drafts listing and never cached.
    let mut drafts = crate::drafts::list_drafts_impl(&ctx.root).drafts;
    drafts.retain(|d| d.github_issue.is_some());
    crate::drafts::attach_graduation(app, &mut drafts);
    let shadows = view::shadow_rows(&drafts);
    let state = state(app);
    let mut slot = state.slot();
    slot.enter(&ctx.key);
    let pending = all_pending_claims(&ctx.root, slot.unsaved_claims(&ctx.key));
    Ok(view::build_view(&slot, &ctx.key, settings, pending, shadows))
}

/// Run one command's body off the window thread (GPP non-functional
/// requirements), on the pattern GitHub publication uses.
async fn off_thread<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| ERR_GITHUB_UNREACHABLE.to_string())?
}

// ---------------------------------------------------------------------------
// Listing and settings
// ---------------------------------------------------------------------------

/// GPP-FR-WYRP: the Projects the token can read.
#[tauri::command]
pub async fn list_github_projects(app: tauri::AppHandle) -> Result<Vec<GithubProjectOption>, String> {
    off_thread(move || list_projects_impl(&app)).await
}

pub(crate) fn list_projects_impl<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Vec<GithubProjectOption>, String> {
    let ctx = context(app)?;
    let token = require_token(app, &ctx.project_key)
        .map_err(|code| report(app, "list_projects", &code, None))?;
    let client = projects_client(app, &token.host);
    let viewer = client
        .viewer_projects(&token.secret)
        .map_err(|code| report(app, "list_projects", &code, None))?;
    let mut truncated = viewer.truncated;
    let mut projects = viewer.items;
    // The repository owner's Projects are listed where a repository resolves;
    // the viewer's own are listed either way.
    if let Ok(repository) = resolve_repository(app, Some(&token)) {
        match client.owner_projects(&token.secret, &repository.owner) {
            Ok(owned) => {
                truncated |= owned.truncated;
                projects.extend(owned.items);
            }
            Err(code) => {
                report(app, "list_owner_projects", &code, None);
            }
        }
    }
    if truncated {
        log_to(
            app,
            logging::LogLevel::Warn,
            "github project listing stopped at its page cap",
            log_fields! { "pages" => client::MAX_PROJECT_PAGES },
        );
    }
    Ok(client::dedupe_projects(projects))
}

/// "get github polling state": contacts no network.
#[tauri::command]
pub fn get_github_polling_state(app: tauri::AppHandle) -> Result<GithubPollingView, String> {
    current_view(&app)
}

/// GPP-FR-IURX / GPP-FR-HZDD: persist the settings, then validate a selected
/// Project at once.
#[tauri::command]
pub async fn set_github_polling_settings(
    project_node_id: Option<String>,
    interval_minutes: Option<u32>,
    app: tauri::AppHandle,
) -> Result<GithubPollingView, String> {
    off_thread(move || set_settings_impl(&app, project_node_id, interval_minutes)).await
}

pub(crate) fn set_settings_impl<R: Runtime>(
    app: &AppHandle<R>,
    project_node_id: Option<String>,
    interval_minutes: Option<u32>,
) -> Result<GithubPollingView, String> {
    let ctx = context(app)?;
    validate_interval(interval_minutes).map_err(|code| report(app, "set_settings", &code, None))?;
    let project_node_id = project_node_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    let settings = GithubPollingSettings { project_node_id: project_node_id.clone(), interval_minutes };
    save_github_polling_settings_to(&ctx.root, &settings)
        .map_err(|code| report(app, "set_settings", &code, None))?;
    let generation = {
        let state = state(app);
        let mut slot = state.slot();
        if is_live(app, &ctx.key) {
            slot.settings_changed(&ctx.key);
        }
        slot.generation()
    };
    log_to(
        app,
        logging::LogLevel::Info,
        "github polling settings saved",
        log_fields! {
            "project_selected" => project_node_id.is_some(),
            "interval_minutes" => interval_minutes,
        },
    );
    if let Some(project_id) = project_node_id.as_deref() {
        // GPP-FR-IURX: validated at once. A validation that cannot reach
        // GitHub leaves the configuration unchecked rather than invalid.
        let checked = require_token(app, &ctx.project_key).and_then(|token| {
            match eligibility::read_configuration(
                projects_client(app, &token.host).as_ref(),
                &token.secret,
                project_id,
            ) {
                Ok(valid) => Ok(GithubPollingConfiguration::valid(&valid.title)),
                Err(failure) if is_configuration_error(&failure.code) => {
                    Ok(GithubPollingConfiguration::invalid(&failure.code, failure.project_title))
                }
                Err(failure) => Err(failure.code),
            }
        });
        match checked {
            Ok(configuration) => {
                if let Some(code) = configuration.error_code.as_deref() {
                    report(app, "validate_configuration", code, None);
                }
                state(app).slot().record_configuration(&ctx.key, generation, project_id, configuration);
            }
            Err(code) => {
                report(app, "validate_configuration", &code, None);
            }
        }
    }
    announce(app, Vec::new());
    current_view(app)
}

// ---------------------------------------------------------------------------
// Polling
// ---------------------------------------------------------------------------

/// GPP-FR-XZTP: clears the in-flight mark of a poll that never settled — one
/// that panicked — when it goes out of scope. After a settlement it does
/// nothing.
struct InFlight<R: Runtime> {
    app: AppHandle<R>,
    key: SessionKey,
    token: u64,
}

impl<R: Runtime> Drop for InFlight<R> {
    fn drop(&mut self) {
        if let Some(state) = self.app.try_state::<GithubPollingState>() {
            state.slot().abandon(&self.key, self.token);
        }
    }
}

/// A poll that started, with what its settlement needs.
pub(crate) struct Begun<R: Runtime> {
    ctx: Context,
    ticket: PollTicket,
    _in_flight: InFlight<R>,
}

/// GPP-FR-XSKT / GPP-FR-XZTP: run one poll, or report the one in flight.
#[tauri::command]
pub async fn poll_github_ready_tasks(app: tauri::AppHandle) -> Result<GithubPollingView, String> {
    off_thread(move || poll_impl(&app)).await
}

pub(crate) fn poll_impl<R: Runtime>(app: &AppHandle<R>) -> Result<GithubPollingView, String> {
    let begun = match begin_poll(app)? {
        Ok(begun) => begun,
        Err(view) => return Ok(view),
    };
    let outcome = fetch_poll(app, &begun);
    finish_poll(app, begun, outcome)
}

/// Start a poll: `Ok(Err(view))` where one already runs (GPP-FR-XZTP).
pub(crate) fn begin_poll<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Result<Begun<R>, GithubPollingView>, String> {
    let ctx = context(app)?;
    // GPP-FR-ELWS: the generation is read before the settings, so a settings
    // write that races this start discards the result.
    let generation = {
        let state = state(app);
        let mut slot = state.slot();
        slot.enter(&ctx.key);
        slot.generation()
    };
    let settings = load_github_polling_settings_from(&ctx.root)
        .map_err(|code| report(app, "poll", &code, None))?;
    let start = {
        let state = state(app);
        let mut slot = state.slot();
        // GPP-FR-EWLZ / GPP-FR-ANDE: nothing selected, or an invalid
        // configuration, runs no poll.
        let project_id = slot.require_pollable(&ctx.key, &settings);
        project_id.map(|project_id| slot.begin_poll(&ctx.key, &project_id, generation))
    };
    let ticket = match start {
        Err(code) => return Err(report(app, "poll", &code, None)),
        Ok(PollStart::AlreadyRunning) => return current_view(app).map(Err),
        Ok(PollStart::Started(ticket)) => ticket,
    };
    let in_flight = InFlight { app: app.clone(), key: ctx.key.clone(), token: ticket.token };
    announce(app, Vec::new());
    Ok(Ok(Begun { ctx, ticket, _in_flight: in_flight }))
}

/// The network half of a poll.
pub(crate) fn fetch_poll<R: Runtime>(
    app: &AppHandle<R>,
    begun: &Begun<R>,
) -> Result<PollSuccess, PollFailure> {
    let token = require_token(app, &begun.ctx.project_key).map_err(PollFailure::code)?;
    let repository = resolve_repository(app, Some(&token)).map_err(PollFailure::code)?;
    eligibility::poll_once(
        &begun.ctx.root,
        projects_client(app, &token.host).as_ref(),
        &token.secret,
        &repository,
        &begun.ticket.project_id,
    )
}

/// Settle a poll. A poll whose project or worktree is no longer the live one
/// is discarded without touching the session (GPP-FR-ELWS).
pub(crate) fn finish_poll<R: Runtime>(
    app: &AppHandle<R>,
    begun: Begun<R>,
    outcome: Result<PollSuccess, PollFailure>,
) -> Result<GithubPollingView, String> {
    let truncated = outcome.as_ref().is_ok_and(|s| s.truncated);
    let settled = match is_live(app, &begun.ctx.key) {
        true => state(app).slot().settle_poll(&begun.ticket, outcome, &crate::notes::now_rfc3339()),
        false => Settled::Discarded,
    };
    drop(begun);
    match settled {
        Settled::Succeeded(new_issues) => {
            if truncated {
                log_to(
                    app,
                    logging::LogLevel::Warn,
                    "github project items stopped at their page cap",
                    log_fields! { "pages" => client::MAX_ITEM_PAGES },
                );
            }
            log_to(
                app,
                logging::LogLevel::Info,
                "github ready tasks polled",
                log_fields! { "new_issues" => new_issues.len() },
            );
            announce(app, new_issues);
        }
        Settled::Failed(code) => {
            report(app, "poll", &code, None);
            announce(app, Vec::new());
        }
        Settled::Discarded => {
            log_to(app, logging::LogLevel::Info, "github poll result discarded", Fields::new());
            announce(app, Vec::new());
        }
    }
    current_view(app)
}

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

/// After a claim or a retry: the new draft reaches the Drafts panel
/// (DRS-FR-22). Its creation event was raised when it was scaffolded
/// (PST-FR-DQZT), so the claim offers the committer its sink and nothing more.
fn settle_claim<R: Runtime>(app: &AppHandle<R>, _root: &RootFs, outcome: &claims::ClaimOutcome) {
    if outcome.created {
        crate::storage_floor::commit::offer_sink(app);
        crate::drafts::announce_drafts_changed(app, &outcome.result.draft_id);
    }
}

/// GPP-FR-IGER: a claim the disk did not take is held in memory, so Retry is
/// still offered, and it is reported at error level.
fn hold_unsaved<R: Runtime>(app: &AppHandle<R>, key: &SessionKey, claim: GithubPendingClaim) {
    log_to(
        app,
        logging::LogLevel::Error,
        "github pending claim held in memory only",
        log_fields! { "issue" => claim.issue_number, "has_draft" => claim.draft_id.is_some() },
    );
    state(app).slot().keep_unsaved(key, claim);
}

/// GPP-FR-IFVC through GPP-FR-DGWL: claim one ready task.
#[tauri::command]
pub async fn claim_github_task(
    issue_number: u64,
    app: tauri::AppHandle,
) -> Result<GithubClaimResult, String> {
    off_thread(move || claim_impl(&app, issue_number)).await
}

pub(crate) fn claim_impl<R: Runtime>(
    app: &AppHandle<R>,
    number: u64,
) -> Result<GithubClaimResult, String> {
    let fail = |code: &str| report(app, "claim", code, Some(number));
    let ctx = context(app)?;
    let settings = load_github_polling_settings_from(&ctx.root).map_err(|c| fail(&c))?;
    // GPP-FR-ANDE: claims refuse on the terms polls do.
    let project_id = state(app).slot().require_pollable(&ctx.key, &settings).map_err(|c| fail(&c))?;
    let token = require_token(app, &ctx.project_key).map_err(|c| fail(&c))?;
    let repository = session_repository(app, &ctx, Some(&token)).map_err(|c| fail(&c))?;
    let state = state(app);
    let _guard = state
        .claims
        .acquire(IssueKey::new(&repository.host, &repository.owner, &repository.name, number))
        .map_err(|c| fail(&c))?;
    let generation = state.slot().generation();
    let held = state.slot().unsaved_claims(&ctx.key);
    let claim_ctx = claims::ClaimContext {
        root: &ctx.root,
        secret: &token.secret,
        repository: &repository,
        project_id: &project_id,
    };
    let now = crate::notes::now_rfc3339();
    let result = match claims::claim(&claim_ctx, projects_client(app, &token.host).as_ref(), number, &now, &held) {
        Ok(outcome) => {
            settle_claim(app, &ctx.root, &outcome);
            if !outcome.saved {
                hold_unsaved(app, &ctx.key, outcome.claim.clone());
            }
            log_to(
                app,
                logging::LogLevel::Info,
                "github task claimed",
                log_fields! { "issue" => number, "draft_id" => &outcome.result.draft_id },
            );
            Ok(outcome.result)
        }
        Err(failure) => {
            if is_configuration_error(&failure.code) {
                state.slot().record_configuration(
                    &ctx.key,
                    generation,
                    &project_id,
                    GithubPollingConfiguration::invalid(&failure.code, failure.project_title),
                );
            }
            if let Some(claim) = failure.unsaved_claim {
                hold_unsaved(app, &ctx.key, claim);
            }
            Err(fail(&failure.code))
        }
    };
    announce(app, Vec::new());
    result
}

/// GPP-FR-TOED / GPP-FR-KSMZ: complete the local steps of a pending claim.
#[tauri::command]
pub async fn retry_github_claim(
    issue_number: u64,
    app: tauri::AppHandle,
) -> Result<GithubClaimResult, String> {
    off_thread(move || retry_impl(&app, issue_number)).await
}

pub(crate) fn retry_impl<R: Runtime>(
    app: &AppHandle<R>,
    number: u64,
) -> Result<GithubClaimResult, String> {
    let fail = |code: &str| report(app, "retry", code, Some(number));
    let ctx = context(app)?;
    // GPP-FR-ANDE: an invalid configuration refuses the retry too. A cleared
    // or changed selection does not: the claim names its own Project.
    let settings = load_github_polling_settings_from(&ctx.root).unwrap_or_default();
    if let Some(project_id) = settings.project_node_id.as_deref() {
        if state(app).slot().is_invalid(&ctx.key, project_id) {
            return Err(fail(ERR_CONFIGURATION_INVALID));
        }
    }
    let token = require_token(app, &ctx.project_key).map_err(|c| fail(&c))?;
    let repository = session_repository(app, &ctx, Some(&token)).map_err(|c| fail(&c))?;
    let state = state(app);
    let _guard = state
        .claims
        .acquire(IssueKey::new(&repository.host, &repository.owner, &repository.name, number))
        .map_err(|c| fail(&c))?;
    let held = state.slot().unsaved_claims(&ctx.key);
    let outcome = claims::retry(
        &ctx.root,
        &token.secret,
        &repository,
        projects_client(app, &token.host).as_ref(),
        number,
        &held,
    );
    let result = match outcome {
        Ok(outcome) => {
            settle_claim(app, &ctx.root, &outcome);
            match outcome.saved {
                true => {
                    state.slot().forget_unsaved(&ctx.key, &repository, number);
                }
                false => hold_unsaved(app, &ctx.key, outcome.claim.clone()),
            }
            log_to(
                app,
                logging::LogLevel::Info,
                "github claim retried",
                log_fields! { "issue" => number, "draft_id" => &outcome.result.draft_id },
            );
            Ok(outcome.result)
        }
        Err(code) => Err(fail(&code)),
    };
    announce(app, Vec::new());
    result
}

/// GPP-FR-BSLI: remove the pending claim of one issue.
#[tauri::command]
pub async fn acknowledge_github_claim(issue_number: u64, app: tauri::AppHandle) -> Result<(), String> {
    off_thread(move || acknowledge_impl(&app, issue_number)).await
}

pub(crate) fn acknowledge_impl<R: Runtime>(app: &AppHandle<R>, number: u64) -> Result<(), String> {
    let fail = |code: &str| report(app, "acknowledge", code, Some(number));
    let ctx = context(app)?;
    let token = project_token(app, &ctx.project_key);
    let repository = session_repository(app, &ctx, token.as_ref()).map_err(|c| fail(&c))?;
    let was_held = state(app).slot().forget_unsaved(&ctx.key, &repository, number);
    // A claim held only in memory has nothing on disk to remove, and a disk
    // that refused it may still refuse a write.
    let on_disk = claims::pending_claim(&ctx.root, &repository, number, &[]).is_some();
    if on_disk || !was_held {
        claims::acknowledge(&ctx.root, &repository, number).map_err(|c| fail(&c))?;
    }
    announce(app, Vec::new());
    Ok(())
}

/// GPP-FR-WOLE: open one listed issue in the OS default browser.
#[tauri::command]
pub async fn open_github_task_issue(issue_number: u64, app: tauri::AppHandle) -> Result<(), String> {
    off_thread(move || {
        let url = listed_issue_url_impl(&app, issue_number)?;
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(&url, None::<&str>)
            .map_err(|_| report(&app, "open_issue", ERR_GITHUB_UNREACHABLE, Some(issue_number)))
    })
    .await
}

fn listed_issue_url_impl<R: Runtime>(app: &AppHandle<R>, number: u64) -> Result<String, String> {
    let fail = |code: &str| report(app, "open_issue", code, Some(number));
    let ctx = context(app)?;
    let token = project_token(app, &ctx.project_key);
    let repository = session_repository(app, &ctx, token.as_ref()).map_err(|c| fail(&c))?;
    let (tasks, held) = {
        let state = state(app);
        let slot = state.slot();
        let tasks = slot.session_for(&ctx.key).map(|s| s.tasks.clone()).unwrap_or_default();
        (tasks, slot.unsaved_claims(&ctx.key))
    };
    let pending = all_pending_claims(&ctx.root, held);
    eligibility::listed_issue_url(number, &repository, &tasks, &pending).map_err(|c| fail(&c))
}

#[cfg(test)]
mod tests;
