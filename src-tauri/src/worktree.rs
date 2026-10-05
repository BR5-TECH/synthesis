//! Worktree context — the backend half of `../ui/WTS-worktree-selector.md`,
//! specified by `specifications/core/WTC-worktree-context.md`.
//!
//! A project *is* a Git repository, and at any moment exactly one of that
//! repository's worktrees is the project's **content root** (WTC-FR-03). This
//! module enumerates the repository's worktrees and the branches that have none
//! (WTC-FR-04 / WTC-FR-06), brings that enumeration up to date against the
//! remote on request (WTC-FR-22), performs the three ways of changing the active
//! worktree — activate an existing one, check a branch out in place, create a
//! new one — and owns the re-rooting that follows: the scan, the watchers, and
//! the `.synthesis/` reads all remount on the new content root
//! (WTC-FR-08 / WTC-FR-10 / WTC-FR-14).
//!
//! A refresh is the one operation here that is *not* a switch: it reads what the
//! repository holds rather than moving the project onto a different checkout, so
//! it re-roots nothing and emits `"branches changed"` rather than
//! `"worktree context changed"` (WTC-FR-24 / WTC-FR-25).
//!
//! No command here removes a worktree, deletes a branch, or rewrites history
//! (WTC-FR-19): every mutation it exposes is additive. Nor does it move a
//! linked worktree onto a different branch (WTC-FR-21). The removal primitive
//! `remove_linked_worktree` (WTC-FR-RDVK) is composed by `GTC-git.md` only.
//!
//! The pure halves take a `&Path` content root and are unit-tested against real
//! temporary repositories without a Tauri runtime; the `#[tauri::command]`
//! wrappers thread the managed state, persist the choice (WTC-FR-17), and emit
//! `"worktree context changed"` (WTC-FR-16).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use git2::{BranchType, Repository};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::changes::{canonicalize_lenient, open_repo, ERR_NOT_A_REPO, ERR_UNKNOWN_BRANCH};
use crate::global_settings::GlobalSettingsStore;
use crate::project::{self, ProjectState};
use crate::watcher::ProjectWatcher;

// ---------------------------------------------------------------------------
// Typed errors (WTC contract surface)
// ---------------------------------------------------------------------------

pub const ERR_NOT_A_WORKTREE: &str = "not a worktree";
/// WTC-FR-JXBM: a work stream a graduation run is holding.
pub const ERR_WORK_STREAM_BUSY: &str = "work stream busy";
/// WTC-FR-FBJQ: a direct graduation run is working in a worktree of the project.
pub const ERR_DIRECT_GRADUATION_ACTIVE: &str = "direct graduation active";
pub const ERR_WORKTREE_MISSING: &str = "worktree missing";
pub const ERR_BRANCH_ALREADY_CHECKED_OUT: &str = "branch already checked out";
/// Git refused the checkout because working-tree modifications would be
/// overwritten. Raised by `GTC-git.md`'s `checkout_branch` primitive
/// (GTC-FR-08) and surfaced here (WTC-FR-12).
pub const ERR_CHECKOUT_BLOCKED: &str = "checkout blocked by local changes";
/// The active worktree is a linked one, and a branch is checked out only in the
/// repository's primary worktree (WTC-FR-21).
pub const ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE: &str =
    "checkout not allowed in a linked worktree";
pub const ERR_WORKTREE_PATH_EXISTS: &str = "worktree path exists";

/// The `"worktree context changed"` channel (WTC-FR-16), as a wire name.
///
/// Tauri validates event names and accepts only alphanumerics, `-`, `/`, `:`,
/// and `_` — a name with spaces is rejected outright, and `Emitter::emit`
/// returns `IllegalEventName` rather than delivering anything. So the spec's
/// abstract channel name is carried on the wire in kebab-case. Matches the
/// literal in `src/events.ts` byte-for-byte.
pub const WORKTREE_CONTEXT_CHANGED: &str = "worktree-context-changed";

/// The `"branches changed"` channel (WTC-FR-25), as a wire name — kebab-case for
/// the same reason as the channel above. Matches the literal in `src/events.ts`
/// byte-for-byte.
///
/// Announces only that the repository's branch **set** may differ from what a
/// consumer holds. It never implies the active checkout moved, which is what
/// `WORKTREE_CONTEXT_CHANGED` announces — the two are separate because a refresh
/// changes the first without touching the second (WTC-FR-24).
pub const BRANCHES_CHANGED: &str = "branches-changed";

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

/// One of the repository's worktrees (WTC contract surface).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WorktreeEntry {
    /// Absolute path of the worktree directory.
    pub path: String,
    /// Display label — the directory's basename.
    pub name: String,
    /// Checked-out branch; absent when `is_detached` (WTC-FR-05).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Abbreviated commit id at `HEAD`. Empty when it cannot be resolved (an
    /// unborn `HEAD`, or a worktree whose directory is gone).
    pub head_short_hash: String,
    pub is_detached: bool,
    /// Rooting the project right now.
    pub is_active: bool,
    /// The repository's non-linked worktree.
    pub is_primary: bool,
    /// Git lists it but its directory is gone (WTC-FR-05).
    pub is_missing: bool,
    /// WTC-FR-QKZD: the work stream this worktree is the working copy of, when
    /// it is one. A worktree that is no stream's carries none of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<WorktreeStream>,
}

/// WTC-FR-QKZD: what a worktree that is a work stream's carries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WorktreeStream {
    pub stream_id: String,
    pub stream_name: String,
    /// The run holding the stream, or `None` (`WKS-work-streams.md` WKS-FR-CYAG).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy_run_id: Option<String>,
}

/// A branch with no worktree (WTC-FR-06).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BranchEntry {
    /// Local branch name, or the remote-tracking ref's short name
    /// (e.g. `origin/experiment`).
    pub name: String,
    /// `"local"` or `"remote"`.
    pub kind: String,
    /// Upstream ref, when a local branch has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    pub head_short_hash: String,
}

pub const BRANCH_KIND_LOCAL: &str = "local";
pub const BRANCH_KIND_REMOTE: &str = "remote";

/// The whole picture the worktree selector renders from (WTC-FR-04).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WorktreeContext {
    /// Absolute path of the repository's primary worktree.
    pub repository_root: String,
    /// Absolute path of the worktree currently rooting the project.
    pub active_worktree_path: String,
    pub worktrees: Vec<WorktreeEntry>,
    /// Only branches with no worktree.
    pub branches: Vec<BranchEntry>,
}

/// Payload of `"worktree context changed"` (WTC-FR-16).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WorktreeContextChangedPayload {
    pub active_worktree_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub is_detached: bool,
}

/// WTC-FR-25: tell every consumer the branch set may differ from what it holds,
/// after `cause` (a branch deletion, a work stream's creation or deletion)
/// changed it. The change is done; an undelivered announcement leaves the Git
/// panel's branch listing stale, so it is recorded rather than returned.
pub(crate) fn announce_branches_changed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    cause: &str,
) {
    let repository_root = open_repo(root)
        .ok()
        .and_then(|repo| primary_worktree_root(&repo))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let payload = BranchesChangedPayload { repository_root };
    if let Err(e) = app.emit(BRANCHES_CHANGED, payload) {
        crate::logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "branches-changed event was not delivered",
            crate::log_fields! { "cause" => cause, "error" => e.to_string() },
        );
    }
}

/// Payload of `"branches changed"` (WTC-FR-25).
///
/// Deliberately a signal rather than a data carrier: it names the repository and
/// nothing else, so each consumer reloads *its own* listing. The selector reads a
/// `WorktreeContext` and the Git panel reads `list_branches` — two different
/// shapes — and a branch set on this event would have to be one of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BranchesChangedPayload {
    pub repository_root: String,
}

/// How the remote half of a refresh ended (WTC-FR-23).
pub const REMOTE_REFRESHED: &str = "refreshed";
/// There was no remote half to run — the project has no primary remote.
pub const REMOTE_SKIPPED: &str = "skipped";
/// The remote half ran and failed; `remote_error` carries the typed cause.
pub const REMOTE_FAILED: &str = "failed";

/// What `refresh_worktrees_and_branches` answers with (WTC contract surface).
///
/// The local half and the remote half are reported separately on purpose
/// (WTC-FR-23): `context` is always a fresh enumeration of what is on disk, so a
/// refresh that could not reach the remote still tells the caller about a branch
/// created or a worktree added since it last looked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RefreshOutcome {
    /// A freshly enumerated context; always present (WTC-FR-23).
    pub context: WorktreeContext,
    /// `"refreshed"` | `"skipped"` | `"failed"`.
    pub remote_state: String,
    /// The typed error, when `remote_state` is `"failed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_error: Option<String>,
}

impl WorktreeContextChangedPayload {
    fn from_entry(entry: &WorktreeEntry) -> Self {
        WorktreeContextChangedPayload {
            active_worktree_path: entry.path.clone(),
            branch: entry.branch.clone(),
            is_detached: entry.is_detached,
        }
    }
}

// ---------------------------------------------------------------------------
// Repository / worktree resolution (pure over a content root)
// ---------------------------------------------------------------------------

fn to_string_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Absolute path of the repository's **primary** worktree — the non-linked
/// checkout. For a linked worktree, libgit2's `commondir` is the primary's
/// `.git` directory, so its parent is the primary worktree itself.
///
/// `None` for a bare repository, which has no working directory at all.
pub fn primary_worktree_root(repo: &Repository) -> Option<PathBuf> {
    if !repo.is_worktree() {
        return repo.workdir().map(|p| canonicalize_lenient(p));
    }
    let common = repo.commondir();
    // `commondir` may carry a trailing separator; `parent()` on `<root>/.git/`
    // yields `<root>/.git`, so strip it first.
    let trimmed = common
        .to_str()
        .map(|s| s.trim_end_matches(['/', '\\']))
        .map(PathBuf::from)
        .unwrap_or_else(|| common.to_path_buf());
    trimmed.parent().map(canonicalize_lenient)
}

/// The identity anchor of the project rooted at `root` (WTC-FR-18 /
/// `GSS-global-settings-storage.md` GSS-FR-18): the repository's primary
/// worktree when `root` is inside a repository, and `root` itself when it is
/// not. This is what the recent-projects list records and what keys the
/// user-global per-project slot, so a repository with several worktrees is one
/// project rather than many.
///
/// A project outside a repository anchors on the path it was opened at,
/// verbatim — resolving symlinks there would change what the picker and the
/// recents list display for no gain, since there is no second worktree to
/// reconcile it against.
pub fn project_anchor(root: &Path) -> PathBuf {
    open_repo(root)
        .ok()
        .and_then(|repo| primary_worktree_root(&repo))
        .unwrap_or_else(|| root.to_path_buf())
}

/// The worktree directory **containing** `active_root`.
///
/// The project's content root is not always the worktree directory itself: a
/// co-located project sits in a subdirectory of the checkout it belongs to
/// (`GTC-git.md` GTC-FR-02), which is exactly what `changes::project_prefix`
/// already scopes diffs by. libgit2's `workdir` for a repository discovered
/// from a nested path is that containing worktree, so this is the one place the
/// distinction has to be resolved — everything downstream compares worktrees to
/// worktrees.
fn containing_worktree(repo: &Repository) -> Option<PathBuf> {
    repo.workdir().map(canonicalize_lenient)
}

/// Where a content root sits *inside* its worktree, as a relative path.
///
/// Empty for a standalone project, whose content root is the worktree itself.
/// Preserved across a switch (WTC-FR-08) so a co-located project lands in the
/// corresponding subdirectory of the new checkout rather than being silently
/// relocated to that checkout's root.
fn root_offset(repo: &Repository, active_root: &Path) -> PathBuf {
    let Some(worktree) = containing_worktree(repo) else {
        return PathBuf::new();
    };
    canonicalize_lenient(active_root)
        .strip_prefix(&worktree)
        .map(|rel| rel.to_path_buf())
        .unwrap_or_default()
}

/// The content root a switch to `worktree` should land on, given where the
/// current content root sits inside its own worktree (WTC-FR-08).
pub fn content_root_in(worktree: &Path, offset: &Path) -> PathBuf {
    if offset.as_os_str().is_empty() {
        worktree.to_path_buf()
    } else {
        worktree.join(offset)
    }
}

/// The admin directory Git keeps for a linked worktree
/// (`<commondir>/worktrees/<name>`), which still holds its `HEAD` after the
/// worktree's own directory has been deleted.
fn linked_admin_dir(repo: &Repository, name: &str) -> PathBuf {
    repo.commondir().join("worktrees").join(name)
}

/// WTC-FR-QVNL: make a worktree this application created record the store it
/// shares as a path **relative** to its own Git directory.
///
/// libgit2 writes that record — the `commondir` file in the worktree's
/// administrative directory — as the absolute host path, where the `git`
/// command line writes the relative form. Git reads it before it reads
/// anything else, so an absolute host path fails the first Git command of any
/// reader that reaches the store somewhere else. Every agent turn is such a
/// reader: it reads the run's repository inside a container that mounts the
/// store at a path of the executor's own (`../tools/EAC-execute-agent-cli.md`
/// EAC-FR-FNFV). The relative form resolves for both.
///
/// FSA-FR-19 exception, on the terms `branch_from_head_file` sets: this is
/// Git's own administrative directory, and for a linked worktree it sits
/// outside the root the shared instance allowlists. The write is still a
/// guarded one — through an instance rooted at that directory alone.
pub(crate) fn normalize_worktree_commondir(main: &Repository, name: &str) -> Result<(), String> {
    // The Git directory of a linked worktree is `<store>/worktrees/<name>`, so
    // the store is two levels above it. A name holding a separator would put it
    // somewhere else — and Git registers no such name — so it is refused here
    // rather than recorded wrongly.
    let mut segments = Path::new(name).components();
    let one_ordinary_segment = matches!(segments.next(), Some(std::path::Component::Normal(_)))
        && segments.next().is_none();
    if !one_ordinary_segment {
        return Err(format!("a worktree name is one path segment, not {name:?}"));
    }
    let admin = linked_admin_dir(main, name);
    let fs = crate::fs::FsAccess::builder()
        .allow_root(&admin)
        .build()
        .map_err(|e| e.to_string())?;
    fs.write_text_atomic(admin.join("commondir"), "../..\n")
        .map_err(|e| e.to_string())
}

/// Branch name recorded in a `HEAD` file (`ref: refs/heads/<branch>`), or
/// `None` when the file is missing or detached.
///
/// FSA-FR-19 exception, deliberate: `admin_dir` is Git's own administrative
/// directory, and for a *linked* worktree that lives under the primary
/// checkout's `.git/worktrees/<name>` — outside the active worktree, which is
/// the root the shared instance allowlists. Routing this through the instance
/// would refuse every linked worktree, which is the case the function exists
/// to serve. It sits alongside the larger boundary that `git2` already reads
/// and writes repository internals through its own IO, so the helper was never
/// going to cover Git's own files.
fn branch_from_head_file(admin_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(admin_dir.join("HEAD")).ok()?;
    let refname = head.trim().strip_prefix("ref:")?.trim();
    refname
        .strip_prefix("refs/heads/")
        .map(|branch| branch.to_string())
}

/// Resolve a present worktree's `HEAD` into `(branch, short hash, detached)`.
pub(crate) fn head_of(path: &Path) -> (Option<String>, String, bool) {
    let Ok(repo) = Repository::open(path) else {
        return (None, String::new(), false);
    };
    let Ok(head) = repo.head() else {
        // An unborn `HEAD` — a repository with no commit yet — has no
        // resolvable reference, but the symbolic target still names the branch
        // the first commit will create, so the row can label itself with it
        // rather than falling back to the directory's basename.
        let branch = repo
            .find_reference("HEAD")
            .ok()
            .and_then(|r| {
                r.symbolic_target()
                    .ok()
                    .flatten()
                    .and_then(|t| t.strip_prefix("refs/heads/"))
                    .map(|b| b.to_string())
            });
        return (branch, String::new(), false);
    };
    let short = head
        .peel_to_commit()
        .ok()
        .map(|c| {
            let id = c.id().to_string();
            id.chars().take(7).collect::<String>()
        })
        .unwrap_or_default();
    if repo.head_detached().unwrap_or(false) {
        (None, short, true)
    } else {
        (head.shorthand().ok().map(|s| s.to_string()), short, false)
    }
}

fn entry_for(path: &Path, is_primary: bool, missing_branch: Option<String>) -> WorktreeEntry {
    let is_missing = !path.is_dir();
    let (branch, head_short_hash, is_detached) = if is_missing {
        (missing_branch, String::new(), false)
    } else {
        head_of(path)
    };
    WorktreeEntry {
        path: to_string_path(path),
        name: project::basename(&path.to_string_lossy()),
        branch,
        head_short_hash,
        is_detached,
        is_active: false,
        is_primary,
        is_missing,
        // WTC-FR-QKZD: filled in by the caller that can resolve the stream
        // store, because this builder answers for a repository alone.
        stream: None,
    }
}

/// Every worktree Git knows for the repository owning `root`, primary first and
/// the linked ones after it by path (WTC-FR-04).
///
/// A work stream's working copy is one of them and is excluded from nothing
/// (per `../core/WKS-work-streams.md` WKS-FR-MFDW): a stream is a checkout the
/// author opens, reads and diffs before they merge it. A stream a run holds
/// carries that fact rather than being hidden (WTC-FR-QKZD).
fn worktree_entries(repo: &Repository) -> Result<Vec<WorktreeEntry>, String> {
    let mut entries = Vec::new();
    if let Some(primary) = primary_worktree_root(repo) {
        entries.push(entry_for(&primary, true, None));
    }
    let mut linked: Vec<WorktreeEntry> = Vec::new();
    let names = repo
        .worktrees()
        .map_err(|e| format!("failed to list worktrees: {e}"))?;
    for name in names.iter().filter_map(|r| r.ok().flatten()) {
        let Ok(worktree) = repo.find_worktree(name) else {
            continue;
        };
        let path = canonicalize_lenient(worktree.path());
        // A worktree whose directory is gone is reported, not pruned
        // (WTC-FR-05) — its branch still reads out of the admin `HEAD`.
        let fallback_branch = branch_from_head_file(&linked_admin_dir(repo, name));
        linked.push(entry_for(&path, false, fallback_branch));
    }
    linked.sort_by(|a, b| a.path.cmp(&b.path));
    entries.extend(linked);
    Ok(entries)
}

fn short_hash_of(oid: git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

/// Exactly the branches with no worktree (WTC-FR-06): local branches not
/// checked out anywhere, plus remote-tracking branches with no local
/// counterpart.
///
/// A graduation run's temporary branch is neither offered as a candidate nor —
/// being checked out in a worktree WTC-FR-04 excludes — mistaken for a free one
/// (per `../core/GRD-graduation.md` WKS-FR-MFDW). The name is what settles it, so
/// the exclusion holds whether or not the run's checkout is in `worktrees`.
fn branch_entries(repo: &Repository, worktrees: &[WorktreeEntry]) -> Result<Vec<BranchEntry>, String> {
    let checked_out: BTreeSet<&str> = worktrees
        .iter()
        .filter_map(|w| w.branch.as_deref())
        .collect();

    let mut locals: Vec<BranchEntry> = Vec::new();
    let mut local_names: BTreeSet<String> = BTreeSet::new();
    let branches = repo
        .branches(Some(BranchType::Local))
        .map_err(|_| ERR_NOT_A_REPO.to_string())?;
    for item in branches {
        let Ok((branch, _)) = item else { continue };
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let name = name.to_string();
        local_names.insert(name.clone());
        if checked_out.contains(name.as_str()) {
            continue;
        }
        let upstream = branch
            .upstream()
            .ok()
            .and_then(|u| u.name().ok().flatten().map(|s| s.to_string()));
        let head_short_hash = branch
            .get()
            .peel_to_commit()
            .ok()
            .map(|c| short_hash_of(c.id()))
            .unwrap_or_default();
        locals.push(BranchEntry {
            name,
            kind: BRANCH_KIND_LOCAL.to_string(),
            upstream,
            head_short_hash,
        });
    }
    locals.sort_by(|a, b| a.name.cmp(&b.name));

    let mut remotes: Vec<BranchEntry> = Vec::new();
    let branches = repo
        .branches(Some(BranchType::Remote))
        .map_err(|_| ERR_NOT_A_REPO.to_string())?;
    for item in branches {
        let Ok((branch, _)) = item else { continue };
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        // `origin/HEAD` is a symbolic pointer at the remote's default branch,
        // not a branch the user can check out under that name.
        if name.ends_with("/HEAD") {
            continue;
        }
        // "no local counterpart" — the short name after the remote prefix.
        let short = name.split_once('/').map(|(_, rest)| rest).unwrap_or(name);
        if local_names.contains(short) {
            continue;
        }
        let head_short_hash = branch
            .get()
            .peel_to_commit()
            .ok()
            .map(|c| short_hash_of(c.id()))
            .unwrap_or_default();
        remotes.push(BranchEntry {
            name: name.to_string(),
            kind: BRANCH_KIND_REMOTE.to_string(),
            upstream: None,
            head_short_hash,
        });
    }
    remotes.sort_by(|a, b| a.name.cmp(&b.name));

    locals.extend(remotes);
    Ok(locals)
}

/// WTC-FR-04 / WTC-FR-06: the whole context, with `active_worktree_path`
/// resolved from the content root the project is currently rooted at.
pub fn context_for(active_root: &Path) -> Result<WorktreeContext, String> {
    let repo = open_repo(active_root)?;
    let repository_root = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    // The active *worktree*, not the content root: a co-located project sits in
    // a subdirectory of its checkout, and it is the checkout that is flagged.
    let active = containing_worktree(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let mut worktrees = worktree_entries(&repo)?;
    for entry in &mut worktrees {
        entry.is_active = Path::new(&entry.path) == active.as_path();
    }
    // WTC-FR-06: `branches` is what could be checked out — and from a linked
    // worktree nothing can (WTC-FR-21), so reporting candidates the caller
    // cannot act on would misrepresent what is on offer.
    let branches = if active == repository_root {
        branch_entries(&repo, &worktrees)?
    } else {
        Vec::new()
    };
    Ok(WorktreeContext {
        repository_root: to_string_path(&repository_root),
        active_worktree_path: to_string_path(&active),
        worktrees,
        branches,
    })
}

/// WTC-FR-22 / WTC-FR-23: the two halves of a refresh — fetch from the primary
/// remote, then re-enumerate — with the local half guaranteed to complete.
///
/// `fetch` is injected rather than called directly so this whole decision table
/// is exercisable without a network, a remote, or a Tauri runtime: what makes a
/// refresh correct is *which outcome each failure maps to*, not the transfer.
///
/// The command errors only when the content root is not inside a Git repository
/// (WTC-FR-02). Every other failure is a `remote_state`, because a refresh that
/// reached no remote still has something to report.
pub fn refresh_outcome_for(
    root: &Path,
    fetch: impl FnOnce() -> Result<(), String>,
) -> Result<RefreshOutcome, String> {
    // Gated before the fetch, so a project outside a repository contacts nothing
    // and gets the one typed error this command makes (WTC-FR-02 / WTC-FR-25:
    // and therefore emits no event either).
    open_repo(root)?;

    let (remote_state, remote_error) = match fetch() {
        Ok(()) => (REMOTE_REFRESHED, None),
        // No remote is not a failure — there is simply no remote half to run,
        // and saying "failed" would send the UI looking for a cause to fix.
        Err(e) if e == crate::git::ERR_NO_REMOTE_CONFIGURED => (REMOTE_SKIPPED, None),
        Err(e) => (REMOTE_FAILED, Some(e)),
    };

    // WTC-FR-22: re-enumerated by exactly the function that serves
    // `list_worktrees_and_branches`, so a refreshed listing and the dropdown's
    // own listing cannot disagree about the same repository.
    let context = context_for(root)?;
    Ok(RefreshOutcome {
        context,
        remote_state: remote_state.to_string(),
        remote_error,
    })
}

/// WTC-FR-07: the active worktree's entry alone, without walking the
/// repository's refs, so the chrome control labels itself in one cheap call.
pub fn active_entry_for(active_root: &Path) -> Result<WorktreeEntry, String> {
    let repo = open_repo(active_root)?;
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let active = containing_worktree(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let is_primary = active == primary;
    let mut entry = entry_for(&active, is_primary, None);
    entry.is_active = true;
    Ok(entry)
}

/// Is the project's content root inside the repository's **primary** worktree?
///
/// The gate on checking a branch out (WTC-FR-21). A co-located project sitting
/// in a subdirectory of the primary checkout still counts as primary — the
/// question is which checkout it belongs to, not where inside it it sits.
pub fn is_in_primary_worktree(active_root: &Path) -> Result<bool, String> {
    let repo = open_repo(active_root)?;
    let primary = primary_worktree_root(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    let active = containing_worktree(&repo).ok_or_else(|| ERR_NOT_A_REPO.to_string())?;
    Ok(active == primary)
}

/// Is `branch` checked out in a worktree *other* than the one rooting the
/// project? That is what makes a checkout impossible (WTC-FR-12 / GTC-FR-08);
/// the active worktree already being on the branch is a no-op, not a conflict.
///
/// A worktree whose directory has been deleted still counts, because Git still
/// holds its registration and would refuse the checkout too. Releasing the
/// branch means pruning that registration, which WTC-FR-19 forbids this module
/// from doing.
pub fn branch_checked_out_elsewhere(active_root: &Path, branch: &str) -> Result<bool, String> {
    let repo = open_repo(active_root)?;
    let active = canonicalize_lenient(active_root);
    Ok(worktree_entries(&repo)?.iter().any(|w| {
        w.branch.as_deref() == Some(branch) && Path::new(&w.path) != active.as_path()
    }))
}

/// WTC-FR-09: `path` must be one of the repository's worktrees, and its
/// directory must still exist. Returns the canonical path to root on.
pub fn validate_activation_target(active_root: &Path, path: &str) -> Result<PathBuf, String> {
    let repo = open_repo(active_root)?;
    let target = canonicalize_lenient(Path::new(path));
    let entry = worktree_entries(&repo)?
        .into_iter()
        .find(|e| Path::new(&e.path) == target.as_path())
        .ok_or_else(|| ERR_NOT_A_WORKTREE.to_string())?;
    if entry.is_missing {
        return Err(ERR_WORKTREE_MISSING.to_string());
    }
    Ok(target)
}

// ---------------------------------------------------------------------------
// Re-rooting (WTC-FR-08) + persistence (WTC-FR-17)
// ---------------------------------------------------------------------------

/// WTC-FR-08: unmount the previous content root's in-memory state and watchers
/// and remount them on `root`, then remember the choice and announce it.
///
/// Shared by all three switching operations, so activation, in-place checkout,
/// and creation produce byte-identical re-rooting.
fn reroot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    store: &GlobalSettingsStore,
    // A plain path: `remount_content_root` is what turns it into a guarded one.
    root: &Path,
) -> Result<WorktreeContext, String> {
    reroot_with(
        app,
        project,
        watcher,
        store,
        root,
        &crate::logging::BUFFER,
        project::Diagnostics::Discard,
    )
}

fn reroot_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    store: &GlobalSettingsStore,
    root: &Path,
    buffer: &'static crate::logging::LogBuffer,
    diagnostics: project::Diagnostics,
) -> Result<WorktreeContext, String> {
    project::remount_content_root(app, project, watcher, root, buffer, diagnostics);
    let context = context_for(root)?;
    if let Some(anchor) = project.anchor() {
        // The *worktree* is what is remembered, not the content root — a
        // co-located project's subdirectory is re-derived on resume from where
        // the project was opened (WTC-FR-17 / GSS-FR-16).
        remember(
            store,
            Path::new(&anchor),
            Path::new(&context.active_worktree_path),
        );
    }
    // WTC-FR-16: exactly one event per successful switch, carrying the new
    // active worktree's path, branch, and detached state.
    let payload = context
        .worktrees
        .iter()
        .find(|w| w.is_active)
        .map(WorktreeContextChangedPayload::from_entry)
        .unwrap_or_else(|| WorktreeContextChangedPayload {
            active_worktree_path: context.active_worktree_path.clone(),
            branch: None,
            is_detached: false,
        });
    if let Err(e) = app.emit(WORKTREE_CONTEXT_CHANGED, payload) {
        // The switch itself has already happened; an undelivered announcement
        // must not undo it. Logged rather than swallowed, because a silent
        // failure here leaves every surface bound to the content root stale.
        eprintln!("synthesis: failed to emit {WORKTREE_CONTEXT_CHANGED}: {e}");
    }
    // WTC-FR-UMGY: a direct run held for its branch starts when the branch is
    // back, so the queues are offered a dispatch once the switch is announced.
    crate::graduation::advance_all_queues(app);
    Ok(context)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_worktrees_and_branches(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<WorktreeContext, String> {
    let mut context = context_for(&project.require_root()?)?;
    // WTC-FR-QKZD: which of these worktrees are work streams, and which of
    // those a run is holding.
    crate::streams::decorate_worktrees(&app, &mut context.worktrees);
    Ok(context)
}

#[tauri::command]
pub fn get_active_worktree(project: State<'_, ProjectState>) -> Result<WorktreeEntry, String> {
    active_entry_for(&project.require_root()?)
}

#[tauri::command]
pub fn propose_worktree_path(
    branch: String,
    project: State<'_, ProjectState>,
) -> Result<String, String> {
    propose_path_for(&project.require_root()?, &branch)
}

#[tauri::command]
pub fn activate_worktree(
    path: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
    store: State<'_, GlobalSettingsStore>,
) -> Result<WorktreeContext, String> {
    activate_worktree_at(&app, &project, &watcher, &store, &path)
}

/// WTC-FR-08: the activation, split out of the command so the refusals that
/// come before the re-rooting are exercisable against a headless app.
pub(crate) fn activate_worktree_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    store: &GlobalSettingsStore,
    path: &str,
) -> Result<WorktreeContext, String> {
    let root = project.require_root()?;
    // WTC-FR-FBJQ: a dispatched direct run pins the project to its worktree.
    crate::graduation::require_no_dispatched_direct_run(app)?;
    // WTC-FR-09: a rejected activation unmounts nothing — validation runs to
    // completion before the current content root is touched.
    let target = validate_activation_target(&root, path)?;
    // WTC-FR-JXBM: an agent turn is writing that tree, so re-rooting onto it
    // would let the author edit files under a process that is rewriting them.
    if let Some((stream_id, run_id)) = crate::streams::busy_stream_at(app, &target) {
        return Err(format!("{ERR_WORK_STREAM_BUSY}: {stream_id}, {run_id}"));
    }
    // A co-located project lands in the corresponding subdirectory of the new
    // checkout rather than being relocated to that checkout's root.
    let offset = root_offset(&open_repo(&root)?, &root);
    reroot(
        app,
        project,
        watcher,
        store,
        &content_root_in(&target, &offset),
    )
}

#[tauri::command]
pub fn check_out_branch_in_active_worktree(
    branch: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
    store: State<'_, GlobalSettingsStore>,
) -> Result<WorktreeContext, String> {
    check_out_branch_at(
        &app,
        &crate::logging::BUFFER,
        &project,
        &watcher,
        &store,
        &branch,
    )
}

/// WTC-FR-10: the checkout, and the re-rooting it makes necessary.
///
/// Split out of the command, and taking its buffer, so the ordering below is
/// exercisable: the whole point of it is which records survive, and a test that
/// cannot choose the buffer cannot ask.
pub(crate) fn check_out_branch_at<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static crate::logging::LogBuffer,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    store: &GlobalSettingsStore,
    branch: &str,
) -> Result<WorktreeContext, String> {
    let root = project.require_root()?;
    // WTC-FR-FBJQ: a checkout moves the branch a direct run pinned.
    crate::graduation::require_no_dispatched_direct_run(app)?;
    // WTC-FR-JXBM: a branch a work stream owns is checked out in that stream's
    // working copy, and a run may be writing it. The refusal comes before the
    // buffer is discarded, so the record explaining it survives.
    if let Some((stream_id, run_id)) = crate::streams::busy_stream_on_branch(app, branch) {
        return Err(format!("{ERR_WORK_STREAM_BUSY}: {stream_id}, {run_id}"));
    }
    // LGC-FR-15: the diagnostic buffer is discarded *before* the operation that
    // performs the switch, so the checkout's own records — the request, the
    // branch it landed on, and any refusal — land in the fresh buffer. Clearing
    // at the re-rooting below instead would wipe exactly the records explaining
    // the checkout that caused it.
    project::discard_diagnostics_for_switch(app, project, buffer);
    // WTC-FR-10: the checkout itself goes through GTC's single primitive, so a
    // checkout from the Git panel and one from the selector take the same path.
    // A refusal returns here, leaving its record in the buffer just discarded
    // for it (LGC-FR-14, LGC-FR-15) and re-rooting nothing.
    crate::git::checkout_branch_at(app, buffer, &root, branch)?;
    // The checkout replaced the content the scan and the `.synthesis/` reads
    // are based on, so the same directory is re-rooted onto — with
    // `AlreadyDiscarded`, since a second clear would discard the records the
    // checkout just wrote.
    reroot_with(
        app,
        project,
        watcher,
        store,
        &root,
        buffer,
        project::Diagnostics::AlreadyDiscarded,
    )
}

/// WTC-FR-22 .. WTC-FR-25: refresh, then announce that the branch set may have
/// moved.
///
/// Generic over the runtime and taking `fetch` as an argument for the same
/// reason `reroot` is separate from its commands: the announcement rule — one
/// event per *returning* refresh, whatever the remote leg did, and none for a
/// refusal — is exercisable against a headless app with no network in reach.
///
/// Note what this does **not** do (WTC-FR-24): it does not go through `reroot`.
/// Nothing is unmounted or remounted, the remembered active worktree is not
/// rewritten, and no `"worktree context changed"` is emitted, because nothing
/// that event describes has changed.
fn refresh_and_announce<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    fetch: impl FnOnce() -> Result<(), String>,
) -> Result<RefreshOutcome, String> {
    let outcome = refresh_outcome_for(root, fetch)?;

    // WTC-FR-25: exactly one event per returning refresh, whether the remote leg
    // refreshed, was skipped, or failed — a branch created or deleted locally
    // since the last enumeration changes the set just as a fetch does. A refresh
    // refused above returns early and emits nothing.
    let payload = BranchesChangedPayload {
        repository_root: outcome.context.repository_root.clone(),
    };
    if let Err(e) = app.emit(BRANCHES_CHANGED, payload) {
        // The refresh itself has already happened; an undelivered announcement
        // must not undo it. Logged rather than swallowed, because a silent
        // failure here leaves the Git panel's branch listing stale.
        eprintln!("synthesis: failed to emit {BRANCHES_CHANGED}: {e}");
    }
    Ok(outcome)
}

/// The only command in this module that reaches a network, and therefore the only
/// one that must not run on the main thread.
///
/// libgit2's transfer is blocking, and an unreachable remote blocks for the OS
/// TCP timeout. A synchronous Tauri command runs on the main thread, which would
/// freeze the whole window for that duration — and take the status bar's own
/// progress reporting (GTC-FR-15) down with it, since rendering it needs the
/// thread the fetch is holding. So the blocking half runs on the blocking pool,
/// which is what lets WTS-FR-31's busy state and the progress region actually
/// paint while a slow fetch runs.
#[tauri::command]
pub async fn refresh_worktrees_and_branches(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<RefreshOutcome, String> {
    let root = project.require_root()?;
    // The user-global slot the token binding is filed under (GSS-FR-18), which
    // is the repository's anchor rather than the active worktree — so which
    // checkout is active never changes which token the project resolves.
    let project_key = project.slot_key();

    // The managed state is resolved inside the closure rather than taken as
    // parameters: a `State<'_, T>` borrows the invoke context and cannot cross
    // onto another thread, while the `AppHandle` can and reaches the same values.
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<GlobalSettingsStore>();
        let tokens = app.state::<crate::github_tokens::GithubTokens>();
        let registry = app.state::<crate::progress::ProgressRegistry>();
        refresh_and_announce(&app, &root, || {
            // WTC-FR-22: GTC's single fetch primitive, composed rather than
            // reimplemented, exactly as WTC-FR-10 composes its checkout primitive.
            crate::git::fetch_remote_branches(
                &app,
                &crate::logging::BUFFER,
                &registry,
                &root,
                &store,
                &tokens,
                &project_key,
            )
        })
    })
    .await
    // A panic inside the attributed work is re-raised by `progress::attribute`
    // after terminating the operation, so it lands here as a join error rather
    // than as a stranded in-flight operation.
    .map_err(|e| format!("refresh failed: {e}"))?
}

#[tauri::command]
pub fn create_worktree(
    branch: String,
    path: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    watcher: State<'_, ProjectWatcher>,
    store: State<'_, GlobalSettingsStore>,
) -> Result<WorktreeContext, String> {
    create_worktree_in(&app, &project, &watcher, &store, &branch, &path)
}

/// WTC-FR-14: the creation, split out of the command for the same reason as
/// [`activate_worktree_at`].
pub(crate) fn create_worktree_in<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project: &ProjectState,
    watcher: &ProjectWatcher,
    store: &GlobalSettingsStore,
    branch: &str,
    path: &str,
) -> Result<WorktreeContext, String> {
    let root = project.require_root()?;
    // WTC-FR-FBJQ: creating a worktree makes it the content root.
    crate::graduation::require_no_dispatched_direct_run(app)?;
    let offset = root_offset(&open_repo(&root)?, &root);
    let created = create_worktree_at(&root, branch, path)?;
    reroot(
        app,
        project,
        watcher,
        store,
        &content_root_in(&created, &offset),
    )
}

mod creation;
mod removal;
mod resume;
mod switch_guard;
pub use creation::*;
pub use removal::*;
pub use resume::*;
pub use switch_guard::*;

#[cfg(test)]
mod tests;
