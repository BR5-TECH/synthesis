//! The Dashboard's widget loaders, its refresh events, and the application-wide
//! timer behind two of them (PST-FR-31 … PST-FR-37, serving
//! `../../specifications/ui/DSH-dashboard.md`).
//!
//! Every loader here answers from the project's **current records and the
//! current state of its filesystem**, and from nothing this application
//! remembers about what it has done. That is the whole design: an artifact
//! written by an external editor and one written in the Editor reach Recently
//! edited on identical terms, because what orders the widget is the modification
//! time of the file rather than a save history (PST-FR-17, PST-FR-31). There is
//! no most-recently-edited list anywhere, and `.synthesis/local.toml` holds none
//! (PSS-FR-12).
//!
//! Four channels tell an open Dashboard that one widget's data may have moved,
//! and each names one loader to re-invoke (PST-FR-35). Two of them —
//! agent activity and pending Git — are driven by the 5-minute timer of
//! PST-FR-36, which runs for as long as the application does, whether or not a
//! project is open and whether or not a Dashboard tab exists. Every refresh it
//! dispatches is overlap-safe, discards a stale result rather than applying it,
//! and reports a failure to the user through `"dashboard refresh failed"` rather
//! than into the log alone (PST-FR-37).
//!
//! The pure halves — [`RefreshSlots`], [`recently_edited_from`],
//! [`active_drafts_from`], and [`agent_runs_from`] — are unit-tested here
//! without a Tauri runtime.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{Emitter, Manager, State};

use crate::artifacts::ArtifactKind;
use crate::drafts::{DraftStatus, DraftSummary};
use crate::project::ProjectState;
use crate::scanning;

// ---------------------------------------------------------------------------
// Events (PST contract surface)
// ---------------------------------------------------------------------------

/// PST-FR-35: the Recently edited widget's data may have moved.
pub const DASHBOARD_RECENTLY_EDITED_CHANGED: &str = "dashboard-recently-edited-changed";
/// PST-FR-35: the Active workstreams widget's data may have moved.
pub const DASHBOARD_ACTIVE_DRAFTS_CHANGED: &str = "dashboard-active-drafts-changed";
/// PST-FR-35 / PST-FR-36: the agent-activity widget's data may have moved.
pub const DASHBOARD_AGENT_ACTIVITY_CHANGED: &str = "dashboard-agent-activity-changed";
/// PST-FR-35 / PST-FR-36: the Pending Git widget's data may have moved.
pub const DASHBOARD_PENDING_GIT_CHANGED: &str = "dashboard-pending-git-changed";
/// PST-FR-37: a background refresh of one widget could not be completed.
pub const DASHBOARD_REFRESH_FAILED: &str = "dashboard-refresh-failed";

/// The five-item limit every widget loader applies (DSH-FR-09, DSH-FR-11,
/// DSH-FR-13).
///
/// Applied by the loader rather than by the surface, so the Dashboard renders
/// what it is given and two surfaces cannot disagree about what "the five most
/// recent" means (`DSH-dashboard.md` non-functional requirements).
pub const DASHBOARD_ITEM_CAP: usize = 5;

/// PST-FR-36: exactly five minutes, and fixed rather than derived from how long
/// a refresh takes.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// The widget a refresh event names (PST-FR-37).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardWidget {
    RecentlyEdited,
    ActiveDrafts,
    AgentActivity,
    PendingGit,
}

impl DashboardWidget {
    /// The refresh event this widget's successful reading emits.
    fn changed_event(self) -> &'static str {
        match self {
            DashboardWidget::RecentlyEdited => DASHBOARD_RECENTLY_EDITED_CHANGED,
            DashboardWidget::ActiveDrafts => DASHBOARD_ACTIVE_DRAFTS_CHANGED,
            DashboardWidget::AgentActivity => DASHBOARD_AGENT_ACTIVITY_CHANGED,
            DashboardWidget::PendingGit => DASHBOARD_PENDING_GIT_CHANGED,
        }
    }

    /// PST-FR-36: whether the 5-minute timer refreshes this widget.
    ///
    /// Only two of the four. The other two follow the filesystem, and their
    /// events are emitted by the watchers that observe it (PST-FR-35) — a timer
    /// tick reads neither and must dispatch neither.
    fn is_timer_driven(self) -> bool {
        matches!(
            self,
            DashboardWidget::AgentActivity | DashboardWidget::PendingGit
        )
    }

    /// The name a log record and the failure payload carry.
    fn as_str(self) -> &'static str {
        match self {
            DashboardWidget::RecentlyEdited => "recently_edited",
            DashboardWidget::ActiveDrafts => "active_drafts",
            DashboardWidget::AgentActivity => "agent_activity",
            DashboardWidget::PendingGit => "pending_git",
        }
    }
}

/// Payload of `"dashboard refresh failed"` (PST-FR-37).
///
/// `error` is the typed failure the loader returned — a message this module
/// composed about a read it performed. Nothing of a file's contents and no
/// credential reaches it: the loaders here read modification times, a change
/// set's paths, and a commit count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardRefreshFailed {
    pub widget: DashboardWidget,
    pub error: String,
}

// ---------------------------------------------------------------------------
// Payload shapes (PST contract surface)
// ---------------------------------------------------------------------------

/// One row of the Recently edited widget (PST-FR-31).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentlyEditedItem {
    /// The artifact's stable, path-derived key (ASC-FR-13).
    pub id: String,
    /// The artifact's display name, from its current record.
    pub name: String,
    /// The routing hint of `open_artifact_by_id` (PST-FR-08). Never `text`: a
    /// file carrying no artifact type is not an artifact and never appears here
    /// (PST-FR-23).
    pub kind: ArtifactKind,
    /// RFC 3339 UTC — the primary source file's filesystem modification time,
    /// read at load.
    pub modified_at: String,
}

/// One row of the Active workstreams widget (PST-FR-32).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveDraftItem {
    pub draft_id: String,
    /// The draft's **stored** name (DRS-FR-03) — never derived from a file's
    /// contents or from any other filesystem metadata.
    pub name: String,
    /// Always `active`: this loader keeps no other status (PST-FR-32).
    pub status: DraftStatus,
    /// RFC 3339 UTC — the prompt file's filesystem modification time
    /// (DRS-FR-41).
    pub activity_at: String,
}

/// One row of the Last/current agent activity widget (PST-FR-33).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunItem {
    pub run_id: String,
    pub draft_id: String,
    /// The source draft's stored name, or `None` for a run whose draft no longer
    /// resolves — such a run is still returned, carrying its `draft_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_name: Option<String>,
    /// PST-FR-33: the work stream the run belongs to. A position means nothing
    /// without the queue it is a position of, and the queue is the stream's.
    pub stream_id: String,
    /// That stream's own name; for a direct run, the name of the worktree it
    /// pinned (GRD-FR-BSNI).
    pub stream_name: String,
    pub state: crate::graduation::GraduationRunState,
    pub stage: crate::graduation::observability::GraduationVisualStage,
    pub stage_condition: crate::graduation::observability::GraduationStageCondition,
    /// The run's index in **its own** queue (a stream's, or an ordinary
    /// worktree's for a direct run), while it is `queued`;
    /// `None` for a run that has left the queue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_position: Option<usize>,
    /// RFC 3339 UTC — the run's own `updated_at`.
    pub updated_at: String,
}

/// The four counts of the Pending Git activity widget (PST-FR-34).
///
/// Every count is optional, and `None` means **unavailable** rather than zero:
/// the two commit counts on a branch with no upstream, and all four for a
/// project whose content root is outside a Git repository. The widget renders
/// unavailable rather than `0` (DSH-FR-14), because "level with the remote" and
/// "there is no remote to be level with" are different answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingGitActivity {
    /// Uncommitted changed paths that carry an artifact type.
    pub modified_artifacts: Option<usize>,
    /// Uncommitted changed paths that carry none. Together with the above the
    /// two partition the change set: no path is counted twice or left out.
    pub modified_source_files: Option<usize>,
    /// Commits the current branch holds above its upstream.
    pub unpushed_commits: Option<usize>,
    /// Commits the upstream holds above the current branch.
    pub fetchable_commits: Option<usize>,
}

/// One row of the Reminders widget (PST-FR-12), backed by the notes carrying a
/// reminder (NTC-FR-17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderItem {
    pub note_id: String,
    /// The entity the reminder is attached to, so the widget's click-through has
    /// a target (DSH-FR-06); `None` for a project-scoped note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// RFC 3339 UTC — when the reminder falls due.
    pub due_at: String,
}

/// One row of the Project health signals widget (PST-FR-12).
///
/// The one loader of the six that serves no real project data yet: it returns an
/// empty list, which hides the widget (DSH-FR-04). The shape is real so that
/// landing a real source later replaces a data source and changes no rendering
/// rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealthSignal {
    pub id: String,
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Recently edited (PST-FR-31)
// ---------------------------------------------------------------------------

/// Flatten the scanned tree into one entry per **artifact**: a file node with a
/// resolved artifact type (ASC-FR-08).
///
/// A folder node is skipped, and so is a file carrying no type — the loader
/// enumerates artifacts, and a plain text file is not one, however recently it
/// was written (PST-FR-23).
///
/// The name is the artifact's **display name**: the name the file declares for
/// itself where its filename identifies nothing (ASC-FR-19), and the basename
/// otherwise. The same fallback every other surface applies, so a skill reads as
/// `analyst` here exactly as it does in the Project panel rather than as the
/// `SKILL.md` every skill shares.
fn collect_artifact_nodes(
    node: &scanning::TreeNode,
    out: &mut Vec<(String, String, scanning::ArtifactType)>,
) {
    if let (scanning::NodeKind::File, Some(t)) = (node.node_kind, node.artifact_type) {
        let name = node.display_name.clone().unwrap_or_else(|| node.name.clone());
        out.push((node.id.clone(), name, t));
    }
    if let Some(children) = &node.children {
        for c in children {
            collect_artifact_nodes(c, out);
        }
    }
}

/// PST-FR-31 (pure): the five most recently modified artifacts, from a list of
/// `(id, name, kind, modified)` readings.
///
/// Ordered **descending by modification time**, with the stable id **ascending**
/// as the tie-breaker, so two files carrying the same instant are ordered the
/// same way on every call. Pure over the readings so the ordering, the
/// tie-break, and the cap are testable without a filesystem.
fn recently_edited_from(
    mut readings: Vec<(String, String, ArtifactKind, SystemTime)>,
) -> Vec<RecentlyEditedItem> {
    readings.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));
    readings.truncate(DASHBOARD_ITEM_CAP);
    readings
        .into_iter()
        .map(|(id, name, kind, modified)| RecentlyEditedItem {
            id,
            name,
            kind,
            modified_at: crate::notes::format_rfc3339_millis_from(modified),
        })
        .collect()
}

/// PST-FR-31: the artifacts the project currently holds whose primary source
/// files were modified most recently.
///
/// One `file_info` per artifact and no file's contents at all, so the cost grows
/// with the number of artifacts rather than with their size. An artifact whose
/// source file is missing or cannot be statted is **omitted** — the loader
/// continues over the rest and returns them, and the omission is recorded
/// through the session log alone. One file the filesystem cannot describe never
/// fails the widget.
fn recently_edited_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
) -> Vec<RecentlyEditedItem> {
    let tree = scanning::scan(root);
    let mut nodes = Vec::new();
    collect_artifact_nodes(&tree, &mut nodes);
    let (readings, omitted) = stat_artifacts(nodes, |id| {
        root.file_info(root.path().join(id)).ok().and_then(|i| i.modified)
    });
    for id in omitted {
        // Handled, and therefore invisible unless it is said out loud — one
        // record per omission, which is the whole account of why an artifact the
        // project holds is not in the widget. The path is structural: it is not
        // the file's contents, and nothing of what the artifact holds is read.
        crate::logging::log_debug(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "recently edited: artifact omitted, no modification time",
            crate::log_fields! { "artifact" => id },
        );
    }
    recently_edited_from(readings)
}

/// PST-FR-31 (pure): stat every enumerated artifact, keeping the ones the
/// filesystem can describe and naming the ones it cannot.
///
/// An artifact whose primary source file is missing or cannot be statted is
/// **omitted** and the walk continues over the rest — one file the filesystem
/// cannot describe never fails the widget. The omissions are returned rather
/// than logged here so that the rule and its reporting are testable without a
/// filesystem that can be made to refuse a stat on demand.
#[allow(clippy::type_complexity)]
fn stat_artifacts(
    nodes: Vec<(String, String, scanning::ArtifactType)>,
    stat: impl Fn(&str) -> Option<SystemTime>,
) -> (Vec<(String, String, ArtifactKind, SystemTime)>, Vec<String>) {
    let mut readings = Vec::with_capacity(nodes.len());
    let mut omitted = Vec::new();
    for (id, name, artifact_type) in nodes {
        match stat(&id) {
            Some(modified) => readings.push((id, name, ArtifactKind::from(artifact_type), modified)),
            None => omitted.push(id),
        }
    }
    (readings, omitted)
}

/// `"list recently edited artifacts"` (PST-FR-31).
///
/// Answers with an empty list rather than an error when no project is open
/// (PST-FR-12), which is what hides the widget on an empty Dashboard.
#[tauri::command]
pub fn list_recently_edited_artifacts(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<Vec<RecentlyEditedItem>, String> {
    let Ok(root) = project.require_root() else {
        return Ok(Vec::new());
    };
    Ok(recently_edited_impl(&app, &root))
}

// ---------------------------------------------------------------------------
// Active drafts (PST-FR-32)
// ---------------------------------------------------------------------------

/// PST-FR-32 (pure): the five `active` drafts whose prompt files were written
/// most recently.
///
/// The **prompt file is the only activity source**: an archived and a graduated
/// draft are both dropped, and a draft the module reports as inconsistent is
/// omitted (it holds no single prompt to stat, DRS-FR-15) while the rest are
/// still returned. `name` and `status` come from the draft's stored record, so
/// no filesystem metadata ever decides a draft's display name or its lifecycle
/// status.
fn active_drafts_from(drafts: Vec<DraftSummary>) -> Vec<ActiveDraftItem> {
    let mut items: Vec<ActiveDraftItem> = drafts
        .into_iter()
        // DRS-FR-OGZC: `published` is not a retired position. A published draft
        // edits, renames, graduates, and archives exactly as an active one
        // does, so it is work in flight like any other.
        .filter(|d| {
            matches!(d.status, DraftStatus::Active | DraftStatus::Published) && !d.inconsistent
        })
        .filter_map(|d| {
            d.prompt_activity_at.clone().map(|activity_at| ActiveDraftItem {
                draft_id: d.id,
                name: d.name,
                status: DraftStatus::Active,
                activity_at,
            })
        })
        .collect();
    // Descending by prompt activity, the draft id ascending as the tie-break, so
    // two prompts written in the same millisecond order the same way every call.
    items.sort_by(|a, b| {
        b.activity_at
            .cmp(&a.activity_at)
            .then_with(|| a.draft_id.cmp(&b.draft_id))
    });
    items.truncate(DASHBOARD_ITEM_CAP);
    items
}

/// `"list active drafts"` (PST-FR-32).
#[tauri::command]
pub fn list_active_drafts(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<Vec<ActiveDraftItem>, String> {
    let Ok(root) = project.require_root() else {
        return Ok(Vec::new());
    };
    let mut drafts = crate::drafts::list_drafts_impl(&root).drafts;
    // DRS-FR-KQTW: the status is resolved against the graduation queue by the
    // same route the Drafts panel reads it by, so a draft released by the
    // discard or the failure of every run that published for it is a workstream
    // again here as well. Reading the stored record instead would leave this
    // widget the one surface still filing it away.
    crate::drafts::attach_graduation(&app, &mut drafts);
    Ok(active_drafts_from(drafts))
}

// ---------------------------------------------------------------------------
// Recent agent runs (PST-FR-33)
// ---------------------------------------------------------------------------

/// PST-FR-33 (pure): the five most recently updated graduation runs, runs still
/// `queued` included.
///
/// Ordered descending by `updated_at` with the run id ascending as the
/// tie-break. `state`, `stage`, and `stage_condition` are the run's persisted
/// state and its `GraduationObservability` — nothing is derived and nothing is
/// inferred about a run. `queue_position` is the run's index in its own work
/// stream's queue, and is `None` for a run that has left `queued`. Runs of two
/// streams stand in one project order but wait in queues of their own, so a
/// position counted over the whole order would name a place no run holds.
/// PST-FR-33: how many runs of the same queue wait ahead of this one.
///
/// A queue is a stream's, or an ordinary worktree's for a direct run, which has
/// no stream id (GRD-FR-ZVNO). Counting by stream id would make every direct
/// run of every worktree wait behind the others.
fn position_in_queue(
    runs: &[crate::graduation::GraduationRun],
    index: usize,
    queue_key: &str,
) -> usize {
    runs[..index]
        .iter()
        .filter(|earlier| earlier.queue_key() == queue_key && earlier.state.waits_in_queue())
        .count()
}

fn agent_runs_from(
    runs: &[crate::graduation::GraduationRun],
    draft_names: &HashMap<String, String>,
) -> Vec<AgentRunItem> {
    let mut items: Vec<AgentRunItem> = runs
        .iter()
        .enumerate()
        .map(|(index, run)| AgentRunItem {
            run_id: run.id.clone(),
            draft_id: run.input.draft_id.clone(),
            draft_name: draft_names.get(&run.input.draft_id).cloned(),
            stream_id: run.stream_id.clone(),
            stream_name: run.target_label(),
            state: run.state,
            stage: run.observability.current_stage,
            stage_condition: run.observability.stage_condition,
            queue_position: (run.state == crate::graduation::GraduationRunState::Queued)
                .then(|| position_in_queue(runs, index, &run.queue_key())),
            updated_at: run.updated_at.clone(),
        })
        .collect();
    items.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.run_id.cmp(&b.run_id))
    });
    items.truncate(DASHBOARD_ITEM_CAP);
    items
}

/// PST-FR-33: the queue read, with each returned run's source draft resolved to
/// its stored name.
///
/// The names are read after the cap is applied, so a queue holding a hundred
/// runs costs five record reads rather than a hundred.
fn agent_runs_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
) -> Result<Vec<AgentRunItem>, String> {
    let key = project.slot_key();
    if key.is_empty() {
        return Ok(Vec::new());
    }
    let queue = crate::graduation::project_queue(app)
        .ok_or_else(|| crate::graduation::ERR_NO_PROJECT_OPEN.to_string())?;
    let _ = &key;
    let mut items = agent_runs_from(&queue.runs, &HashMap::new());
    if let Ok(root) = project.require_root() {
        for item in &mut items {
            // DRS-FR-38: read from the record rather than from anything the run
            // captured, so a draft renamed since the run started is named as it
            // now stands. A run whose draft no longer resolves keeps its
            // `draft_id` and no name.
            item.draft_name = crate::drafts::draft_record(&root, &item.draft_id)
                .ok()
                .map(|record| record.name);
        }
    }
    Ok(items)
}

/// `"list recent agent runs"` (PST-FR-33).
#[tauri::command]
pub fn list_recent_agent_runs(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<Vec<AgentRunItem>, String> {
    agent_runs_impl(&app, &project)
}

// ---------------------------------------------------------------------------
// Pending Git activity (PST-FR-34)
// ---------------------------------------------------------------------------

/// PST-FR-34: the four counts, from two reads and no others.
///
/// **No fetch and no network call.** Both commit counts come from local refs
/// alone (GTC-FR-21), so a refresh costs the same on a slow connection as on
/// none, and both are `None` on a branch with no upstream. A content root
/// outside a Git repository answers with all four counts `None` rather than
/// with an error: there is nothing pending in a repository that does not exist.
fn pending_git_impl(root: &crate::fs::RootFs) -> Result<PendingGitActivity, String> {
    if crate::changes::open_repo(root).is_err() {
        return Ok(PendingGitActivity::default());
    }
    let set = crate::changes::uncommitted_change_set(root)?;
    let (mut modified_artifacts, mut modified_source_files) = (0usize, 0usize);
    for entry in &set.entries {
        // The two counts partition the change set (CHC-FR-10): an entry either
        // carries an artifact type or it does not.
        if entry.artifact_type.is_some() {
            modified_artifacts += 1;
        } else {
            modified_source_files += 1;
        }
    }
    let sync = crate::git::upstream_sync_state(root.path())?;
    Ok(PendingGitActivity {
        modified_artifacts: Some(modified_artifacts),
        modified_source_files: Some(modified_source_files),
        unpushed_commits: sync.ahead,
        fetchable_commits: sync.behind,
    })
}

/// `"list pending git activity"` (PST-FR-34).
#[tauri::command]
pub fn list_pending_git_activity(
    project: State<'_, ProjectState>,
) -> Result<PendingGitActivity, String> {
    let Ok(root) = project.require_root() else {
        return Ok(PendingGitActivity::default());
    };
    pending_git_impl(&root)
}

// ---------------------------------------------------------------------------
// Reminders and health signals (PST-FR-12)
// ---------------------------------------------------------------------------

/// `"list due/upcoming reminders"` (PST-FR-12), backed by the notes carrying a
/// reminder (NTC-FR-17). Soonest first, which is the order that module returns.
#[tauri::command]
pub fn list_due_reminders(project: State<'_, ProjectState>) -> Result<Vec<ReminderItem>, String> {
    let Ok(root) = project.require_root() else {
        return Ok(Vec::new());
    };
    Ok(crate::notes::reminder_notes(&root)
        .into_iter()
        .filter_map(|note| {
            note.reminder.clone().map(|due_at| ReminderItem {
                note_id: note.id.clone(),
                entity_id: note.scope.entity_id().map(|s| s.to_string()),
                due_at,
            })
        })
        .take(DASHBOARD_ITEM_CAP)
        .collect())
}

/// `"list project health signals"` (PST-FR-12). Empty in the walking skeleton,
/// which hides the widget (DSH-FR-04).
#[tauri::command]
pub fn list_project_health_signals(
    _project: State<'_, ProjectState>,
) -> Result<Vec<ProjectHealthSignal>, String> {
    Ok(Vec::new())
}

// ---------------------------------------------------------------------------
// Refresh events driven by the filesystem (PST-FR-35)
// ---------------------------------------------------------------------------

/// PST-FR-35: what PST-FR-31 enumerates, or how it orders, may have changed.
///
/// Emitted from the content-modification channel and from the structural channel
/// of the project watcher, and **with no self-write suppression**: a write this
/// application performed moves the file's modification time and must therefore
/// move the widget, which is the whole point of ordering by that time.
pub fn note_recently_edited_changed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let _ = app.emit(DASHBOARD_RECENTLY_EDITED_CHANGED, ());
}

/// PST-FR-35: which drafts qualify, how they are named, or how they order may
/// have changed.
///
/// Emitted on `"draft prompt changed"` (DRS-FR-42), which is what moves the
/// ordering, and on `"drafts changed"` (DRS-FR-22), which changes which drafts
/// qualify and what their names and statuses are. A refresh caused by the second
/// re-reads the same prompt-file times and therefore reorders nothing.
pub fn note_active_drafts_changed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let _ = app.emit(DASHBOARD_ACTIVE_DRAFTS_CHANGED, ());
}

// ---------------------------------------------------------------------------
// The background refresh (PST-FR-36 / PST-FR-37)
// ---------------------------------------------------------------------------

/// One widget's refresh bookkeeping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Slot {
    in_flight: bool,
    /// A tick that arrived while a refresh was in flight. Several such ticks
    /// coalesce into this one flag: each would read the same source, and only
    /// the newest reading is of any use.
    pending: bool,
    next_seq: u64,
    /// The highest sequence that has settled for this widget.
    last_settled: u64,
}

/// The per-widget refresh state of PST-FR-37, pure over its own fields so the
/// coalescing and the stale-result rule are unit-testable without a runtime.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RefreshSlots {
    recently_edited: Slot,
    active_drafts: Slot,
    agent_activity: Slot,
    pending_git: Slot,
}

impl RefreshSlots {
    fn slot(&mut self, widget: DashboardWidget) -> &mut Slot {
        match widget {
            DashboardWidget::RecentlyEdited => &mut self.recently_edited,
            DashboardWidget::ActiveDrafts => &mut self.active_drafts,
            DashboardWidget::AgentActivity => &mut self.agent_activity,
            DashboardWidget::PendingGit => &mut self.pending_git,
        }
    }

    /// A tick asks for a refresh of `widget`.
    ///
    /// `Some(seq)` is the sequence to run now; `None` means the tick was
    /// recorded as the one pending refresh behind the reading already in flight.
    /// **No tick is lost either way** (PST-FR-36).
    fn begin(&mut self, widget: DashboardWidget) -> Option<u64> {
        let slot = self.slot(widget);
        if slot.in_flight {
            slot.pending = true;
            return None;
        }
        slot.in_flight = true;
        slot.next_seq += 1;
        Some(slot.next_seq)
    }

    /// A dispatched refresh has returned.
    ///
    /// The first flag is whether its result is **fresh** — the highest sequence
    /// yet settled for this widget — and so may be applied; a result whose
    /// sequence has been overtaken is discarded, and neither its refresh event
    /// nor a failure event is emitted for it. The second is the sequence of the
    /// coalesced refresh to start now, which is what makes a pending refresh
    /// begin the moment the in-flight one settles.
    fn settle(&mut self, widget: DashboardWidget, seq: u64) -> (bool, Option<u64>) {
        let slot = self.slot(widget);
        let fresh = seq > slot.last_settled;
        if fresh {
            slot.last_settled = seq;
        }
        if slot.pending {
            slot.pending = false;
            slot.next_seq += 1;
            // The slot stays in flight: handing the seat straight to the pending
            // refresh is what keeps "at most one read of this source at a time"
            // true without a window a tick could slip through.
            return (fresh, Some(slot.next_seq));
        }
        slot.in_flight = false;
        (fresh, None)
    }
}

/// The application-wide refresh state (PST-FR-37). One instance for the process,
/// like the timer it serves.
#[derive(Default)]
pub struct DashboardRefreshState {
    slots: std::sync::Mutex<RefreshSlots>,
}

/// The project and active worktree a refresh was dispatched for (PST-FR-37).
///
/// A result belonging to a project or a worktree no longer open is discarded
/// rather than reported as the incoming root's data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RootIdentity {
    project_key: String,
    worktree: String,
}

/// The open project's identity, or the empty one when no project is open.
///
/// `try_state`, not `state`: this runs on a background thread, where a panic
/// surfaces as a silently dead timer rather than as a failure anyone sees.
fn current_identity<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> RootIdentity {
    app.try_state::<ProjectState>()
        .map(|project| identity_of(&project))
        .unwrap_or_default()
}

impl RootIdentity {
    /// PST-FR-36: no project is open, so a tick dispatches nothing and a
    /// refresh that settles against it emits nothing.
    fn is_open(&self) -> bool {
        !self.worktree.is_empty()
    }
}

fn identity_of(project: &ProjectState) -> RootIdentity {
    RootIdentity {
        project_key: project.slot_key(),
        worktree: project
            .root()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// PST-FR-36: one tick — dispatch a refresh of **both** timer-driven widgets for
/// the currently open project and return at once, awaiting neither.
///
/// A tick that finds no project open dispatches nothing, emits nothing, and
/// keeps the cadence: the next tick is due 5 minutes later whatever this one
/// found.
fn tick<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if !current_identity(app).is_open() {
        return;
    }
    dispatch_refresh(app, DashboardWidget::AgentActivity);
    dispatch_refresh(app, DashboardWidget::PendingGit);
}

/// Dispatch one widget's refresh, or record it as the pending one behind a
/// reading already in flight (PST-FR-37).
///
/// Returns at once in both cases: nothing here is awaited, and no part of it
/// runs on a thread that serves the UI.
pub fn dispatch_refresh<R: tauri::Runtime>(app: &tauri::AppHandle<R>, widget: DashboardWidget) {
    if !widget.is_timer_driven() {
        // PST-FR-35: the Recently edited and Active workstreams widgets follow
        // the filesystem, and their events are emitted by the watchers that
        // observe it. Dispatching one here would emit a refresh event for a
        // read that never happened — a lie rather than a no-op — so it is
        // refused and said out loud.
        crate::logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "dashboard refresh refused: widget is not timer-driven",
            crate::log_fields! { "widget" => widget.as_str() },
        );
        return;
    }
    let Some(state) = app.try_state::<DashboardRefreshState>() else {
        // Unmanaged state would leave the slot claimed for good, so the absence
        // is reported rather than swallowed.
        crate::logging::log_error(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "dashboard refresh state is not managed",
            crate::log_fields! { "widget" => widget.as_str() },
        );
        return;
    };
    let Ok(mut slots) = state.slots.lock() else {
        crate::logging::log_error(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "dashboard refresh state is poisoned",
            crate::log_fields! { "widget" => widget.as_str() },
        );
        return;
    };
    let Some(seq) = slots.begin(widget) else {
        // Coalesced: the ticks are honoured, the redundant reads are not.
        return;
    };
    drop(slots);
    run_refresh(app, widget, seq);
}

/// Read one widget's source off the UI thread and settle the result
/// (PST-FR-37).
fn run_refresh<R: tauri::Runtime>(app: &tauri::AppHandle<R>, widget: DashboardWidget, seq: u64) {
    let dispatched_for = current_identity(app);
    let app = app.clone();
    // A dedicated thread rather than the async runtime's cooperative scheduler:
    // every loader below is a blocking filesystem or libgit2 read, and blocking
    // a runtime worker is what would eventually starve the commands that share
    // it (PST-FR-37, "nothing here occupies a UI thread").
    std::thread::spawn(move || {
        // The slot is claimed until this thread settles it, so a panic inside a
        // loader would leave the widget "refreshing" for the life of the
        // process — never refreshed again, with nothing said about it. Catching
        // it here turns that into an ordinary failed refresh: the widget's
        // error state, one ERROR record, and the next tick trying again.
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_for(&app, widget)
        }));
        let outcome = read.unwrap_or_else(|_| {
            Err("the widget's reader panicked".to_string())
        });
        settle_refresh(&app, widget, seq, &dispatched_for, outcome);
    });
}

/// One widget's read, for the thread of [`run_refresh`].
fn read_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    widget: DashboardWidget,
) -> Result<(), String> {
    #[allow(clippy::let_and_return)]
    {
        let outcome: Result<(), String> = match widget {
            DashboardWidget::AgentActivity => match app.try_state::<ProjectState>() {
                Some(project) => agent_runs_impl(&app, &project).map(|_| ()),
                None => Ok(()),
            },
            DashboardWidget::PendingGit => {
                match app.try_state::<ProjectState>().and_then(|p| p.require_root().ok()) {
                    Some(root) => pending_git_impl(&root).map(|_| ()),
                    // The project closed while the refresh was queued. Not a
                    // failure: the result is discarded below on identity.
                    None => Ok(()),
                }
            }
            // Unreachable: `dispatch_refresh` refuses a widget the timer does
            // not drive, so nothing reaches here without a read to perform.
            DashboardWidget::RecentlyEdited | DashboardWidget::ActiveDrafts => {
                debug_assert!(false, "a filesystem-driven widget was dispatched");
                Err("this widget is refreshed by the filesystem, not by the timer".to_string())
            }
        };
        outcome
    }
}

/// Apply or discard a settled refresh, and start the coalesced one behind it
/// (PST-FR-37).
fn settle_refresh<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    widget: DashboardWidget,
    seq: u64,
    dispatched_for: &RootIdentity,
    outcome: Result<(), String>,
) {
    let (fresh, next) = {
        let Some(state) = app.try_state::<DashboardRefreshState>() else {
            return;
        };
        let Ok(mut slots) = state.slots.lock() else {
            return;
        };
        slots.settle(widget, seq)
    };
    match settlement(fresh, &current_identity(app), dispatched_for, outcome) {
        Settlement::Changed => {
            let _ = app.emit(widget.changed_event(), ());
        }
        Settlement::Failed(error) => {
            // PST-FR-37: a failed refresh reaches the user, not only the log.
            // The error is a message this module composed about a read it
            // performed — a path, a count, a libgit2 summary — and carries no
            // credential and no file's contents.
            crate::logging::log_error(
                app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "dashboard refresh failed",
                crate::log_fields! {
                    "widget" => widget.as_str(),
                    "reason" => error.clone(),
                },
            );
            let _ = app.emit(DASHBOARD_REFRESH_FAILED, DashboardRefreshFailed { widget, error });
        }
        // A discarded stale result is not a failure and emits neither event
        // (PST-FR-37); it is not logged as one either, an overtaken read being
        // ordinary rather than notable.
        Settlement::Discarded => {}
    }
    if let Some(next_seq) = next {
        run_refresh(app, widget, next_seq);
    }
}

/// What a settled refresh does: emit its widget's refresh event, report a
/// failure, or be discarded (PST-FR-37).
#[derive(Debug, PartialEq, Eq)]
enum Settlement {
    Changed,
    Failed(String),
    Discarded,
}

/// PST-FR-37 (pure): decide a settled refresh's fate.
///
/// A result is applied — and its event emitted — only when its sequence is the
/// highest yet settled for that widget **and** the identity it was dispatched
/// for is still the open one. A result whose sequence has been overtaken, one
/// belonging to a project or worktree no longer open, and one that settles while
/// no project is open at all are each **discarded**: no event, no error, and the
/// widget's data left as it is.
///
/// Pure over its four inputs, because the alternative is a test that asserts two
/// `RootIdentity` values differ and calls that coverage of the rule.
fn settlement(
    fresh: bool,
    current: &RootIdentity,
    dispatched_for: &RootIdentity,
    outcome: Result<(), String>,
) -> Settlement {
    if !fresh || !current.is_open() || current != dispatched_for {
        return Settlement::Discarded;
    }
    match outcome {
        Ok(()) => Settlement::Changed,
        Err(error) => Settlement::Failed(error),
    }
}

/// PST-FR-36: start the application-wide 5-minute refresh timer.
///
/// It starts when the application starts and runs for as long as the application
/// runs, whether or not a project is open and whether or not a Dashboard tab
/// exists anywhere. The cadence is fixed rather than derived from how long any
/// refresh takes — the tick dispatches and returns — so the next tick falls due
/// 5 minutes after the last whatever either refresh is still doing. Closing the
/// Dashboard tab, closing a project, and changing the active worktree each leave
/// it running; it stops only when the application quits.
pub fn start_refresh_timer<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(REFRESH_INTERVAL);
        // PST-FR-36: no tick is skipped, dropped, or silently coalesced away.
        // `Burst` is what says that — ticks the process slept through (a closed
        // laptop lid) fall due at once rather than being discarded — and it is
        // affordable precisely because the pending-refresh handling of
        // PST-FR-37 collapses them into at most one extra read per widget.
        // Stated rather than defaulted, because the alternative would silently
        // drop ticks the requirement forbids dropping.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
        // The first tick of a tokio interval completes immediately; the timer's
        // first refresh is due 5 minutes after start, not at start.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            tick(&app);
        }
    });
}

#[cfg(test)]
mod tests;
