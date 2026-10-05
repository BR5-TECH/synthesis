//! Following a rename of a commented file.

use super::*;

// ---------------------------------------------------------------------------
// Rename following (CMS-FR-24 / CMS-FR-25)
// ---------------------------------------------------------------------------

/// CMS-FR-24: rename an artifact's log to the new path's `log_id`.
///
/// No line inside it is rewritten — the file's *name* is what binds a log to an
/// artifact, and the `artifact_path` an event carries is a human-readable record
/// of where the thread was opened. That is what keeps a rename append-only.
///
/// This is the correlation seam of CMS-FR-24, mirroring `notes::follow_rename`:
/// it is called where the application knows a single old path became a single new
/// path. A change it cannot correlate that way never reaches here, and the log
/// stays under its old id (CMS-FR-25) rather than being removed.
///
/// Best-effort, returning how many logs moved: a log that cannot be renamed is
/// left as it was rather than failing the rename that triggered this.
pub fn follow_rename(
    root: &fsa::RootFs,
    worktree: &fsa::RootFs,
    old_rel: &str,
    new_rel: &str,
) -> usize {
    if old_rel.is_empty() || new_rel.is_empty() || old_rel == new_rel {
        return 0;
    }
    let dir = comments_dir(root);
    if !dir.is_dir() {
        return 0;
    }
    // The correlation runs off the *filename*, which is `log_id(current path)`
    // and is kept in sync by this function itself — so it stays correct on the
    // second rename of an artifact and every one after.
    //
    // Deliberately NOT off the `artifact_path` an event carries: that field is
    // written once when the thread is opened and, because the log is append-only,
    // is never updated. Correlating by it works exactly once and then silently
    // stops matching, stranding the log under a name nothing looks for.
    //
    // `new_rel` is the path *after* the filesystem rename has already happened
    // (this is called from `library.rs` immediately after it), so walking it is
    // what enumerates the artifacts that moved — one file for a file rename, the
    // whole subtree for a folder.
    let mut moved = 0;
    // CMS-FR-24 / CMS-FR-55: a file's two logs are renamed together, the reserved
    // marker intact on the discussion one. Each is moved on its own terms — one
    // existing without the other is ordinary (a file discussed but never annotated,
    // or the reverse), so neither is a precondition for moving the other.
    let mut rename_log = |old_path: &str, new_path: &str| {
        let old_id = log_id(old_path);
        let new_id = log_id(new_path);
        if new_id == old_id {
            return;
        }
        for marker in ["", DISCUSSION_LOG_MARKER] {
            let old_name = format!("{old_id}{marker}.jsonl");
            if !dir.join(&old_name).is_file() {
                continue;
            }
            let new_name = format!("{new_id}{marker}.jsonl");
            // Never overwrite: if the destination already holds a log, the new path
            // has its own conversation and merging the two is not this module's call.
            if dir.join(&new_name).exists() {
                continue;
            }
            if root.rename_under(root.path(), format!("{COMMENTS_REL}/{old_name}"), &new_name).is_ok() {
                moved += 1;
            }
        }
    };

    // The renamed path is in the **worktree**: what moved is a file of the
    // project, and what follows it is a log in the store.
    let Ok(target) = fsa::resolve_under(worktree, new_rel) else {
        return 0;
    };
    if target.is_dir() {
        for tail in relative_files(worktree, &target, String::new()) {
            rename_log(&format!("{old_rel}/{tail}"), &format!("{new_rel}/{tail}"));
        }
    } else {
        rename_log(old_rel, new_rel);
    }
    moved
}

/// Every file under `dir`, as paths relative to it with forward slashes.
///
/// Used to enumerate what a folder rename moved. Symlinks are not followed: a
/// link out of the project would take the walk with it, and a comment log is only
/// ever keyed by a real project-relative path.
pub(super) fn relative_files(root: &fsa::RootFs, dir: &Path, prefix: String) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = root.list_dir(dir) else {
        return out;
    };
    for entry in entries {
        let name = entry.name.clone();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = dir.join(&entry.name);
        let Ok(meta) = root.file_info(&path) else {
            continue;
        };
        if meta.kind == fsa::EntryKind::Symlink {
            continue;
        }
        if meta.kind == fsa::EntryKind::Dir {
            out.extend(relative_files(root, &path, rel));
        } else {
            out.push(rel);
        }
    }
    out
}
