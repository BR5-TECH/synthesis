//! `GDP-github-delivery-plan.md`: the delivery plan whose source is a GitHub
//! Project.
//!
//! GitHub is the plan's one source of truth. A read takes the selected
//! Project's issues, their sub-issue hierarchy, milestones, blocked-by
//! relations, and `Status` values through the tracker adapter of
//! [`adapter`], stores what it read as the read cache of [`cache`], and builds
//! the plan with [`plan`]. An edit of [`edits`] goes to GitHub at once, reads
//! the changed items again, and updates the cache. The cache is never written
//! back to GitHub.
//!
//! No log record of this module carries a token, a title, a body, a Type name,
//! or a URL query (GDP-FR-QHQR).

pub mod adapter;
pub mod cache;
pub mod edits;
pub mod http;
pub mod plan;
pub mod plan_reads;

use crate::project::OpenProject as _;
use crate::project_scope::events::EmitToProject;
use crate::project_scope::ProjectStores;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::delivery::machine_state::read_delivery_overlay;
use crate::delivery::settings::DeliverySettings;
use crate::delivery::source::{self, Resolved, ERR_NOT_GITHUB_SOURCE};
use crate::delivery::DeliveryError;
use crate::flow_delivery::ItemDeliveryState;
use crate::flow_overlap::Overlap;
use crate::flow_validation::model::FlowDocument;
use crate::github_polling::client::ProjectShape;
use crate::github_polling::delivery_access;
use crate::github_polling::eligibility::{validate_shape, StatusNames};
use crate::github_polling::records::{
    configuration_error_text, error_text, is_configuration_error, ConfigurationState,
    GithubPollingConfiguration, ERR_CONFIGURATION_INVALID, ERR_UNCONFIGURED,
};
use crate::logging::{self, Domain, Fields, LogBuffer, LogLevel};

use adapter::{TrackerAdapter, TrackerRepository, TrackerSeam};
use cache::TrackerRead;
use plan::{build_plan, Notices, PlanTrackerFacts};

/// GDP-FR-GBDF.
pub const ERR_CHANGED_ON_TRACKER: &str = "changed_on_tracker";
/// GDP-FR-CNVO.
pub const ERR_ISSUE_TYPE_UNAVAILABLE: &str = "issue_type_unavailable";
/// GDP-FR-EJTS, GDP-FR-NJJZ.
pub const ERR_KIND_CHANGE_UNSUPPORTED: &str = "kind_change_unsupported";
/// GDP-FR-YGXA.
pub const ERR_MILESTONE_EDGE_UNSUPPORTED: &str = "milestone_edge_unsupported";
/// GDP-FR-ICDW.
pub const ERR_LABEL_UNKNOWN: &str = "label_unknown";
/// GDP-FR-KOWL.
pub const ERR_STATUS_UNKNOWN: &str = "status_unknown";
/// GDP-FR-MVAA.
pub const ERR_NO_CHANGE: &str = "no_change";
/// GDP-FR-HULQ.
pub const ERR_TRACKER_ITEM_NOT_FOUND: &str = "tracker_item_not_found";
pub const ERR_ALREADY_IN_PLAN: &str = "already_in_plan";
pub const ERR_TYPE_UNMAPPED: &str = "type_unmapped";
/// GDP-FR-MSVS.
pub const ERR_PARTIAL_EDIT: &str = "partial_edit";
/// GDP-FR-PIAQ.
pub const ERR_TRACKER_URL_REFUSED: &str = "tracker_url_refused";

/// GDP-FR-URZY: the event each change of a read cache emits. Kebab-case,
/// because Tauri refuses a space in an event name at `emit`.
pub const GITHUB_DELIVERY_PLAN_CHANGED: &str = "github-delivery-plan-changed";

/// GDP-FR-URZY: the event payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanChanged {
    pub project_node_id: String,
}

/// GDP contract surface `GithubDeliveryPlan`, in the frontend shape of
/// `src/types/delivery.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubDeliveryPlan {
    pub document: FlowDocument,
    pub tracker: Vec<PlanTrackerFacts>,
    pub states: Vec<ItemDeliveryState>,
    pub overlaps: Vec<Overlap>,
    pub notices: Notices,
    pub read_at: String,
    pub stale: bool,
    pub error: Option<DeliveryError>,
}

/// GDP-FR-IRDD: edits run one at a time, in the order they arrive. Each caller
/// takes a ticket, and the tickets are served in order.
#[derive(Default)]
pub struct EditQueue {
    next: AtomicU64,
    serving: Mutex<u64>,
    turn: Condvar,
}

/// The turn of one edit. The next ticket is served when it is dropped.
pub struct EditTurn<'a>(&'a EditQueue);

impl EditQueue {
    pub fn enter(&self) -> EditTurn<'_> {
        let ticket = self.next.fetch_add(1, Ordering::SeqCst);
        let mut serving = self.serving.lock().unwrap_or_else(|e| e.into_inner());
        while *serving != ticket {
            serving = self.turn.wait(serving).unwrap_or_else(|e| e.into_inner());
        }
        EditTurn(self)
    }
}

impl Drop for EditTurn<'_> {
    fn drop(&mut self) {
        let mut serving = self.0.serving.lock().unwrap_or_else(|e| e.into_inner());
        *serving += 1;
        self.0.turn.notify_all();
    }
}

/// The last GitHub read that ended, by its key.
#[derive(Default)]
struct ReadLog {
    completed: u64,
    last: Option<(String, Result<TrackerRead, DeliveryError>)>,
}

/// GDP-FR-RJFS, GDP-FR-IRDD: the managed state of the GitHub plan.
pub struct GithubDeliveryState {
    read_lock: Mutex<()>,
    /// The loads that reached the read lock, so a test can see a load wait.
    entered: std::sync::atomic::AtomicUsize,
    reads: Mutex<ReadLog>,
    /// GDP-FR-RJFS, GDP-FR-YOSF, GDP-FR-IRDD: the generation of the read
    /// cache. Each write of a read cache holds this lock, and each edit write
    /// increments the generation. A GitHub read keeps the generation at its
    /// start. When the generation changed before the read writes, an edit
    /// wrote the cache while the read ran, and the read's data can be older
    /// than that write. Then the read does not write, and it answers the
    /// newer cache.
    cache_lock: Mutex<u64>,
    pub edits: EditQueue,
    buffer: &'static LogBuffer,
}

impl Default for GithubDeliveryState {
    fn default() -> Self {
        Self::with_buffer(&logging::BUFFER)
    }
}

impl GithubDeliveryState {
    pub fn with_buffer(buffer: &'static LogBuffer) -> Self {
        Self {
            read_lock: Mutex::new(()),
            entered: std::sync::atomic::AtomicUsize::new(0),
            reads: Mutex::new(ReadLog::default()),
            cache_lock: Mutex::new(0),
            edits: EditQueue::default(),
            buffer,
        }
    }

    pub(crate) fn lock_cache(&self) -> std::sync::MutexGuard<'_, u64> {
        self.cache_lock.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The log buffer of the delivery modules: the session buffer, or a test's
/// own.
pub(crate) fn buffer_of<R: Runtime>(app: &AppHandle<R>) -> &'static LogBuffer {
    app.try_project_store::<GithubDeliveryState>().map(|s| s.buffer).unwrap_or(&logging::BUFFER)
}

/// GDP-FR-QHQR: one record of this module.
pub(crate) fn log<R: Runtime>(app: &AppHandle<R>, level: LogLevel, message: &str, fields: Fields) {
    logging::log(app, buffer_of(app), level, &[Domain::Backend, Domain::Remote], message, fields);
}

/// The typed error of a GitHub code of `GPP-github-polling.md` (GPP-FR-SUFH)
/// or of GitHub publication.
pub fn github_error(code: &str) -> DeliveryError {
    DeliveryError::new(code, error_text(code))
}

/// The typed error of a configuration that is not valid (GPP-FR-EHRC).
fn configuration_error(configuration: &GithubPollingConfiguration) -> DeliveryError {
    let code = configuration.error_code.clone().unwrap_or_else(|| ERR_CONFIGURATION_INVALID.into());
    let message = configuration.error.clone().unwrap_or_else(|| error_text(&code));
    DeliveryError::new(&code, message)
}

/// What the plan of one Project is read with.
#[derive(Clone, Debug)]
pub struct GithubTarget {
    pub project_node_id: String,
    pub settings: DeliverySettings,
    pub statuses: StatusNames,
}

/// GDP-FR-QSHX: the target of a plan whose source need not be `github`, for
/// a publish (DSR-FR-RCLA). No selected Project is `polling_unconfigured`, and
/// a configuration known to be invalid is its error.
pub(crate) fn target_of_selected<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<GithubTarget, DeliveryError> {
    let root = source::project_root(app)?;
    let settings = source::load_settings(&root)?;
    let polling = delivery_access::polling_settings(app).map_err(|e| github_error(&e))?;
    let selected = polling.project_node_id.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let Some(project_node_id) = selected.map(str::to_string) else {
        return Err(github_error(ERR_UNCONFIGURED));
    };
    let configuration = delivery_access::configuration(app).map_err(|e| github_error(&e))?;
    if configuration.state == ConfigurationState::Invalid {
        return Err(configuration_error(&configuration));
    }
    Ok(GithubTarget { project_node_id, settings, statuses: StatusNames::of(&polling) })
}

/// GDP-FR-QSHX, GDP-FR-ZFQH: the target of the delivery plan, or the typed
/// refusal.
pub(crate) fn delivery_target<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<GithubTarget, DeliveryError> {
    let root = source::project_root(app)?;
    let (_, resolved) = source::resolve_local(&root)?;
    if !matches!(resolved, Resolved::Github { .. }) {
        return Err(DeliveryError::new(
            ERR_NOT_GITHUB_SOURCE,
            "The delivery source of the project is not a GitHub Project.",
        ));
    }
    target_of_selected(app)
}

/// GDP-FR-ZGJB: the tracker adapter of the polling repository, with the
/// project's token. The secret goes to the host of its own token alone.
pub(crate) fn connect<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<dyn TrackerAdapter>, DeliveryError> {
    let token = delivery_access::project_token(app).map_err(|e| github_error(&e))?;
    let repository =
        delivery_access::polling_repository(app, Some(&token)).map_err(|e| github_error(&e))?;
    let host = crate::github_tokens::host::stored_host;
    if host(&token.host) != host(&repository.host) {
        return Err(github_error(crate::github_tokens::host::ERR_HOST_MISMATCH));
    }
    let seam = app.state::<TrackerSeam>();
    let bound =
        TrackerRepository { host: repository.host, owner: repository.owner, name: repository.name };
    Ok(seam.0.connect(&bound, &token.secret))
}

pub(crate) fn machine_store<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<crate::fs::RootFs, DeliveryError> {
    let project = app.open_project_state().ok_or_else(DeliveryError::no_project)?;
    crate::delivery::machine_store(&project)
}

/// GDP-FR-URZY: announce a change of the read cache.
pub(crate) fn announce<R: Runtime>(app: &AppHandle<R>, project_node_id: &str) {
    let _ = app.emit_project(
        GITHUB_DELIVERY_PLAN_CHANGED,
        PlanChanged { project_node_id: project_node_id.to_string() },
    );
}

/// GDP-FR-YOSF, GDP-FR-URZY, GDP-FR-IRDD: replace the read cache with the
/// result of an edit and announce it. The write adds 1 to the cache
/// generation, so a GitHub read that began before it does not replace it.
pub(crate) fn store_edit<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    read: &TrackerRead,
) -> Result<(), DeliveryError> {
    let store = machine_store(app)?;
    {
        let state = app.project_store::<GithubDeliveryState>();
        let mut generation = state.lock_cache();
        cache::write_cache(&store, &target.project_node_id, read)?;
        *generation += 1;
    }
    announce(app, &target.project_node_id);
    Ok(())
}

/// GDP-FR-YOSF, GDP-FR-URZY, GDP-FR-RJFS: replace the read cache with a
/// GitHub read that began at cache generation `began`, and announce it. When
/// an edit wrote the cache after the read began, the read does not write, and
/// the answer is the cache, which is newer. The answer is the read itself only
/// when no cache is found.
fn store_github_read<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    read: TrackerRead,
    began: u64,
) -> Result<TrackerRead, DeliveryError> {
    let store = machine_store(app)?;
    let state = app.project_store::<GithubDeliveryState>();
    let generation = state.lock_cache();
    if *generation != began {
        let newer = cache::read_cache(app, buffer_of(app), &store, &target.project_node_id)?;
        drop(generation);
        log(
            app,
            LogLevel::Info,
            "github delivery plan read older than an edit",
            crate::log_fields! { "cache_found" => newer.is_some() },
        );
        return Ok(newer.unwrap_or(read));
    }
    cache::write_cache(&store, &target.project_node_id, &read)?;
    drop(generation);
    announce(app, &target.project_node_id);
    Ok(read)
}

/// GDP-FR-BGIT, GDP-FR-MYLM, GPP-FR-DEZO: one GitHub read of the Project. The
/// read checks the configuration on the terms of GPP-FR-DEZO and records what
/// it found.
pub(crate) fn github_read<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    adapter: &dyn TrackerAdapter,
) -> Result<TrackerRead, DeliveryError> {
    let generation = delivery_access::generation(app);
    let record = |configuration| {
        delivery_access::record_configuration(
            app,
            generation,
            &target.project_node_id,
            configuration,
        )
    };
    let project = adapter.read_project(&target.project_node_id).map_err(|code| {
        if is_configuration_error(&code) {
            record(GithubPollingConfiguration::invalid(&code, None, None));
        }
        github_error(&code)
    })?;
    let shape = ProjectShape {
        title: project.title.clone(),
        status_field: project.status_field.as_ref().map(|f| f.as_polling()),
    };
    match validate_shape(&shape, &target.statuses) {
        Ok(valid) => record(GithubPollingConfiguration::valid(&valid.title)),
        Err(failure) => {
            let configuration = GithubPollingConfiguration::invalid(
                &failure.code,
                failure.project_title,
                failure.missing_status,
            );
            let error = DeliveryError::new(
                &failure.code,
                configuration_error_text(
                    &failure.code,
                    configuration.missing_status.as_deref(),
                    configuration.project_title.as_deref(),
                ),
            );
            record(configuration);
            return Err(error);
        }
    }
    let milestones = adapter.read_milestones().map_err(|code| github_error(&code))?;
    Ok(TrackerRead {
        read_at: crate::notes::now_rfc3339(),
        repository: adapter.repository().clone(),
        project,
        milestones,
    })
}

/// GDP-FR-RJFS: one GitHub read at a time for the open project. A load that
/// asks for a read while one runs waits for it and takes its answer.
pub(crate) fn shared_read<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
) -> Result<TrackerRead, DeliveryError> {
    let state = app.project_store::<GithubDeliveryState>();
    let key = format!(
        "{}\u{0}{}",
        app.open_project_state_or_default().slot_key(),
        target.project_node_id
    );
    let seen = state.reads.lock().unwrap_or_else(|e| e.into_inner()).completed;
    state.entered.fetch_add(1, Ordering::SeqCst);
    let _reading = state.read_lock.lock().unwrap_or_else(|e| e.into_inner());
    {
        let reads = state.reads.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((last_key, result)) = reads.last.as_ref() {
            if reads.completed > seen && *last_key == key {
                return result.clone();
            }
        }
    }
    let started = Instant::now();
    let began = *state.lock_cache();
    let result = connect(app)
        .and_then(|adapter| github_read(app, target, adapter.as_ref()))
        .and_then(|read| store_github_read(app, target, read, began));
    match &result {
        Ok(read) => log(
            app,
            LogLevel::Info,
            "github delivery plan read",
            crate::log_fields! {
                "project_items" => read.project.items.len(),
                "milestones" => read.milestones.len(),
                "ms" => started.elapsed().as_millis() as u64,
            },
        ),
        Err(e) => log(
            app,
            LogLevel::Warn,
            "github delivery plan read failed",
            crate::log_fields! { "code" => &e.code, "ms" => started.elapsed().as_millis() as u64 },
        ),
    }
    let mut reads = state.reads.lock().unwrap_or_else(|e| e.into_inner());
    reads.completed += 1;
    reads.last = Some((key, result.clone()));
    result
}

/// GDP-FR-HNEW, GDP-FR-XWES, GDP-FR-CRRO: the answer for one read. The
/// overlay and the states are read now, so an answer from the cache shows the
/// current drafts and runs (GDP-FR-LKXA).
pub(crate) fn answer<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    read: &TrackerRead,
    stale: bool,
    error: Option<DeliveryError>,
) -> Result<GithubDeliveryPlan, DeliveryError> {
    let store = machine_store(app)?;
    let overlay = read_delivery_overlay(app, buffer_of(app), &store, &target.project_node_id)?;
    let built = build_plan(read, &target.settings, &overlay, &target.statuses);
    let root = source::project_root(app)?;
    let states =
        crate::flow_delivery::compute_delivery_state(app, &root, &built.document, &built.facts);
    Ok(GithubDeliveryPlan {
        overlaps: crate::flow_overlap::overlaps(&built.document),
        document: built.document,
        tracker: built.tracker,
        states: states.items,
        notices: built.notices,
        read_at: read.read_at.clone(),
        stale,
        error,
    })
}

/// The read cache of the target, or `None`.
pub(crate) fn cached<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
) -> Result<Option<TrackerRead>, DeliveryError> {
    let store = machine_store(app)?;
    cache::read_cache(app, buffer_of(app), &store, &target.project_node_id)
}

/// GDP-FR-LKXA, GDP-FR-GCJO, GDP-FR-WEFV: the plan of the target.
pub(crate) fn load_target<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    refresh: bool,
) -> Result<GithubDeliveryPlan, DeliveryError> {
    if !refresh {
        if let Some(read) = cached(app, target)? {
            return answer(app, target, &read, false, None);
        }
    }
    match shared_read(app, target) {
        Ok(read) => answer(app, target, &read, false, None),
        // GDP-FR-QSHX: a read that finds the configuration not valid refuses
        // with its error, as a later load does, and gives no stale plan.
        Err(error) if is_configuration_error(&error.code) => Err(error),
        Err(error) => match cached(app, target)? {
            Some(read) => answer(app, target, &read, true, Some(error)),
            None => Err(error),
        },
    }
}

/// GDP-FR-QSHX, GDP-FR-QHQR: `load_github_delivery_plan(refresh)` on the
/// calling thread.
pub fn load_plan_impl<R: Runtime>(
    app: &AppHandle<R>,
    refresh: bool,
) -> Result<GithubDeliveryPlan, DeliveryError> {
    let started = Instant::now();
    log(
        app,
        LogLevel::Debug,
        "github delivery plan load started",
        crate::log_fields! { "refresh" => refresh },
    );
    let result = delivery_target(app).and_then(|target| load_target(app, &target, refresh));
    match &result {
        Ok(plan) => log(
            app,
            LogLevel::Info,
            "github delivery plan loaded",
            crate::log_fields! {
                "refresh" => refresh,
                "items" => plan.document.items.len(),
                "edges" => plan.document.edges.len(),
                "skipped" => plan.notices.skipped,
                "stale" => plan.stale,
                "ms" => started.elapsed().as_millis() as u64,
            },
        ),
        Err(e) => log(
            app,
            LogLevel::Warn,
            "github delivery plan load refused",
            crate::log_fields! { "code" => &e.code, "ms" => started.elapsed().as_millis() as u64 },
        ),
    }
    result
}

/// FDL-FR-JLUJ: the plan of the read cache of the delivery source, where the
/// source is `github`, with its tracker facts. It makes no request, and it
/// answers `None` where no cache exists.
pub(crate) fn cached_delivery_plan<R: Runtime>(
    app: &AppHandle<R>,
    root: &crate::fs::RootFs,
) -> Option<plan::BuiltPlan> {
    let (settings, resolved) = source::resolve_local(root).ok()?;
    let Resolved::Github { project_node_id } = resolved else { return None };
    let polling = crate::project_settings::load_github_polling_settings_from(root).ok()?;
    let store = machine_store(app).ok()?;
    let read = cache::read_cache(app, buffer_of(app), &store, &project_node_id).ok()??;
    let overlay =
        read_delivery_overlay(app, buffer_of(app), &store, &project_node_id).unwrap_or_default();
    Some(build_plan(&read, &settings, &overlay, &StatusNames::of(&polling)))
}

/// GDP-FR-PIAQ: whether `url` is an HTTPS issue or milestone address of the
/// polling repository. A query, a fragment, a credential, and a backslash are
/// refused. Pure.
pub fn is_tracker_url(url: &str, host: &str, owner: &str, repo: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    if rest.contains(['?', '#', '@', '\\']) || rest.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let mut parts = rest.split('/');
    let (Some(h), Some(o), Some(r), Some(kind), Some(number), None) =
        (parts.next(), parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    h.eq_ignore_ascii_case(host)
        && o.eq_ignore_ascii_case(owner)
        && r.eq_ignore_ascii_case(repo)
        && matches!(kind, "issues" | "milestone")
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
        && !number.starts_with('0')
}

/// GDP-FR-PIAQ: whether `url` is the address of the selected Project. It must
/// be the same text as the stored Project URL (`stored`), an HTTPS address on
/// the host of the polling repository, with no query, fragment, credential,
/// backslash, or space. An empty stored URL matches nothing. Pure.
pub fn is_project_url(url: &str, host: &str, stored: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    if stored.is_empty() || url != stored {
        return false;
    }
    if rest.contains(['?', '#', '@', '\\']) || rest.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let (h, path) = rest.split_once('/').unwrap_or((rest, ""));
    h.eq_ignore_ascii_case(host) && !path.is_empty()
}

/// GDP-FR-PIAQ: the stored URL of the selected Project, from its read cache,
/// or empty where no read is cached.
fn selected_project_url<R: Runtime>(app: &AppHandle<R>) -> String {
    let selected = delivery_access::polling_settings(app)
        .ok()
        .and_then(|p| p.project_node_id)
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let read = selected.and_then(|node| {
        let store = machine_store(app).ok()?;
        cache::read_cache(app, buffer_of(app), &store, &node).ok().flatten()
    });
    read.map(|r| r.project.url).unwrap_or_default()
}

/// GDP-FR-PIAQ: the URL to open, or `tracker_url_refused`.
pub fn checked_tracker_url<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
) -> Result<String, DeliveryError> {
    source::project_root(app)?;
    let token = delivery_access::project_token(app).ok();
    let repository =
        delivery_access::polling_repository(app, token.as_ref()).map_err(|e| github_error(&e))?;
    let tracker = is_tracker_url(url, &repository.host, &repository.owner, &repository.name);
    if !tracker && !is_project_url(url, &repository.host, &selected_project_url(app)) {
        return Err(DeliveryError::new(
            ERR_TRACKER_URL_REFUSED,
            "The address is not an issue or a milestone of the polling repository, or the selected Project.",
        ));
    }
    Ok(url.to_string())
}

/// GDP contract surface: `"load github delivery plan (refresh)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn load_github_delivery_plan(
    app: AppHandle,
    refresh: bool,
) -> Result<GithubDeliveryPlan, DeliveryError> {
    source::off_thread(move || load_plan_impl(&app, refresh)).await
}

/// GDP contract surface: `"apply github delivery edit (edit)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn apply_github_delivery_edit(
    app: AppHandle,
    edit: edits::GithubDeliveryEdit,
) -> Result<GithubDeliveryPlan, DeliveryError> {
    source::off_thread(move || edits::apply_edit_impl(&app, edit)).await
}

/// GDP contract surface: `"read github plan issue (number)"`
/// (GDP-FR-WQEN, GDP-FR-KRMB). The read logs its own outcome.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn read_github_plan_issue(
    app: AppHandle,
    number: u64,
) -> Result<plan_reads::PlanIssueRead, DeliveryError> {
    plan_reads::read_plan_issue_detail(app, number).await
}

/// GDP contract surface: `"open tracker link (url)"`. The OS default browser
/// opens the URL; no page opens inside the application.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn open_tracker_link(app: AppHandle, url: String) -> Result<(), DeliveryError> {
    source::off_thread(move || {
        let started = Instant::now();
        let url = checked_tracker_url(&app, &url).inspect_err(|e| {
            // The URL is not logged: a refused one can carry anything.
            log(
                &app,
                LogLevel::Warn,
                "tracker link refused",
                crate::log_fields! { "code" => &e.code },
            );
        })?;
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|_| {
            let error = github_error(crate::github_polling::records::ERR_GITHUB_UNREACHABLE);
            log(
                &app,
                LogLevel::Warn,
                "tracker link could not be opened",
                crate::log_fields! { "code" => &error.code },
            );
            error
        })?;
        let kind = if url.contains("/milestone/") {
            "milestone"
        } else if url.contains("/issues/") {
            "issue"
        } else {
            "project"
        };
        log(
            &app,
            LogLevel::Info,
            "tracker link opened",
            crate::log_fields! { "kind" => kind, "ms" => started.elapsed().as_millis() as u64 },
        );
        Ok(())
    })
    .await
}

#[cfg(test)]
pub(crate) mod tests;
