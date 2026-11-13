//! The scratch folder of a plan update (OPU-FR-BNFL through OPU-FR-WJKF,
//! OPU-FR-MBIY through OPU-FR-MCEZ).
//!
//! The apply agent writes each planning file it proposes, whole, at
//! `.synthesis-plan/<path>`. The folder is not a file of the project: the
//! repository's `info/exclude` hides it, and the change set, every commit, and
//! every review checkout leave it out also where that line is absent.

use std::path::Path;

use super::PlanRefusalCause;
use crate::change_proposals::PathedHunk;
use crate::fs::{EntryKind, FsAccess, FsError};

/// OPU-FR-BNFL: the folder, at the root of the stream's working copy.
pub const DIR: &str = ".synthesis-plan";

/// OPU-FR-WINO: the line the repository's `info/exclude` holds.
pub const EXCLUDE_LINE: &str = "/.synthesis-plan/";

/// OPU-FR-WINO: whether a project-relative path is the scratch folder or is in
/// it.
pub fn is_scratch_path(path: &str) -> bool {
    let path = path.trim_start_matches("./");
    path == DIR || path.strip_prefix(DIR).is_some_and(|rest| rest.starts_with('/'))
}

/// OPU-FR-WINO: make sure the repository's `info/exclude` holds the line of
/// the scratch folder. Answers whether the line was added.
pub fn ensure_excluded(worktree: &Path) -> Result<bool, String> {
    crate::graduation::driver::checks_folder::ensure_exclude_line(worktree, EXCLUDE_LINE)
}

/// OPU-FR-WJKF, OPU-FR-FOPN: remove the scratch folder. Answers whether a
/// folder stood there. A link it meets is unlinked and not followed.
pub fn clear(worktree: &Path) -> Result<bool, String> {
    let fs = FsAccess::builder().owned_root(worktree).build().map_err(|e| e.to_string())?;
    match fs.delete_owned_tree(worktree.join(DIR)) {
        Ok(()) => Ok(true),
        Err(FsError::NotFound { .. }) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// Why the scratch folder could not be read as a plan update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScratchError {
    /// OPU-FR-XLVF: the plan update is refused with this cause.
    Refused(PlanRefusalCause),
    /// The folder or the working copy could not be read.
    Unreadable(String),
}

/// One entry below the scratch folder.
struct Entry {
    /// The project-relative path the entry proposes a file for.
    path: String,
    /// `None` where the entry is not a regular file.
    bytes: Option<Vec<u8>>,
}

/// OPU-FR-MBIY, OPU-FR-XLVF, OPU-FR-MCEZ: the hunks of the scratch folder of
/// `worktree` against the same paths in the working copy, in the order of
/// their paths. Each path is change-relative. A folder that does not stand
/// gives no hunk. The cause `plan_update_empty` is the caller's, which knows
/// the questions.
pub fn read(worktree: &Path, change: &str) -> Result<Vec<PathedHunk>, ScratchError> {
    let fs = FsAccess::builder()
        .allow_root(worktree)
        .build()
        .map_err(|e| ScratchError::Unreadable(e.to_string()))?;
    let root = worktree.join(DIR);
    let mut entries = Vec::new();
    match fs.file_info(&root) {
        Ok(info) if info.kind == EntryKind::Dir => walk(&fs, &root, "", &mut entries)?,
        Ok(_) => return Err(ScratchError::Refused(PlanRefusalCause::PlanPathOutsideChange)),
        Err(FsError::NotFound { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(ScratchError::Unreadable(error.to_string())),
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let folder = format!("openspec/changes/{change}/");
    // OPU-FR-XLVF: the causes in their order, each over every entry.
    let mut files = Vec::new();
    for entry in &entries {
        let rel = entry
            .path
            .strip_prefix(&folder)
            .filter(|rel| crate::comments::is_change_relative_path(rel));
        match (rel, entry.bytes.as_ref()) {
            (Some(rel), Some(bytes)) => files.push((rel.to_string(), &entry.path, bytes)),
            _ => return Err(ScratchError::Refused(PlanRefusalCause::PlanPathOutsideChange)),
        }
    }
    let mut texts = Vec::new();
    for (rel, path, bytes) in files {
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| ScratchError::Refused(PlanRefusalCause::PlanFileNotText))?;
        texts.push((rel, path, text));
    }
    let mut compared = Vec::new();
    for (rel, path, text) in texts {
        let current = match fs.read_bytes(worktree.join(path)) {
            Ok(bytes) => Some(
                String::from_utf8(bytes)
                    .map_err(|_| ScratchError::Refused(PlanRefusalCause::PlanFileNotText))?,
            ),
            Err(FsError::NotFound { .. }) => None,
            Err(error) => return Err(ScratchError::Unreadable(error.to_string())),
        };
        if text.is_empty() && current.as_deref().is_some_and(|c| !c.is_empty()) {
            return Err(ScratchError::Refused(PlanRefusalCause::PlanDeletesFile));
        }
        compared.push((rel, current, text));
    }
    let mut hunks = Vec::new();
    for (rel, current, text) in compared {
        // An empty proposal for a path that holds nothing proposes nothing.
        if text.is_empty() {
            continue;
        }
        for hunk in crate::graduation::driver::hunks_of(current.as_deref(), &text) {
            hunks.push(PathedHunk { path: rel.clone(), hunk });
        }
    }
    Ok(hunks)
}

/// Every entry below `dir`, with its path below the scratch folder.
fn walk(fs: &FsAccess, dir: &Path, prefix: &str, out: &mut Vec<Entry>) -> Result<(), ScratchError> {
    let listed = fs.list_dir(dir).map_err(|e| ScratchError::Unreadable(e.to_string()))?;
    for entry in listed {
        let path = format!("{prefix}{}", entry.name);
        let at = dir.join(&entry.name);
        match entry.kind {
            EntryKind::Dir => walk(fs, &at, &format!("{path}/"), out)?,
            EntryKind::File => {
                let bytes =
                    fs.read_bytes(&at).map_err(|e| ScratchError::Unreadable(e.to_string()))?;
                out.push(Entry { path, bytes: Some(bytes) });
            }
            EntryKind::Symlink | EntryKind::Other => out.push(Entry { path, bytes: None }),
        }
    }
    Ok(())
}
