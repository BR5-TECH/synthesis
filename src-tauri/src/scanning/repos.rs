//! Repository boundaries inside a project (`ASC-artifact-scanning.md`
//! ASC-FR-SFCR, ASC-FR-TWVB, ASC-FR-LXOQ, ASC-FR-YLGN).
//!
//! A git submodule, or any other repository inside the project, is a nested
//! repository root. Its `.git` entry is skipped, its own ignore rules apply
//! below it, and the path conventions match relative to it. These helpers are
//! the one place that knows what a repository root is.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::{EntryKind, RootFs};

/// ASC-FR-YLGN: what kind of repository boundary a folder is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NestedRepo {
    /// A nested repository root that `.gitmodules` of its enclosing repository
    /// lists.
    Submodule,
    /// A nested repository root that no `.gitmodules` lists.
    Repository,
    /// A folder that `.gitmodules` lists, with no `.git` entry.
    Uninitialised,
}

/// ASC-FR-SFCR: the name of the entry that makes a folder a repository root.
pub const GIT_ENTRY: &str = ".git";

/// ASC-FR-SFCR: whether a path segment list holds a `.git` entry, which the
/// scan and the watcher skip at every depth.
pub fn has_git_segment(rel: &str) -> bool {
    rel.split('/').any(|seg| seg == GIT_ENTRY)
}

/// ASC-FR-SFCR: whether the folder `rel` of the project holds a `.git` entry,
/// a file or a folder. A symbolic link named `.git` does not count.
pub fn is_repo_root(root: &RootFs, rel: &str) -> bool {
    root.file_info(join_rel(rel, GIT_ENTRY))
        .is_ok_and(|info| matches!(info.kind, EntryKind::Dir | EntryKind::File))
}

/// ASC-FR-TWVB: the git directory that the `.git` entry of the repository root
/// `rel` names. A `.git` folder is the git directory. A `.git` file names it on
/// its `gitdir:` line, relative to the repository root or absolute.
pub fn git_dir_of(root: &RootFs, rel: &str) -> Option<PathBuf> {
    let entry = join_rel(rel, GIT_ENTRY);
    let info = root.file_info(&entry).ok()?;
    let repo_abs = root.join(rel);
    match info.kind {
        EntryKind::Dir => Some(repo_abs.join(GIT_ENTRY)),
        EntryKind::File => {
            let text = root.read_text(&entry).ok()?;
            let target = text.lines().find_map(|l| l.trim().strip_prefix("gitdir:"))?.trim();
            if target.is_empty() {
                return None;
            }
            let path = Path::new(target);
            Some(if path.is_absolute() { path.to_path_buf() } else { repo_abs.join(path) })
        }
        _ => None,
    }
}

/// ASC-FR-TWVB: the `info/exclude` file of the repository root `rel`. A linked
/// worktree keeps it in the common git directory, which its `commondir` file
/// names.
///
/// FSA-FR-19 exception, deliberate: the git directory of a submodule or of a
/// linked worktree is git's own administrative folder, and it is often outside
/// the project root that the shared instance allows. The ignore walker reads it
/// with its own IO in the same way. This reads two small text files and writes
/// nothing.
pub fn exclude_file_of(root: &RootFs, rel: &str) -> Option<PathBuf> {
    let git_dir = git_dir_of(root, rel)?;
    // Git reads `info/exclude` from the common git directory alone, so a
    // linked worktree's own git directory is not asked.
    let common = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(text) => {
            let text = text.trim();
            if Path::new(text).is_absolute() {
                PathBuf::from(text)
            } else {
                git_dir.join(text)
            }
        }
        Err(_) => git_dir,
    };
    let exclude = common.join("info").join("exclude");
    exclude.is_file().then_some(exclude)
}

/// ASC-FR-YLGN: the project-relative paths that the `.gitmodules` file of the
/// repository at `repo_rel` lists. A missing or unreadable file lists none.
pub fn submodule_paths(root: &RootFs, repo_rel: &str) -> BTreeSet<String> {
    let file = join_rel(repo_rel, ".gitmodules");
    let Ok(text) = root.read_text(&file) else {
        return BTreeSet::new();
    };
    parse_gitmodules(&text).into_iter().map(|p| join_rel(repo_rel, &p)).collect()
}

/// The `path = ...` values of a `.gitmodules` text, normalised. The key
/// matches without regard to letter case, as git config keys do. A value in
/// quotes ends at the closing quote; an unquoted value ends at a `;` or `#`
/// comment. A path that leaves its repository is dropped.
pub fn parse_gitmodules(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            if !key.trim().eq_ignore_ascii_case("path") {
                return None;
            }
            let value = value.trim();
            let value = match value.strip_prefix('"') {
                Some(rest) => rest.split('"').next().unwrap_or(""),
                None => value.split([';', '#']).next().unwrap_or("").trim(),
            };
            Some(value.replace('\\', "/").trim_matches('/').to_string())
        })
        .filter(|p| !p.is_empty() && !p.split('/').any(|s| s == ".." || s == "." || s.is_empty()))
        .collect()
}

/// ASC-FR-LXOQ: the nearest repository root of `rel` among `roots` (the
/// project root `""` when none holds it). Each ancestor folder of `rel` is
/// looked up once, deepest first.
pub fn nearest_root(rel: &str, roots: &BTreeSet<String>) -> String {
    if roots.is_empty() {
        return String::new();
    }
    let mut dir = parent_of(rel);
    while !dir.is_empty() {
        if roots.contains(dir) {
            return dir.to_string();
        }
        dir = parent_of(dir);
    }
    String::new()
}

/// ASC-FR-LXOQ: `rel` relative to the repository root `repo`.
pub fn relative_to<'a>(rel: &'a str, repo: &str) -> &'a str {
    if repo.is_empty() {
        rel
    } else {
        rel.strip_prefix(repo).and_then(|r| r.strip_prefix('/')).unwrap_or(rel)
    }
}

/// ASC-FR-LXOQ: the nearest repository root of `rel`, read from the disk. A
/// caller with no project index (a one-off classification) uses this. The
/// walk stops at the project root.
pub fn nearest_root_on_disk(root: &RootFs, rel: &str) -> String {
    let mut dir = parent_of(rel);
    while !dir.is_empty() {
        let has_git = root
            .file_info(join_rel(dir, GIT_ENTRY))
            .is_ok_and(|info| matches!(info.kind, EntryKind::Dir | EntryKind::File));
        if has_git {
            return dir.to_string();
        }
        dir = parent_of(dir);
    }
    String::new()
}

fn parent_of(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(parent, _)| parent)
}

pub(crate) fn join_rel(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    }
}
