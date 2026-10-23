//! The pull of the active worktree's branch, and the landing every write of a
//! pull goes through (GTC-FR-QHMT .. GTC-FR-IHCE).
//!
//! A pull fetches first, through the refresh of `WTC-worktree-context.md`
//! WTC-FR-22, so the listing the top chrome shows is current whatever the
//! pull does next. It then compares the branch with its upstream. A branch
//! with nothing of its own is fast-forwarded; a branch that has diverged is
//! reported and not written, because the author chooses how the two histories
//! join (`reconcile_pull`).

use std::path::Path;
use std::time::Instant;

use git2::{BranchType, Oid, Repository};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::changes::{self};
use crate::github_tokens::GithubTokens;
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, LogBuffer, LogLevel, LogSink};
use crate::progress::{self, ProgressRegistry, ProgressSink};
use crate::project::ProjectState;
use crate::worktree::{RefreshOutcome, REMOTE_FAILED, REMOTE_SKIPPED};

use super::*;

/// The operation name the output lines and the terminal event of a pull carry.
pub const OPERATION_PULL: &str = "pull";

/// The label the status bar shows while a pull runs (GTC-FR-IHCE).
pub(crate) const PULL_LABEL: &str = "Pulling";

/// GTC-FR-ZWAE: the branch tracks nothing.
pub const ERR_NO_UPSTREAM: &str = "no_upstream";
/// GTC-FR-ZWAE: the upstream's remote-tracking ref is absent.
pub const ERR_UPSTREAM_GONE: &str = "upstream_gone";
/// GTC-FR-ZWAE: the upstream belongs to a remote other than the primary one.
pub const ERR_UPSTREAM_NOT_ON_PRIMARY_REMOTE: &str = "upstream_not_on_primary_remote";
/// GTC-FR-VOXB: the landing would overwrite an uncommitted path.
pub const ERR_PULL_WOULD_OVERWRITE: &str = "pull_would_overwrite";
/// GTC-FR-NJUY: a tip moved after the author was shown it.
pub const ERR_PULL_TIPS_MOVED: &str = "pull_tips_moved";
/// GTC-FR-TRWC: a pull run is live for the worktree.
pub const ERR_PULL_RUN_ACTIVE: &str = "pull_run_active";

/// GTC-FR-KDRW: the states of a `PullOutcome`.
pub const PULL_UP_TO_DATE: &str = "up_to_date";
pub const PULL_FAST_FORWARDED: &str = "fast_forwarded";
pub const PULL_DIVERGED: &str = "diverged";
pub const PULL_REFUSED: &str = "refused";
pub const PULL_FETCH_FAILED: &str = "fetch_failed";

/// GTC-FR-SBLF / GTC-FR-EGIP: the states of a `PullReconcileOutcome`.
pub const RECONCILE_MERGED: &str = "merged";
pub const RECONCILE_REBASED: &str = "rebased";
pub const RECONCILE_HANDED_TO_RUN: &str = "handed_to_run";

/// GTC-FR-NJUY: how the author joins a diverged branch with its upstream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullStrategy {
    #[default]
    Merge,
    Rebase,
}

impl PullStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }
}

/// What `pull_current_branch` answers with (GTC contract surface).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PullOutcome {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_tip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_tip: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub updated_paths: Vec<String>,
    /// WTC-FR-PLHU: the refresh the pull ran first; always present.
    pub refresh: RefreshOutcome,
}

/// What `reconcile_pull` answers with (GTC contract surface).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PullReconcileOutcome {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The new tip of the branch, for `merged` and `rebased`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub updated_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_title: Option<String>,
}

impl PullReconcileOutcome {
    pub fn refused(error: String) -> Self {
        Self { state: PULL_REFUSED.to_string(), error: Some(error), ..Self::default() }
    }
}

/// GTC-FR-ZWAE: a branch and the upstream it tracks, both resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    pub branch: String,
    /// The upstream's short name, for example `origin/main`.
    pub upstream: String,
    pub local: Oid,
    pub remote: Oid,
}

/// GTC-FR-ZWAE: resolve the branch `repo` has out and its upstream, or the
/// typed refusal. Reads local refs alone.
pub fn resolve_upstream(repo: &Repository) -> Result<Upstream, String> {
    let branch = current_branch(repo).ok_or_else(|| ERR_NO_BRANCH_CHECKED_OUT.to_string())?;
    let local = repo
        .find_branch(&branch, BranchType::Local)
        .ok()
        .and_then(|b| b.get().peel_to_commit().ok())
        .map(|c| c.id())
        .ok_or_else(|| ERR_NO_BRANCH_CHECKED_OUT.to_string())?;
    let refname = format!("refs/heads/{branch}");
    let remote_name = repo
        .branch_upstream_remote(&refname)
        .ok()
        .and_then(|buf| buf.as_str().ok().map(str::to_string))
        .ok_or_else(|| ERR_NO_UPSTREAM.to_string())?;
    let upstream_ref = repo
        .branch_upstream_name(&refname)
        .ok()
        .and_then(|buf| buf.as_str().ok().map(str::to_string))
        .ok_or_else(|| ERR_NO_UPSTREAM.to_string())?;
    // A branch that tracks a local branch (`remote = .`) tracks no remote.
    if changes::primary_remote_name(repo).as_deref() != Some(remote_name.as_str()) {
        return Err(ERR_UPSTREAM_NOT_ON_PRIMARY_REMOTE.to_string());
    }
    let remote = repo
        .find_reference(&upstream_ref)
        .ok()
        .and_then(|r| r.peel_to_commit().ok())
        .map(|c| c.id())
        .ok_or_else(|| ERR_UPSTREAM_GONE.to_string())?;
    let upstream = upstream_ref.strip_prefix("refs/remotes/").unwrap_or(&upstream_ref).to_string();
    Ok(Upstream { branch, upstream, local, remote })
}

/// The short form of an object id the output lines name.
pub(crate) fn short_oid(oid: Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

/// GTC-FR-VOXB / GTC-FR-YLTM: land `new_tip` on `branch` in the worktree that
/// `holder` opens, and answer the paths it changed.
///
/// The overlap check runs before anything is written, so the typed refusal
/// and its path list do not depend on how libgit2 plans a checkout. The
/// checkout is safe, never forced, so an uncommitted path the pull does not
/// change keeps its content and its index state. The branch moves only after
/// the working tree is written, and only from `old_tip`.
pub fn land_on_branch(
    holder: &Repository,
    branch: &str,
    old_tip: Oid,
    new_tip: Oid,
) -> Result<Vec<String>, String> {
    let workdir = holder.workdir().ok_or_else(|| changes::ERR_NOT_A_REPO.to_string())?;
    // The branch and `HEAD` must both still stand at `old_tip` before any file
    // is written: a commit made since the tip was read would otherwise have its
    // files reverted by the checkout, and the move of the branch refused only
    // after that.
    let at_old_tip = |name: &str| {
        holder.find_reference(name).and_then(|r| r.peel_to_commit()).map(|c| c.id()).ok()
            == Some(old_tip)
    };
    let refname = format!("refs/heads/{branch}");
    let head_names_branch =
        holder.head().ok().and_then(|h| h.name().ok().map(str::to_string)).as_deref()
            == Some(refname.as_str());
    if !head_names_branch || !at_old_tip(&refname) || !at_old_tip("HEAD") {
        return Err(ERR_PULL_TIPS_MOVED.to_string());
    }
    let old_tree = holder.find_commit(old_tip).and_then(|c| c.tree()).map_err(|e| e.to_string())?;
    let new_tree = holder.find_commit(new_tip).and_then(|c| c.tree()).map_err(|e| e.to_string())?;
    let diff = holder
        .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)
        .map_err(|e| e.to_string())?;
    let mut changed: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        for path in [delta.old_file().path(), delta.new_file().path()].into_iter().flatten() {
            changed.push(path.to_string_lossy().into_owned());
        }
    }
    changed.sort();
    changed.dedup();

    let dirty = crate::streams::checked_out_uncommitted_paths_of(workdir);
    let overlap: Vec<String> =
        changed.iter().filter(|path| dirty.binary_search(path).is_ok()).cloned().collect();
    if !overlap.is_empty() {
        return Err(format!("{ERR_PULL_WOULD_OVERWRITE}: {}", overlap.join(", ")));
    }

    // The index is written once, unchanged, before the checkout, so an index
    // lock that another process holds refuses the landing before any file is
    // changed (the same order as the merge apply).
    let mut index = holder.index().map_err(|e| e.to_string())?;
    index.read(true).map_err(|e| e.to_string())?;
    index.write().map_err(|e| e.to_string())?;
    let mut opts = git2::build::CheckoutBuilder::new();
    opts.safe().update_index(true);
    holder.checkout_tree(new_tree.as_object(), Some(&mut opts)).map_err(|e| {
        if e.code() == git2::ErrorCode::Conflict {
            format!("{ERR_PULL_WOULD_OVERWRITE}: {}", e.message())
        } else {
            e.to_string()
        }
    })?;
    holder
        .reference_matching(&refname, new_tip, true, old_tip, "synthesis: pull")
        .map_err(|e| e.to_string())?;
    Ok(changed)
}

/// GTC-FR-OYDA: the emitter of a pull's output lines and its terminal event.
pub(crate) struct PullOutput<'a> {
    emit: &'a dyn Fn(&str, serde_json::Value),
}

impl<'a> PullOutput<'a> {
    pub(crate) fn new(emit: &'a dyn Fn(&str, serde_json::Value)) -> Self {
        Self { emit }
    }

    pub(crate) fn line(&self, text: &str) {
        (self.emit)(
            GIT_OUTPUT_LINE,
            serde_json::to_value(GitOutputLine {
                operation: OPERATION_PULL.to_string(),
                line: redact_credentials(text),
            })
            .unwrap_or_default(),
        );
    }

    /// GTC-FR-04: exactly one terminal event, whatever the outcome.
    pub(crate) fn finished(&self, ok: bool, error: Option<String>) {
        (self.emit)(
            GIT_OPERATION_FINISHED,
            serde_json::to_value(GitOperationFinished {
                operation: OPERATION_PULL.to_string(),
                ok,
                error,
            })
            .unwrap_or_default(),
        );
    }
}

/// The status bar label of a pull of `root` (GTC-FR-IHCE).
pub(crate) fn pull_label(root: &Path) -> String {
    match changes::open_repo(root).ok().and_then(|repo| current_branch(&repo)) {
        Some(branch) => format!("{PULL_LABEL} {branch}"),
        None => PULL_LABEL.to_string(),
    }
}

/// GTC-FR-QHMT .. GTC-FR-KDRW: pull the branch the worktree at `root` has out.
///
/// `refresh` is the refresh of WTC-FR-22, injected so the decision table runs
/// without a Tauri runtime. `guard` takes the guards of GTC-FR-TRWC for the
/// fast-forward and holds them until the landing ends. The call errors only
/// where `refresh` does, which is a root outside a Git repository.
pub fn pull_branch_at<S, H>(
    sink: &S,
    buffer: &'static LogBuffer,
    emit: &dyn Fn(&str, serde_json::Value),
    registry: &ProgressRegistry,
    root: &Path,
    refresh: impl FnOnce() -> Result<RefreshOutcome, String>,
    guard: impl FnOnce(&str) -> Result<H, String>,
) -> Result<PullOutcome, String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    let output = PullOutput::new(emit);
    let started = Instant::now();
    let result = progress::attribute(
        sink,
        registry,
        PROGRESS_KIND_GIT,
        &pull_label(root),
        Some(root.to_path_buf()),
        || pull_inner(&output, root, refresh, guard),
    );
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            output.line(&format!("Pull failed: {error}"));
            output.finished(false, Some(error.clone()));
            log_failure(sink, buffer, TRANSFER, "pull failed", &error, log_fields! {});
            return Err(error);
        }
    };
    let ok = outcome.state != PULL_REFUSED && outcome.state != PULL_FETCH_FAILED;
    output.finished(ok, outcome.error.clone());
    let fields = log_fields! {
        "state" => &outcome.state,
        "branch" => outcome.branch.clone().unwrap_or_default(),
        "upstream" => outcome.upstream.clone().unwrap_or_default(),
        "ahead" => outcome.ahead,
        "behind" => outcome.behind,
        "updatedCount" => outcome.updated_paths.len(),
        "durationMs" => duration_ms(started),
    };
    match &outcome.error {
        None => log_ok(sink, buffer, LogLevel::Info, TRANSFER, "branch pulled", fields),
        Some(error) => {
            let mut fields = fields;
            fields.insert("error".to_string(), serde_json::json!(error));
            log_ok(sink, buffer, LogLevel::Warn, TRANSFER, "pull refused", fields);
        }
    }
    Ok(outcome)
}

fn pull_inner<H>(
    output: &PullOutput<'_>,
    root: &Path,
    refresh: impl FnOnce() -> Result<RefreshOutcome, String>,
    guard: impl FnOnce(&str) -> Result<H, String>,
) -> Result<PullOutcome, String> {
    let refreshed = refresh()?;
    let mut outcome = PullOutcome { refresh: refreshed, ..PullOutcome::default() };
    let refuse = |mut outcome: PullOutcome, state: &str, error: String| {
        output.line(&format!("Pull refused: {error}"));
        outcome.state = state.to_string();
        outcome.error = Some(error);
        outcome
    };

    // GTC-FR-QHMT: a remote leg that did not refresh integrates nothing.
    if outcome.refresh.remote_state == REMOTE_SKIPPED {
        return Ok(refuse(outcome, PULL_REFUSED, ERR_NO_REMOTE_CONFIGURED.to_string()));
    }
    if outcome.refresh.remote_state == REMOTE_FAILED {
        let error = outcome.refresh.remote_error.clone().unwrap_or_default();
        return Ok(refuse(outcome, PULL_FETCH_FAILED, error));
    }

    let repo = changes::open_repo(root)?;
    let up = match resolve_upstream(&repo) {
        Ok(up) => up,
        Err(error) => return Ok(refuse(outcome, PULL_REFUSED, error)),
    };
    outcome.branch = Some(up.branch.clone());
    outcome.upstream = Some(up.upstream.clone());
    outcome.local_tip = Some(up.local.to_string());
    outcome.remote_tip = Some(up.remote.to_string());
    output.line(&format!("Pulling {} from {}", up.branch, up.upstream));

    let (ahead, behind) =
        repo.graph_ahead_behind(up.local, up.remote).map_err(|e| e.to_string())?;
    outcome.ahead = ahead;
    outcome.behind = behind;

    // GTC-FR-KDRW: the three answers of the comparison.
    if behind == 0 {
        output.line("Already up to date");
        outcome.state = PULL_UP_TO_DATE.to_string();
        return Ok(outcome);
    }
    if ahead > 0 {
        output.line(&format!(
            "{} and {} have diverged ({ahead} local, {behind} remote). Choose merge or rebase.",
            up.branch, up.upstream
        ));
        outcome.state = PULL_DIVERGED.to_string();
        return Ok(outcome);
    }

    // GTC-FR-TRWC: the guards are held for the landing alone.
    let _hold = match guard(&up.branch) {
        Ok(hold) => hold,
        Err(error) => return Ok(refuse(outcome, PULL_REFUSED, error)),
    };
    // Read again under the guard, so the landing stands on the tips compared.
    match resolve_upstream(&repo) {
        Ok(now) if now == up => {}
        _ => return Ok(refuse(outcome, PULL_REFUSED, ERR_PULL_TIPS_MOVED.to_string())),
    }
    match land_on_branch(&repo, &up.branch, up.local, up.remote) {
        Ok(paths) => {
            output.line(&format!(
                "Fast-forwarded {} {}..{} ({behind} commit{})",
                up.branch,
                short_oid(up.local),
                short_oid(up.remote),
                if behind == 1 { "" } else { "s" }
            ));
            outcome.state = PULL_FAST_FORWARDED.to_string();
            outcome.updated_paths = paths;
            Ok(outcome)
        }
        Err(error) => Ok(refuse(outcome, PULL_REFUSED, error)),
    }
}

/// CHC-FR-16: a pull moved `HEAD` and the index, which the watcher does not
/// watch, so every surface that follows the change set is told here. The pull
/// run's apply announces through it too (GRD-FR-PAPX).
pub(crate) fn announce_changes<R: tauri::Runtime>(app: &tauri::AppHandle<R>, change_count: usize) {
    if let Err(e) =
        app.emit(changes::CHANGES_UPDATED, changes::ChangesUpdatedPayload { change_count })
    {
        logging::log_warn(
            app,
            &logging::BUFFER,
            LOCAL,
            "pull landed but the changes-updated event was not delivered",
            log_fields! { "event" => changes::CHANGES_UPDATED, "error" => e.to_string() },
        );
    }
}

/// GTC-FR-QHMT: pull the active worktree's branch, streaming its output.
#[tauri::command]
pub async fn pull_current_branch(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<PullOutcome, String> {
    let root = project.require_root()?;
    let project_key = project.slot_key();
    tauri::async_runtime::spawn_blocking(move || {
        pull_current_branch_blocking(&app, &root, &project_key)
    })
    .await
    .map_err(|e| format!("pull failed: {e}"))?
}

/// `pull_current_branch` on the calling thread, generic over the runtime so the
/// composition of WTC-FR-PLHU runs against a headless app.
pub(crate) fn pull_current_branch_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    project_key: &str,
) -> Result<PullOutcome, String> {
    let store = app.state::<GlobalSettingsStore>();
    let tokens = app.state::<GithubTokens>();
    let registry = app.state::<ProgressRegistry>();
    let emit = |name: &str, payload: serde_json::Value| {
        // An event that cannot be delivered is dropped: reporting must never
        // take down the transfer.
        let _ = app.emit(name, payload);
    };
    let refresh = || {
        crate::worktree::refresh_and_announce(app, root, || {
            fetch_remote_branches(
                app,
                &logging::BUFFER,
                &registry,
                root,
                &store,
                &tokens,
                project_key,
            )
        })
    };
    let guard = |branch: &str| crate::streams::begin_pull(app, root, branch);
    let outcome = pull_branch_at(app, &logging::BUFFER, &emit, &registry, root, refresh, guard)?;
    if outcome.state == PULL_FAST_FORWARDED {
        announce_changes(app, outcome.updated_paths.len());
    }
    Ok(outcome)
}

/// GTC-FR-NJUY: integrate a diverged branch with its upstream on the author's
/// choice, at the tips the author was shown.
#[tauri::command]
pub async fn reconcile_pull(
    app: tauri::AppHandle,
    strategy: PullStrategy,
    local_tip: String,
    remote_tip: String,
) -> Result<PullReconcileOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let outcome =
            crate::streams::reconcile_pull_blocking(&app, strategy, &local_tip, &remote_tip);
        if outcome.state == RECONCILE_MERGED || outcome.state == RECONCILE_REBASED {
            announce_changes(&app, outcome.updated_paths.len());
        }
        Ok(outcome)
    })
    .await
    .map_err(|e| format!("pull failed: {e}"))?
}
