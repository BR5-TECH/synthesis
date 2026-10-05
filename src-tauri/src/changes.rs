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

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use git2::{BranchType, Delta, Diff, DiffFindOptions, DiffOptions, Oid, Repository};
use notify_debouncer_mini::DebounceEventResult;
use serde::Serialize;
use tauri::State;

use crate::progress::{self, ProgressRegistry};
use crate::project::ProjectState;
use crate::scanning::{self, ArtifactType, TypeSource};

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
    repo_rel
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(|s| s.to_string())
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
    diff.find_similar(Some(&mut find))
        .map_err(|e| format!("failed to detect renames: {e}"))
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

/// A change as Git sees it, before project-relative mapping and classification.
#[derive(Clone, Debug, PartialEq)]
struct RawChange {
    /// Repository-relative current path.
    path: String,
    /// Repository-relative pre-rename path (renames/copies only).
    previous_path: Option<String>,
    status: ChangeStatus,
    added: u32,
    removed: u32,
    is_binary: bool,
}

/// Walk a diff into per-file changes with line counts.
///
/// Rename detection runs first (CHC-FR-09), then a single `foreach` pass
/// accumulates insertions and deletions per path. Binary deltas produce no line
/// callbacks, so their counts stay unset and are reported as `null`
/// (CHC-FR-08). The counting rule of CHC-FR-07 falls out of the diff itself: an
/// added or untracked file is all `+`, a deleted file is all `-`, and a modified
/// or renamed one is its unified diff against the comparison base.
fn collect_changes(diff: &mut Diff<'_>) -> Result<Vec<RawChange>, String> {
    find_renames(diff)?;

    // Both callbacks need to write; `foreach` hands each out as a separate
    // `&mut dyn FnMut`, so the shared accumulators go behind `RefCell`.
    let stats: RefCell<BTreeMap<String, (u32, u32)>> = RefCell::new(BTreeMap::new());
    let binary: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());

    let key_of = |delta: &git2::DiffDelta<'_>| -> Option<String> {
        delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(to_forward)
    };

    {
        let mut file_cb = |_delta: git2::DiffDelta<'_>, _progress: f32| true;
        let mut binary_cb = |delta: git2::DiffDelta<'_>, _bin: git2::DiffBinary<'_>| {
            if let Some(k) = key_of(&delta) {
                binary.borrow_mut().insert(k);
            }
            true
        };
        let mut line_cb = |delta: git2::DiffDelta<'_>,
                           _hunk: Option<git2::DiffHunk<'_>>,
                           line: git2::DiffLine<'_>| {
            let Some(k) = key_of(&delta) else { return true };
            let mut stats = stats.borrow_mut();
            let counts = stats.entry(k).or_insert((0, 0));
            match line.origin() {
                '+' => counts.0 += 1,
                '-' => counts.1 += 1,
                _ => {}
            }
            true
        };
        diff.foreach(
            &mut file_cb,
            Some(&mut binary_cb),
            None,
            Some(&mut line_cb),
        )
        .map_err(|e| format!("failed to read diff: {e}"))?;
    }

    let stats = stats.into_inner();
    let binary = binary.into_inner();

    let mut out = Vec::new();
    for delta in diff.deltas() {
        let Some(status) = map_status(delta.status()) else {
            continue;
        };
        let Some(path) = key_of(&delta) else { continue };
        let previous_path = if status == ChangeStatus::Renamed {
            delta.old_file().path().map(to_forward).filter(|p| *p != path)
        } else {
            None
        };
        // `foreach` populates the delta's BINARY flag while generating each
        // patch; the binary callback above is the belt to that brace.
        let is_binary = binary.contains(&path)
            || delta.flags().is_binary()
            || delta.new_file().is_binary()
            || delta.old_file().is_binary();
        let (added, removed) = stats.get(&path).copied().unwrap_or((0, 0));
        out.push(RawChange {
            path,
            previous_path,
            status,
            added,
            removed,
            is_binary,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Classification (CHC-FR-10 / CHC-FR-11)
// ---------------------------------------------------------------------------

/// Turn raw Git changes into contract entries: map every path into the project's
/// coordinate system, classify it with the Library's own classifier so a changed
/// file carries exactly the type the Library shows (CHC-FR-10), and key it by
/// the stable id of ASC-FR-13 (CHC-FR-11).
///
/// A deleted path is classified from path convention and stored assignments
/// only — the content tiebreak of ASC-FR-04 needs a file that is no longer on
/// disk, so no content read is attempted for it (CHC-FR-10).
fn to_entries(root: &crate::fs::RootFs, prefix: &str, raw: Vec<RawChange>) -> Vec<ChangeEntry> {
    let assignments = scanning::load_assignments(root);

    // Project-relative paths whose file is gone, so the content tiebreak is
    // skipped for them rather than attempted and failed.
    let deleted: BTreeSet<String> = raw
        .iter()
        .filter(|r| r.status == ChangeStatus::Deleted)
        .filter_map(|r| to_project_rel(prefix, &r.path))
        .collect();

    let root_owned = root.to_path_buf();
    let content = move |rel: &str| -> scanning::ContentFacts {
        if deleted.contains(rel) {
            return scanning::ContentFacts::default();
        }
        if !rel.to_ascii_lowercase().ends_with(".md") {
            return scanning::ContentFacts::default();
        }
        let path = root_owned.join(rel);
        if !path.is_file() {
            return scanning::ContentFacts::default();
        }
        match root.read_text(path) {
            Ok(text) => scanning::read_content_facts(&text),
            Err(_) => scanning::ContentFacts::default(),
        }
    };

    let mut entries: Vec<ChangeEntry> = raw
        .into_iter()
        .filter_map(|r| {
            let path = to_project_rel(prefix, &r.path)?;
            let previous_path = r
                .previous_path
                .as_deref()
                .and_then(|p| to_project_rel(prefix, p));
            let (artifact_type, type_source) =
                scanning::classify_file(&path, &assignments, &content);
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            Some(ChangeEntry {
                id: path.clone(),
                name,
                change_status: r.status,
                previous_path,
                added_lines: if r.is_binary { None } else { Some(r.added) },
                removed_lines: if r.is_binary { None } else { Some(r.removed) },
                is_binary: r.is_binary,
                artifact_type,
                type_source,
                path,
            })
        })
        .collect();
    // Flat and deterministic (CHC-FR-12): no grouping, no untracked separation.
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries
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

/// The commit the working tree sits on. A detached `HEAD` resolves here exactly
/// like an attached one — the checked-out commit stands in for the branch tip
/// when computing the merge-base (CHC-FR-05).
fn head_commit(repo: &Repository) -> Option<Oid> {
    repo.head().ok()?.peel_to_commit().ok().map(|c| c.id())
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
    let names: Vec<String> = remotes
        .iter()
        .filter_map(|r| r.ok().flatten().map(|s| s.to_string()))
        .collect();
    names
        .iter()
        .find(|n| n.as_str() == "origin")
        .or_else(|| names.first())
        .cloned()
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
    Ok(current_branch_name(repo)
        .filter(|s| s != "HEAD")
        .unwrap_or_else(|| "main".to_string()))
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

// ---------------------------------------------------------------------------
// Change-set computation
// ---------------------------------------------------------------------------

/// CHC-FR-03: the working tree against `HEAD`. Staged and unstaged
/// modifications are both included and reported identically — the returned
/// entries carry no index state, because the consuming panel is read-only.
pub fn uncommitted_change_set(root: &crate::fs::RootFs) -> Result<ChangeSet, String> {
    let repo = open_repo(root)?;
    let prefix = project_prefix(&repo, root);
    let mut opts = diff_options();
    scope_to_project(&mut opts, &prefix);
    // An unborn HEAD (a repository with no commits) has no tree; diffing
    // against `None` reports the whole working tree as new, which is correct.
    let tree = head_commit(&repo)
        .and_then(|oid| repo.find_commit(oid).ok())
        .and_then(|c| c.tree().ok());
    let mut diff = repo
        .diff_tree_to_workdir_with_index(tree.as_ref(), Some(&mut opts))
        .map_err(|e| format!("failed to diff working tree: {e}"))?;
    let raw = collect_changes(&mut diff)?;
    Ok(ChangeSet {
        comparison: Comparison::Uncommitted,
        entries: to_entries(root, &prefix, raw),
    })
}

/// CHC-FR-04: the working tree against the merge-base of the current branch and
/// `target_branch`. The result therefore holds every change the current branch
/// introduces — committed and uncommitted alike — and nothing from commits that
/// landed on `target_branch` after that merge-base.
pub fn branch_change_set(root: &crate::fs::RootFs, target_branch: &str) -> Result<ChangeSet, String> {
    let repo = open_repo(root)?;
    let prefix = project_prefix(&repo, root);
    let target = resolve_branch_commit(&repo, target_branch)?;
    let head = head_commit(&repo).ok_or_else(|| ERR_NO_MERGE_BASE.to_string())?;
    let base = repo
        .merge_base(head, target)
        .map_err(|_| ERR_NO_MERGE_BASE.to_string())?;
    let base_tree = repo
        .find_commit(base)
        .and_then(|c| c.tree())
        .map_err(|_| ERR_NO_MERGE_BASE.to_string())?;
    let mut opts = diff_options();
    scope_to_project(&mut opts, &prefix);
    let mut diff = repo
        .diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))
        .map_err(|e| format!("failed to diff against merge base: {e}"))?;
    let raw = collect_changes(&mut diff)?;
    Ok(ChangeSet {
        comparison: Comparison::Branch {
            target_branch: target_branch.to_string(),
            merge_base: base.to_string(),
        },
        entries: to_entries(root, &prefix, raw),
    })
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

    let branches = repo
        .branches(None)
        .map_err(|e| format!("failed to list branches: {e}"))?;
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
    let mut totals = DiffTotals {
        file_count: set.entries.len(),
        ..DiffTotals::default()
    };
    for entry in &set.entries {
        totals.added_lines = totals
            .added_lines
            .saturating_add(entry.added_lines.unwrap_or(0));
        totals.removed_lines = totals
            .removed_lines
            .saturating_add(entry.removed_lines.unwrap_or(0));
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
/// set (CHC-FR-16): every working-tree path the scanner surfaces — a content
/// edit changes a diff even though it leaves the tree shape alone — plus every
/// Git-directory path from [`is_git_meta_rel`]. Zero means no
/// `"changes updated"` event is due.
///
/// `git_dir` is the repository's Git directory, wherever it lives. Passing
/// `None` (no repository) counts working-tree changes only.
pub fn changes_change_count(
    result: &DebounceEventResult,
    root: &Path,
    git_dir: Option<&Path>,
) -> usize {
    let worktree = scanning::changed_rel_paths(result, root).len();
    let (Ok(events), Some(git_dir)) = (result, git_dir) else {
        return worktree;
    };
    let mut seen = BTreeSet::new();
    for ev in events {
        let Ok(rel) = ev.path.strip_prefix(git_dir) else {
            continue;
        };
        let rel = to_forward(rel);
        if is_git_meta_rel(&rel) {
            seen.insert(rel);
        }
    }
    worktree + seen.len()
}

// ---------------------------------------------------------------------------
// Tauri commands (CHC contract surface)
// ---------------------------------------------------------------------------

/// CHC-FR-03. Change-set computation is one of the operations that attribute
/// progress (`../core/PRG-progress-reporting.md` PRG-FR-11): on a large working
/// tree the diff walk is long enough to be worth showing, and on a small one it
/// registers and terminates within the call, which is exactly what PRG-FR-06
/// prescribes for work that finishes quickly.
#[tauri::command]
pub fn list_uncommitted_changes(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    progress: State<'_, ProgressRegistry>,
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
}

#[tauri::command]
pub fn list_branch_changes(
    target_branch: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    progress: State<'_, ProgressRegistry>,
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
}

/// CHC-FR-21 / CHC-FR-22: the diff summary `../ui/STB-status-bar.md` STB-FR-25
/// renders. Independent of the Changes panel — it holds no change set of its own
/// and needs none to have been loaded.
#[tauri::command]
pub fn get_uncommitted_diff_totals(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    progress: State<'_, ProgressRegistry>,
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

#[tauri::command]
pub fn get_default_branch(project: State<'_, ProjectState>) -> Result<String, String> {
    let root = project.require_root()?;
    let repo = open_repo(&root)?;
    default_branch(&repo)
}

#[tauri::command]
pub fn list_comparison_branches(
    project: State<'_, ProjectState>,
) -> Result<Vec<BranchOption>, String> {
    let root = project.require_root()?;
    comparison_branches(&root)
}

/// Test helpers shared with `crate::git`'s tests.
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use crate::fs as fsa;
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
