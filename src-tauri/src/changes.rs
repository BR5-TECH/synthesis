//! Change-set computation for the Changes vertical panel
//! (`specifications/core/CHC-changes.md`).
//!
//! Answers the panel's two questions — what is modified but not yet committed
//! (CHC-FR-03) and what the current branch introduces relative to a target
//! branch (CHC-FR-04) — as a flat, fully-annotated [`ChangeSet`]. Every entry
//! carries the per-file line-count summary (CHC-FR-07), the binary marker
//! (CHC-FR-08), the rename's previous path (CHC-FR-09) and the artifact type
//! the Library reports for the same path (CHC-FR-10), so the panel can group,
//! tag and filter without a second round-trip.
//!
//! The module is strictly read-only with respect to the repository
//! (CHC-FR-18): it opens, diffs, and enumerates refs, and never writes to the
//! index, the object database, refs, or the working tree. The diff *payload* a
//! Diff tab renders is not owned here — it comes from `crate::git`'s
//! `get_diff` (GTC).
//!
//! Grouping into a folder tree and the **Unrevisioned** group are deliberately
//! absent (CHC-FR-12): `entries` is flat and the UI assembles the tree.

use std::collections::BTreeSet;
use std::path::Path;

use git2::{BranchType, Delta, Diff, DiffFindOptions, DiffOptions, Oid, Repository};
use notify_debouncer_mini::DebounceEventResult;
use serde::Serialize;

use crate::progress::{self, ProgressRegistry};
use crate::project::ProjectState;
use crate::scanning::{ArtifactType, TypeSource};

mod compute;
pub mod repo_set;

pub use compute::{branch_change_set, uncommitted_change_set};
pub use repo_set::RepositoryInfo;

// ---------------------------------------------------------------------------
// Typed errors (CHC contract surface)
// ---------------------------------------------------------------------------

pub const ERR_NOT_A_REPO: &str = "not a git repository";
pub const ERR_UNKNOWN_BRANCH: &str = "unknown branch";
pub const ERR_NO_MERGE_BASE: &str = "no merge base";

/// CHC-FR-16: the repository or working tree changed the current change set.
/// Emitted by the watcher, consumed by the Changes panel (CHG-FR-21) and by the
/// status bar's diff summary (`../ui/STB-status-bar.md` STB-FR-27).
pub const CHANGES_UPDATED: &str = "changes-updated";

// ---------------------------------------------------------------------------
// Wire shapes (CHC "Payload shapes")
// ---------------------------------------------------------------------------

/// Which comparison produced a change set. Serialised as an internally-tagged
/// union so the UI can key an open Diff tab on it (CHG-FR-19).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Comparison {
    /// Working tree vs `HEAD` (CHC-FR-03).
    Uncommitted,
    /// Working tree vs the merge-base of the current branch and `target_branch`
    /// (CHC-FR-04).
    #[serde(rename_all = "camelCase")]
    Branch {
        target_branch: String,
        /// The resolved merge-base commit id.
        merge_base: String,
    },
}

/// How Git sees an entry. `Untracked` is what the panel renders under its
/// **Unrevisioned** group (CHG-FR-09); gitignored paths never appear at all
/// (CHC-FR-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

/// Whether an entry is a file or a submodule pointer (CHC-FR-EBLI).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    File,
    Submodule,
}

/// One changed file. `added_lines` / `removed_lines` are `null` for binary
/// content — counts are never fabricated (CHC-FR-08).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    /// The stable path-derived key of ASC-FR-13, identical to the id
    /// `load_project_tree` reports for the same file (CHC-FR-11).
    pub id: String,
    /// Project-relative path at the entry's current location.
    pub path: String,
    /// Basename.
    pub name: String,
    /// The project-relative root of the repository that owns the entry
    /// (CHC-FR-TMNY); `""` for the main repository.
    pub repository: String,
    /// CHC-FR-EBLI: `submodule` for a pointer entry.
    pub entry_kind: EntryKind,
    /// Pointer entries only: the commit the base records. Absent for an added
    /// submodule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_commit: Option<String>,
    /// Pointer entries only: the commit the submodule has checked out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_commit: Option<String>,
    pub change_status: ChangeStatus,
    /// Renamed entries only (CHC-FR-09).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub added_lines: Option<u32>,
    pub removed_lines: Option<u32>,
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<ArtifactType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_source: Option<TypeSource>,
}

/// A comparison plus the flat entry list it produced (CHC-FR-12).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSet {
    pub comparison: Comparison,
    pub entries: Vec<ChangeEntry>,
    /// CHC-FR-TMNY: every member of the repository set, the main repository
    /// first.
    pub repositories: Vec<RepositoryInfo>,
    /// GSM-FR-FRLZ: initialised submodules that could not be opened, with the
    /// cause. The commands log them; they are not part of the payload.
    #[serde(skip)]
    pub unopened: Vec<(String, String)>,
}

/// One candidate comparison target for the panel's picker (CHC-FR-14).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOption {
    pub name: String,
    pub is_current: bool,
    pub is_default: bool,
}

/// Payload of the debounced `"changes updated"` event (CHC-FR-16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesUpdatedPayload {
    pub change_count: usize,
}

/// The summed line counts of the uncommitted change set (CHC-FR-21), which
/// `../ui/STB-status-bar.md` STB-FR-25 renders as its diff summary.
///
/// `file_count` counts every entry, binary included; the two line totals count
/// only the entries that have line counts at all, because none is ever
/// fabricated for binary content (CHC-FR-08).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffTotals {
    pub added_lines: u32,
    pub removed_lines: u32,
    pub file_count: usize,
}

// ---------------------------------------------------------------------------
// Repository resolution (CHC-FR-02)
// ---------------------------------------------------------------------------

/// Open the Git repository that owns the project at `root`, resolved exactly as
/// GTC-FR-02 resolves it: `discover` walks upward from the project root, so a
/// standalone project finds its own repository and a co-located project finds
/// the host code repository. Any failure is the typed
/// `"not a git repository"` error (CHC-FR-02).
pub fn open_repo(root: &Path) -> Result<Repository, String> {
    Repository::discover(root).map_err(|_| ERR_NOT_A_REPO.to_string())
}

/// Forward-slash rendering of a path, dropping non-`Normal` components.
pub fn to_forward(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The project root's location inside the repository working directory, as a
/// forward-slash prefix (empty when the two coincide, the standalone case).
///
/// Git speaks workdir-relative paths; the panel and every `id` in the corpus
/// speak *project*-relative ones (ASC-FR-13). This prefix is what converts
/// between them, and — used as a pathspec — is what keeps a co-located project's
/// change set inside the project the tree is drawn from, so CHC-FR-11's "the
/// same id `load_project_tree` reports" holds in both modes.
pub fn project_prefix(repo: &Repository, root: &Path) -> String {
    let workdir = match repo.workdir() {
        Some(w) => w,
        None => return String::new(), // bare repository: nothing to scope to
    };
    // Both sides are canonicalised before comparison: libgit2 reports a
    // resolved workdir, so a root reached through a symlink (`/tmp` ->
    // `/private/tmp` on macOS) would otherwise never match it, and the silent
    // empty prefix that produced would mis-scope the entire change set.
    let wd = canonicalize_lenient(workdir);
    let rt = canonicalize_lenient(root);
    match rt.strip_prefix(&wd) {
        Ok(rel) => to_forward(rel),
        Err(_) => String::new(),
    }
}

/// Canonicalise `path`, resolving as much of it as exists.
///
/// Plain `canonicalize` fails outright on a path whose leaf is missing, which
/// for [`project_prefix`] is the difference between a correct prefix and a
/// silently empty one. Falling back to the raw path is no better, since it
/// would then be compared against a resolved workdir. So the nearest existing
/// ancestor is resolved and the missing remainder re-appended.
pub fn canonicalize_lenient(path: &Path) -> std::path::PathBuf {
    if let Ok(resolved) = path.canonicalize() {
        return resolved;
    }
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    let mut current = path;
    while let Some(parent) = current.parent() {
        match current.file_name() {
            Some(name) => missing.push(name.to_os_string()),
            None => break,
        }
        if let Ok(resolved) = parent.canonicalize() {
            let mut out = resolved;
            out.extend(missing.iter().rev());
            return out;
        }
        current = parent;
    }
    path.to_path_buf()
}

/// Convert a repository-relative path to a project-relative one, or `None` when
/// it falls outside the project root (pure — the pathspec already filters, this
/// is the belt to its braces).
pub fn to_project_rel(prefix: &str, repo_rel: &str) -> Option<String> {
    if prefix.is_empty() {
        return Some(repo_rel.to_string());
    }
    repo_rel.strip_prefix(prefix).and_then(|rest| rest.strip_prefix('/')).map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Diff collection (CHC-FR-06..FR-09)
// ---------------------------------------------------------------------------

/// Diff options shared by every comparison, with **no** pathspec of their own.
///
/// Untracked files are surfaced with their content so a new file's lines count
/// as additions (CHC-FR-07); ignored files are never requested, so they are
/// absent from every change set (CHC-FR-06).
///
/// Pathspecs are deliberately the caller's business: `DiffOptions::pathspec`
/// *appends*, and libgit2 matches a path against any entry — so an options
/// object that already carried the project prefix would widen, not narrow, a
/// caller that then adds a single file's path.
pub fn diff_options() -> DiffOptions {
    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .include_typechange(true)
        .ignore_submodules(true);
    opts
}

/// Scope a diff to the project when it sits below the repository root. A
/// standalone project (`prefix` empty) adds no pathspec, so the whole
/// repository is in view.
pub fn scope_to_project(opts: &mut DiffOptions, prefix: &str) {
    if !prefix.is_empty() {
        opts.pathspec(prefix);
    }
}

/// Pair renames across a diff (CHC-FR-09).
///
/// `for_untracked` is what makes an *unstaged* rename a rename: without it
/// libgit2 only pairs `Deleted` against `Added`, and a file moved in the editor
/// and not yet staged is `Deleted` + `Untracked` — which would surface as two
/// unrelated rows in the panel that exists to answer "what have I changed right
/// now".
pub fn find_renames(diff: &mut Diff<'_>) -> Result<(), String> {
    let mut find = DiffFindOptions::new();
    find.renames(true).copies(false).for_untracked(true);
    diff.find_similar(Some(&mut find)).map_err(|e| format!("failed to detect renames: {e}"))
}

/// Map a libgit2 delta status onto the contract's `change_status`. Entries that
/// are not changes (unmodified, ignored, unreadable) yield `None` and are
/// dropped (CHC-FR-06).
pub fn map_status(delta: Delta) -> Option<ChangeStatus> {
    match delta {
        Delta::Added => Some(ChangeStatus::Added),
        Delta::Deleted => Some(ChangeStatus::Deleted),
        // A typechange (file -> symlink) and a conflicted path both read as a
        // modification to the panel; neither has a distinct rendering.
        Delta::Modified | Delta::Typechange | Delta::Conflicted => Some(ChangeStatus::Modified),
        // A copy is reported at its new path with its source as `previous_path`,
        // exactly like a rename.
        Delta::Renamed | Delta::Copied => Some(ChangeStatus::Renamed),
        Delta::Untracked => Some(ChangeStatus::Untracked),
        Delta::Unmodified | Delta::Ignored | Delta::Unreadable => None,
    }
}

// ---------------------------------------------------------------------------
// Branch resolution (CHC-FR-04 / FR-05 / FR-13 / FR-14 / FR-15)
// ---------------------------------------------------------------------------

/// Resolve a branch name to its tip commit. Local branches win over
/// remote-tracking ones, so `main` means the local `main` when both exist.
/// Anything that is not a branch is the typed `"unknown branch"` error
/// (CHC-FR-15) — a tag or a raw commit id is not a comparison target the picker
/// can offer, so resolving one would answer a question the panel never asks.
///
/// Public so `crate::git`'s branch-comparison scope resolves its target the same
/// way, keeping a Diff tab and the panel row that opened it on one comparison.
pub fn resolve_branch_commit(repo: &Repository, name: &str) -> Result<Oid, String> {
    for kind in [BranchType::Local, BranchType::Remote] {
        if let Ok(branch) = repo.find_branch(name, kind) {
            if let Some(oid) = branch.get().peel_to_commit().ok().map(|c| c.id()) {
                return Ok(oid);
            }
        }
    }
    Err(ERR_UNKNOWN_BRANCH.to_string())
}

/// The project's **primary remote** — `"origin"` when it exists, otherwise the
/// first remote configured, and `None` when the repository has none.
///
/// `GTC-git.md` GTC-FR-05 makes the primary remote the target of every operation
/// that reaches a forge, and GTC-FR-12's fetch is one of them. One helper rather
/// than the rule restated at each site: two spellings of "which remote" that
/// drift would have a fetch updating one remote's refs while the default branch
/// is read from another's.
pub fn primary_remote_name(repo: &Repository) -> Option<String> {
    let remotes = repo.remotes().ok()?;
    let names: Vec<String> =
        remotes.iter().filter_map(|r| r.ok().flatten().map(|s| s.to_string())).collect();
    names.iter().find(|n| n.as_str() == "origin").or_else(|| names.first()).cloned()
}

/// The repository's default branch (CHC-FR-13): the primary remote's published
/// `HEAD`, else a local `main`, else a local `master`, else the current branch.
pub fn default_branch(repo: &Repository) -> Result<String, String> {
    // (1) The primary remote's published HEAD.
    if let Some(remote) = primary_remote_name(repo) {
        let head_ref = format!("refs/remotes/{remote}/HEAD");
        if let Ok(reference) = repo.find_reference(&head_ref) {
            if let Ok(Some(target)) = reference.symbolic_target() {
                let prefix = format!("refs/remotes/{remote}/");
                if let Some(rest) = target.strip_prefix(&prefix) {
                    if !rest.is_empty() {
                        return Ok(rest.to_string());
                    }
                }
            }
        }
    }
    // (2)/(3) A local `main`, then a local `master`.
    for candidate in ["main", "master"] {
        if repo.find_branch(candidate, BranchType::Local).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    // (4) The current branch. An unborn or detached HEAD has no branch name to
    // offer, so the conventional default stands in rather than erroring — the
    // picker stays populated and the user can choose.
    Ok(current_branch_name(repo).filter(|s| s != "HEAD").unwrap_or_else(|| "main".to_string()))
}

/// The shorthand name of the branch `HEAD` points at, or `None` when `HEAD` is
/// unborn. A detached `HEAD` reports the literal `"HEAD"`, which callers filter.
fn current_branch_name(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    head.shorthand().ok().map(|s| s.to_string())
}

/// Sort key for the picker: local branches first, then remote-tracking ones,
/// each alphabetical. Pure so the ordering is pinned by a test.
fn branch_sort_key(name: &str, remote: bool) -> (u8, String) {
    (u8::from(remote), name.to_string())
}

/// CHC-FR-14: the local and remote-tracking branches available as comparison
/// targets, each flagged `is_current` and `is_default`.
pub fn comparison_branches(root: &Path) -> Result<Vec<BranchOption>, String> {
    let repo = open_repo(root)?;
    let default = default_branch(&repo).unwrap_or_default();
    // A detached HEAD is on no branch, so nothing in the picker is "current".
    let current = repo
        .head()
        .ok()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().ok().map(|s| s.to_string()));

    let branches = repo.branches(None).map_err(|e| format!("failed to list branches: {e}"))?;
    let mut rows: Vec<((u8, String), BranchOption)> = Vec::new();
    for item in branches {
        let Ok((branch, kind)) = item else { continue };
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        // `origin/HEAD` is a symbolic pointer, not a comparison target.
        if name.ends_with("/HEAD") {
            continue;
        }
        let remote = kind == BranchType::Remote;
        let option = BranchOption {
            is_current: !remote && current.as_deref() == Some(name),
            is_default: name == default,
            name: name.to_string(),
        };
        rows.push((branch_sort_key(name, remote), option));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows.into_iter().map(|(_, o)| o).collect())
}

/// CHC-FR-21 (pure): sum a change set's per-entry line counts.
///
/// Deliberately derived from the entries rather than from a second diff walk,
/// so the totals and the list can never disagree about the same working tree —
/// which is exactly what CHC-FR-21 asserts. `saturating_add` keeps a
/// pathological repository from panicking a read-only query.
pub fn totals_of(set: &ChangeSet) -> DiffTotals {
    let mut totals = DiffTotals { file_count: set.entries.len(), ..DiffTotals::default() };
    for entry in &set.entries {
        totals.added_lines = totals.added_lines.saturating_add(entry.added_lines.unwrap_or(0));
        totals.removed_lines =
            totals.removed_lines.saturating_add(entry.removed_lines.unwrap_or(0));
    }
    totals
}

/// CHC-FR-21 / CHC-FR-22: the added and removed line totals of the uncommitted
/// change set, plus how many files it holds. A standalone read — it neither
/// requires nor is affected by any prior `list_uncommitted_changes` call — and
/// read-only under CHC-FR-18 like the rest of the surface.
pub fn uncommitted_diff_totals(root: &crate::fs::RootFs) -> Result<DiffTotals, String> {
    Ok(totals_of(&uncommitted_change_set(root)?))
}

// ---------------------------------------------------------------------------
// Watch surface (CHC-FR-16)
// ---------------------------------------------------------------------------

/// True for a path *inside the Git directory* whose change could alter the
/// current change set: `HEAD`, the index, and refs (CHC-FR-16). `rel` is
/// relative to the Git directory itself, not to the project root — in
/// co-located mode the two are not the same place.
///
/// Lock files are the transient scaffolding of a Git operation, not its result,
/// so they are ignored — the real ref/index write that follows is what fires the
/// event. Object writes are likewise covered by the ref update that follows.
///
/// Pure, so the exact watch surface is unit-testable without a filesystem.
pub fn is_git_meta_rel(rel: &str) -> bool {
    if rel.ends_with(".lock") {
        return false;
    }
    rel == "HEAD"
        || rel == "ORIG_HEAD"
        || rel == "MERGE_HEAD"
        || rel == "index"
        || rel == "packed-refs"
        || rel.starts_with("refs/")
}

/// CHC-FR-EXTM: true for a path inside the Git directory of a submodule, which
/// the parent keeps below `modules/<name>/`, whose change could alter the
/// change set. A submodule name can hold `/`, so the path matches on what it
/// ends with, by the rules of [`is_git_meta_rel`].
pub fn is_submodule_git_meta_rel(rel: &str) -> bool {
    let Some(rest) = rel.strip_prefix("modules/") else {
        return false;
    };
    if rest.ends_with(".lock") {
        return false;
    }
    ["/HEAD", "/ORIG_HEAD", "/MERGE_HEAD", "/index", "/packed-refs"]
        .iter()
        .any(|leaf| rest.ends_with(leaf))
        || (rest.contains("/refs/") && !rest.contains("/logs/"))
}

/// CHC-FR-EXTM: the last `.git` folder of `path` and the part of `path` after
/// it, when there is one: the path inside the Git directory of a checkout that
/// holds it.
fn embedded_git_rel(path: &Path) -> Option<(std::path::PathBuf, String)> {
    let parts: Vec<_> = path.components().collect();
    let at = parts.iter().rposition(|c| c.as_os_str() == ".git")?;
    let dot_git: std::path::PathBuf = parts[..=at].iter().collect();
    let rest: std::path::PathBuf = parts[at + 1..].iter().collect();
    Some((dot_git, to_forward(&rest)))
}

/// The Git directory the watcher must observe in addition to the project root,
/// or `None` when watching the root already covers it.
///
/// A standalone project holds its own `.git` inside the watched tree, so a
/// second watch would only duplicate events. A co-located project's repository
/// lives *above* the project root, so without this its commits, checkouts and
/// index writes would reach no watcher at all and the panel would sit on a stale
/// change set until manually refreshed (CHC-FR-16).
pub fn git_dir_to_watch(git_dir: &Path, root: &Path) -> Option<std::path::PathBuf> {
    let gd = canonicalize_lenient(git_dir);
    let rt = canonicalize_lenient(root);
    if gd.starts_with(&rt) {
        None
    } else {
        Some(gd)
    }
}

/// How many changes in a debounced watcher batch could alter the current change
/// set (CHC-FR-16): every working-tree path in `worktree` — a content edit
/// changes a diff even though it leaves the tree shape alone — plus every
/// Git-directory path from [`is_git_meta_rel`]. Zero means no
/// `"changes updated"` event is due.
///
/// `worktree` is the batch's working-tree paths with the `.gitignore`d ones
/// already removed (`scanning::drop_gitignored`): an ignored path cannot alter
/// the change set (CHC-FR-AADZ). The watcher filters the batch once and hands
/// the same list to every consumer.
///
/// `git_dir` is the repository's Git directory, wherever it lives. Passing
/// `None` (no repository) counts working-tree changes only.
pub fn changes_change_count(
    worktree: &[String],
    result: &DebounceEventResult,
    git_dir: Option<&Path>,
) -> usize {
    let worktree = worktree.len();
    let (Ok(events), Some(git_dir)) = (result, git_dir) else {
        return worktree;
    };
    let mut seen = BTreeSet::new();
    for ev in events {
        let Ok(rel) = ev.path.strip_prefix(git_dir) else {
            // CHC-FR-EXTM: a submodule that keeps its own `.git` folder in its
            // checkout writes its metadata there.
            // The `.git` folder of the repository itself, which a linked
            // worktree's Git directory is below, is not such a checkout.
            if let Some((dot_git, rel)) = embedded_git_rel(&ev.path) {
                if !git_dir.starts_with(&dot_git) && is_git_meta_rel(&rel) {
                    seen.insert(to_forward(&ev.path));
                }
            }
            continue;
        };
        let rel = to_forward(rel);
        if is_git_meta_rel(&rel) || is_submodule_git_meta_rel(&rel) {
            seen.insert(rel);
        }
    }
    worktree + seen.len()
}

/// CHC-FR-VHAA: how many ref changes in a batch come from the common Git
/// directory of a linked worktree. Only `refs/` and `packed-refs` count: the
/// `HEAD` and `index` there belong to the main worktree, and the Git directory
/// of each linked worktree (`worktrees/<name>/`) is counted by
/// [`changes_change_count`] or not at all.
pub fn common_dir_change_count(result: &DebounceEventResult, common_dir: Option<&Path>) -> usize {
    let (Ok(events), Some(common_dir)) = (result, common_dir) else {
        return 0;
    };
    let mut seen = BTreeSet::new();
    for ev in events {
        let Ok(rel) = ev.path.strip_prefix(common_dir) else {
            continue;
        };
        let rel = to_forward(rel);
        if is_git_meta_rel(&rel) && (rel == "packed-refs" || rel.starts_with("refs/")) {
            seen.insert(rel);
        }
    }
    seen.len()
}

// ---------------------------------------------------------------------------
// Tauri commands (CHC contract surface)
// ---------------------------------------------------------------------------

/// CHC-FR-03. Change-set computation is one of the operations that attribute
/// progress (`../core/PRG-progress-reporting.md` PRG-FR-11): on a large working
/// tree the diff walk is long enough to be worth showing, and on a small one it
/// registers and terminates within the call, which is exactly what PRG-FR-06
/// prescribes for work that finishes quickly.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn list_uncommitted_changes(
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
    progress: crate::project_scope::Scoped<'_, ProgressRegistry>,
) -> Result<ChangeSet, String> {
    let root = project.require_root()?;
    progress::attribute(
        &app,
        &progress,
        "changes",
        "Computing changes…",
        Some(root.to_path_buf()),
        || uncommitted_change_set(&root),
    )
    .inspect(|set| log_unopened(&app, set))
}

/// GSM-FR-FRLZ: one `WARN` record for each submodule that could not be opened.
fn log_unopened(app: &tauri::AppHandle, set: &ChangeSet) {
    for (path, cause) in &set.unopened {
        crate::logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "a submodule could not be opened",
            crate::log_fields! { "repository" => path, "cause" => cause },
        );
    }
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn list_branch_changes(
    target_branch: String,
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
    progress: crate::project_scope::Scoped<'_, ProgressRegistry>,
) -> Result<ChangeSet, String> {
    let root = project.require_root()?;
    progress::attribute(
        &app,
        &progress,
        "changes",
        "Computing changes…",
        Some(root.to_path_buf()),
        || branch_change_set(&root, &target_branch),
    )
    .inspect(|set| log_unopened(&app, set))
}

/// CHC-FR-21 / CHC-FR-22: the diff summary `../ui/STB-status-bar.md` STB-FR-25
/// renders. Independent of the Changes panel — it holds no change set of its own
/// and needs none to have been loaded.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn get_uncommitted_diff_totals(
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
    progress: crate::project_scope::Scoped<'_, ProgressRegistry>,
) -> Result<DiffTotals, String> {
    let root = project.require_root()?;
    progress::attribute(
        &app,
        &progress,
        "changes",
        "Computing diff totals…",
        Some(root.to_path_buf()),
        || uncommitted_diff_totals(&root),
    )
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn get_default_branch(
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<String, String> {
    let root = project.require_root()?;
    let repo = open_repo(&root)?;
    default_branch(&repo)
}

#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn list_comparison_branches(
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<Vec<BranchOption>, String> {
    let root = project.require_root()?;
    comparison_branches(&root)
}

/// Test helpers shared with `crate::git`'s tests.
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use crate::fs as fsa;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// Content hash of every file under `root`, `.git` included, so a write to
    /// the index, a ref, or the working tree is caught. This is what makes the
    /// read-only guarantee (CHC-FR-18 / CHG-FR-24) provable rather than
    /// asserted: it does not rely on knowing which files Git might touch.
    pub fn snapshot(root: PathBuf) -> BTreeMap<String, String> {
        fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    walk(&path, base, out);
                } else if let Ok(bytes) = std::fs::read(&path) {
                    let rel = path.strip_prefix(base).unwrap().to_string_lossy().to_string();
                    out.insert(rel, fsa::sha256_bytes(&bytes));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&root, &root, &mut out);
        out
    }
}

#[cfg(test)]
mod tests;
