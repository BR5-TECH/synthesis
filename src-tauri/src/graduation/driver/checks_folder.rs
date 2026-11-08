//! The check folder of a graduation working copy (GRL-FR-XQMT).
//!
//! A `work` or `merge_work` turn runs the project's own checks and records
//! their results in `.synthesis-checks/`. A review turn runs no check
//! (GRL-FR-RTNC). It reads that record instead, so each check runs once in a
//! pass. The folder is not a file of the project: the repository's private
//! `info/exclude` hides it (GRL-FR-LXWU), and the change set and every commit
//! leave it out also where that line is absent (GRL-FR-DCKG).

use std::path::Path;

use crate::fs::{EntryKind, FsAccess, FsError};

/// GRL-FR-XQMT: the folder, at the root of the working copy.
pub const DIR: &str = ".synthesis-checks";

/// GRL-FR-FESZ: the report, as a path relative to the working copy.
pub const REPORT_PATH: &str = ".synthesis-checks/report.md";

/// GRL-FR-FESZ: the value of `checks` where the review checkout holds no report.
pub const MISSING: &str = "missing";

/// GRL-FR-LXWU: the line the repository's `info/exclude` holds.
pub const EXCLUDE_LINE: &str = "/.synthesis-checks/";

/// GRL-FR-DCKG: whether a project-relative path is the check folder or is in it.
pub fn is_check_path(path: &str) -> bool {
    let path = path.trim_start_matches("./");
    path == DIR || path.strip_prefix(DIR).is_some_and(|rest| rest.starts_with('/'))
}

/// GRL-FR-LXWU: make sure the repository's `info/exclude` holds the line of the
/// check folder. Answers whether the line was added.
///
/// `info/exclude` is in Git's common directory, which a linked worktree shares
/// with the primary checkout. That directory is outside the roots of the
/// application's shared instance, so the write goes through an instance rooted
/// at it, on the terms `worktree.rs` writes a linked worktree's `commondir`.
pub fn ensure_excluded(worktree: &Path) -> Result<bool, String> {
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let common = repo.commondir().to_path_buf();
    let fs = FsAccess::builder().allow_root(&common).build().map_err(|e| e.to_string())?;
    let info = common.join("info");
    if fs.file_info(&info).is_err() {
        fs.create_dir(&info).map_err(|e| e.to_string())?;
    }
    let exclude = info.join("exclude");
    let existing = match fs.read_text(&exclude) {
        Ok(text) => text,
        Err(FsError::NotFound { .. }) => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    if existing.lines().any(|line| line.trim() == EXCLUDE_LINE) {
        return Ok(false);
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(EXCLUDE_LINE);
    updated.push('\n');
    fs.write_text_atomic(&exclude, &updated).map_err(|e| e.to_string())?;
    Ok(true)
}

/// GRL-FR-KYYW: remove the check folder of a working copy. Answers whether a
/// folder stood there.
///
/// The agent writes this folder, so it can hold a symbolic link, or be one. The
/// removal unlinks every link it meets and follows none, so a planted link can
/// neither stop the removal nor reach a path outside the folder.
pub fn clear(worktree: &Path) -> Result<bool, String> {
    let fs = FsAccess::builder().owned_root(worktree).build().map_err(|e| e.to_string())?;
    match fs.delete_owned_tree(worktree.join(DIR)) {
        Ok(()) => Ok(true),
        Err(FsError::NotFound { .. }) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// What [`copy_into`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Copied {
    /// Whether the working copy held a check folder to copy.
    pub found: bool,
    /// The files copied.
    pub files: usize,
    /// The entries left out: symbolic links and entries that are not a regular
    /// file or a directory.
    pub skipped: usize,
}

/// GRL-FR-AMQE: copy the check folder of the working copy into the same path
/// of a review checkout.
///
/// A folder that already stands in the checkout, for example one the base
/// commit tracks, is removed first, so the review reads the work turn's record
/// and no other. Only regular files and directories are copied: a symbolic link
/// is left out rather than followed, so the copy reads nothing outside the
/// folder.
pub fn copy_into(worktree: &Path, checkout: &Path) -> Result<Copied, String> {
    let target = checkout.join(DIR);
    let cleaner = FsAccess::builder().owned_root(checkout).build().map_err(|e| e.to_string())?;
    match cleaner.delete_owned_tree(&target) {
        Ok(()) | Err(FsError::NotFound { .. }) => {}
        Err(error) => return Err(error.to_string()),
    }
    let fs = FsAccess::builder()
        .allow_root(worktree)
        .allow_root(checkout)
        .build()
        .map_err(|e| e.to_string())?;
    let source = worktree.join(DIR);
    let mut copied = Copied::default();
    match fs.file_info(&source) {
        Ok(info) if info.kind == EntryKind::Dir => {}
        Ok(_) => {
            copied.skipped = 1;
            return Ok(copied);
        }
        Err(_) => return Ok(copied),
    }
    copied.found = true;
    fs.create_dir(&target).map_err(|e| e.to_string())?;
    copy_dir(&fs, &source, &target, &mut copied)?;
    Ok(copied)
}

/// Copy the regular files and directories below `source` into `target`.
fn copy_dir(
    fs: &FsAccess,
    source: &Path,
    target: &Path,
    copied: &mut Copied,
) -> Result<(), String> {
    for entry in fs.list_dir(source).map_err(|e| e.to_string())? {
        let from = source.join(&entry.name);
        let to = target.join(&entry.name);
        match entry.kind {
            EntryKind::Dir => {
                fs.create_dir(&to).map_err(|e| e.to_string())?;
                copy_dir(fs, &from, &to, copied)?;
            }
            EntryKind::File => {
                fs.copy_path(&from, &to).map_err(|e| e.to_string())?;
                copied.files += 1;
            }
            EntryKind::Symlink | EntryKind::Other => copied.skipped += 1,
        }
    }
    Ok(())
}

/// GRL-FR-FESZ: the `checks` value of a review turn that stands in `checkout`.
pub fn report_ref(checkout: &Path) -> String {
    let Ok(fs) = FsAccess::builder().allow_root(checkout).build() else {
        return MISSING.to_string();
    };
    let is_file =
        fs.file_info(checkout.join(REPORT_PATH)).is_ok_and(|info| info.kind == EntryKind::File);
    if is_file {
        REPORT_PATH.to_string()
    } else {
        MISSING.to_string()
    }
}

#[cfg(test)]
mod tests;
