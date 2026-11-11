//! Change version history (`CVH-change-version-history.md`).
//!
//! A **version** of an OpenSpec change is a commit of the stream branch that
//! changes a file of `openspec/changes/<change>/` (CVH-FR-HDTA). This module
//! reads the versions from Git and keeps no record of its own. It reads Git
//! objects alone and writes nothing (CVH-FR-DVTL).

use std::path::Path;

use git2::{Oid, Repository, Tree};
use serde::Serialize;

use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};

/// CVH-FR-DAAQ: the most versions a list returns.
pub const MAX_VERSIONS: usize = 500;

pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_UNKNOWN_STREAM: &str = "unknown_stream";
pub const ERR_INVALID_CHANGE_NAME: &str = "invalid_change_name";
pub const ERR_CHANGE_NOT_FOUND: &str = "change_not_found";
pub const ERR_UNKNOWN_VERSION: &str = "unknown_version";
pub const ERR_READ_FAILED: &str = "read_failed";

/// CVH contract surface `ChangeVersion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeVersion {
    pub commit_id: String,
    pub summary: String,
    pub author_name: String,
    pub committed_at: String,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

/// What `"list change versions"` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeVersions {
    pub versions: Vec<ChangeVersion>,
    pub truncated: bool,
}

/// CVH contract surface `ChangeVersionFile`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeVersionFile {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub bytes: u64,
    pub binary: bool,
}

/// What `"read change version"` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeVersionFiles {
    pub files: Vec<ChangeVersionFile>,
}

/// The folder of the change inside the repository: the project prefix, then
/// `openspec/changes/<change>`.
fn change_dir(repo: &Repository, root: &Path, change: &str) -> String {
    let prefix = crate::changes::project_prefix(repo, root);
    let rel = format!("openspec/changes/{change}");
    if prefix.is_empty() {
        rel
    } else {
        format!("{}/{rel}", prefix.trim_end_matches('/'))
    }
}

/// The subtree of `tree` at `dir`, or `None` where the tree holds no folder
/// there.
fn subtree<'r>(repo: &'r Repository, tree: &Tree<'r>, dir: &str) -> Option<Tree<'r>> {
    let entry = tree.get_path(Path::new(dir)).ok()?;
    repo.find_tree(entry.id()).ok()
}

/// The id of the subtree at `dir`, which two commits share where the change
/// folder is the same in both (CVH-FR-HDTA).
fn subtree_id(tree: &Tree<'_>, dir: &str) -> Option<Oid> {
    tree.get_path(Path::new(dir)).ok().map(|e| e.id())
}

fn rfc3339(time: git2::Time) -> String {
    crate::notes::format_rfc3339_utc(time.seconds())
}

/// CVH-FR-HDTA / CVH-FR-VZGR / CVH-FR-URCF / CVH-FR-ITHO / CVH-FR-DAAQ: every
/// version reachable from `HEAD`, most recent first by the commit graph.
pub fn list_versions_in(root: &Path, change: &str) -> Result<ChangeVersions, String> {
    list_versions_limited(root, change, MAX_VERSIONS)
}

/// [`list_versions_in`] with the bound of CVH-FR-DAAQ given, for a test.
pub(crate) fn list_versions_limited(
    root: &Path,
    change: &str,
    limit: usize,
) -> Result<ChangeVersions, String> {
    let repo = crate::changes::open_repo(root).map_err(|_| ERR_READ_FAILED.to_string())?;
    let dir = change_dir(&repo, root, change);
    let mut walk = repo.revwalk().map_err(|_| ERR_READ_FAILED.to_string())?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
        .map_err(|_| ERR_READ_FAILED.to_string())?;
    if walk.push_head().is_err() {
        // An unborn branch holds no commit, and so no version.
        return Err(ERR_CHANGE_NOT_FOUND.to_string());
    }
    let mut versions = Vec::new();
    let mut truncated = false;
    for oid in walk {
        let oid = oid.map_err(|_| ERR_READ_FAILED.to_string())?;
        let commit = repo.find_commit(oid).map_err(|_| ERR_READ_FAILED.to_string())?;
        let tree = commit.tree().map_err(|_| ERR_READ_FAILED.to_string())?;
        let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
        let now = subtree_id(&tree, &dir);
        let before = parent_tree.as_ref().and_then(|t| subtree_id(t, &dir));
        if now == before {
            continue;
        }
        if versions.len() == limit {
            truncated = true;
            break;
        }
        let new_sub = subtree(&repo, &tree, &dir);
        let old_sub = parent_tree.as_ref().and_then(|t| subtree(&repo, t, &dir));
        // CVH-FR-URCF: no rename detection, so a rename is a delete and an add.
        let diff = repo
            .diff_tree_to_tree(old_sub.as_ref(), new_sub.as_ref(), None)
            .map_err(|_| ERR_READ_FAILED.to_string())?;
        let (mut added, mut modified, mut deleted) = (Vec::new(), Vec::new(), Vec::new());
        for delta in diff.deltas() {
            let path = |f: git2::DiffFile<'_>| {
                f.path().map(crate::changes::to_forward).unwrap_or_default()
            };
            match delta.status() {
                git2::Delta::Added => added.push(path(delta.new_file())),
                git2::Delta::Deleted => deleted.push(path(delta.old_file())),
                _ => modified.push(path(delta.new_file())),
            }
        }
        let author = commit.author();
        versions.push(ChangeVersion {
            commit_id: oid.to_string(),
            summary: String::from_utf8_lossy(commit.message_bytes())
                .lines()
                .next()
                .unwrap_or("")
                .to_string(),
            author_name: String::from_utf8_lossy(author.name_bytes()).into_owned(),
            committed_at: rfc3339(commit.time()),
            added,
            modified,
            deleted,
        });
    }
    // CVH-FR-HXVV: refused only where the branch holds no version at all.
    if versions.is_empty() {
        return Err(ERR_CHANGE_NOT_FOUND.to_string());
    }
    Ok(ChangeVersions { versions, truncated })
}

/// CVH-FR-VDZZ / CVH-FR-VSQD: the files of the change folder at one version,
/// ordered by change-relative path.
pub fn read_version_in(
    root: &Path,
    change: &str,
    commit_id: &str,
) -> Result<ChangeVersionFiles, String> {
    let repo = crate::changes::open_repo(root).map_err(|_| ERR_READ_FAILED.to_string())?;
    let dir = change_dir(&repo, root, change);
    let oid = Oid::from_str(commit_id).map_err(|_| ERR_UNKNOWN_VERSION.to_string())?;
    let commit = repo.find_commit(oid).map_err(|_| ERR_UNKNOWN_VERSION.to_string())?;
    // CVH-FR-VSQD / CVH-FR-ITHO: the commit is a version where it is reachable
    // from the tip and changes the folder against its first parent. Checked
    // on this commit alone, with no walk of the history.
    let head =
        repo.head().ok().and_then(|h| h.target()).ok_or_else(|| ERR_UNKNOWN_VERSION.to_string())?;
    let reachable = head == oid || repo.graph_descendant_of(head, oid).unwrap_or(false);
    let tree = commit.tree().map_err(|_| ERR_READ_FAILED.to_string())?;
    let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
    let changes_folder =
        subtree_id(&tree, &dir) != parent_tree.as_ref().and_then(|t| subtree_id(t, &dir));
    if !reachable || !changes_folder {
        return Err(ERR_UNKNOWN_VERSION.to_string());
    }
    let mut files = Vec::new();
    if let Some(sub) = subtree(&repo, &tree, &dir) {
        sub.walk(git2::TreeWalkMode::PreOrder, |parent, entry| {
            if entry.kind() != Some(git2::ObjectType::Blob) {
                return git2::TreeWalkResult::Ok;
            }
            let Ok(blob) = repo.find_blob(entry.id()) else {
                return git2::TreeWalkResult::Ok;
            };
            let path = format!("{parent}{}", entry.name().unwrap_or(""));
            let text = std::str::from_utf8(blob.content()).ok().map(str::to_string);
            files.push(ChangeVersionFile {
                path,
                binary: text.is_none(),
                text,
                bytes: blob.size() as u64,
            });
            git2::TreeWalkResult::Ok
        })
        .map_err(|_| ERR_READ_FAILED.to_string())?;
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ChangeVersionFiles { files })
}

/// CVH-FR-YDVP: the stream's working copy, or the refusal for the call.
fn resolve<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    if app.try_state::<crate::project::ProjectState>().and_then(|p| p.require_root().ok()).is_none()
    {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    if !crate::openspec::change::valid_change_name(change) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    let (_, copy) = crate::change_proposals::stream_working_copy(stream_id)
        .ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    Ok(copy.path().to_path_buf())
}

/// CVH-FR-NCKN: one `DEBUG` record on success and one `WARN` on a refusal.
fn report<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    operation: &'static str,
    ids: (&str, &str),
    started: std::time::Instant,
    outcome: &Result<T, String>,
    count: impl Fn(&T) -> usize,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (stream_id, change) = ids;
    match outcome {
        Ok(value) => logging::log_debug(
            app,
            &BUFFER,
            &[Domain::Backend],
            "change versions read",
            log_fields! {
                "operation" => operation,
                "streamId" => stream_id,
                "change" => change,
                "count" => count(value),
                "elapsedMs" => started.elapsed().as_millis() as u64,
            },
        ),
        Err(code) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "change versions refused",
            log_fields! {
                "operation" => operation,
                "streamId" => stream_id,
                "change" => change,
                "reason" => code,
            },
        ),
    }
}

/// CVH-FR-VZGR / CVH-FR-GHMO: `"list change versions"`, on a blocking thread.
#[tauri::command]
pub async fn list_change_versions<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
) -> Result<ChangeVersions, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let outcome =
            resolve(&app, &stream_id, &change).and_then(|root| list_versions_in(&root, &change));
        report(&app, "list", (&stream_id, &change), started, &outcome, |v| v.versions.len());
        outcome
    })
    .await
    .map_err(|_| ERR_READ_FAILED.to_string())?
}

/// CVH-FR-VDZZ / CVH-FR-GHMO: `"read change version"`, on a blocking thread.
#[tauri::command]
pub async fn read_change_version<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
    commit_id: String,
) -> Result<ChangeVersionFiles, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let outcome = resolve(&app, &stream_id, &change)
            .and_then(|root| read_version_in(&root, &change, &commit_id));
        report(&app, "read", (&stream_id, &change), started, &outcome, |v| v.files.len());
        outcome
    })
    .await
    .map_err(|_| ERR_READ_FAILED.to_string())?
}

/// CVH contract surface `count_change_versions`, for `CST-change-statistics.md`
/// (CST-FR-PUAN). It refuses on the terms of CVH-FR-YDVP.
pub fn count_change_versions(stream_id: &str, change: &str) -> Result<usize, String> {
    if !crate::openspec::change::valid_change_name(change) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    let (_, copy) = crate::change_proposals::stream_working_copy(stream_id)
        .ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    list_versions_in(copy.path(), change).map(|v| v.versions.len())
}

#[cfg(test)]
mod tests;
