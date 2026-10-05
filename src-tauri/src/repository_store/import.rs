//! The import pass (`RMS-repository-machine-storage.md` RMS-FR-JVEC …
//! RMS-FR-XLTR).
//!
//! An earlier build committed the conversation logs and the statistics logs into
//! the project itself. This pass copies what those folders hold into the
//! repository machine store, so a project opened by this build reads the whole
//! of its own history rather than only what it has recorded since.
//!
//! Two rules make it safe to run at every open. It is **additive and
//! idempotent** (RMS-FR-SUAK): a line is appended only where the destination log
//! holds no event of that identity, and an attachment is written only where the
//! destination holds no file of that digest, so a second pass over the same
//! folders writes nothing. And it **removes nothing** (RMS-FR-XRPT): it deletes,
//! rewrites, moves, stages, and commits no path of the project, so the committed
//! files stay exactly as they are and the author removes them in a commit of
//! their own.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs as fsa;

use super::{COMMENTS_DIRNAME, DRAFTS_DIRNAME, STATISTICS_DIRNAME};

/// The folders an earlier build committed the logs into (RMS-FR-JVEC).
const LEGACY_COMMENTS_REL: &str = ".synthesis/comments";
const LEGACY_STATISTICS_REL: &str = ".synthesis/statistics";

/// The `comments/` folder inside a draft's own directory, which an earlier build
/// kept that draft's review logs in.
const LEGACY_DRAFT_COMMENTS_DIR: &str = "comments";

/// RMS-FR-SUAK: the attributes file is a Git artifact and belongs to no store.
const GITATTRIBUTES: &str = ".gitattributes";

/// What one pass added. Counted rather than named, so a record of it carries no
/// path and no draft identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    /// Log lines appended that the store did not already hold.
    pub lines: usize,
    /// Whole files — attachments — written that the store did not already hold.
    pub files: usize,
}

impl ImportOutcome {
    pub fn is_empty(&self) -> bool {
        self.lines == 0 && self.files == 0
    }
}

/// RMS-FR-JVEC: one import pass for a worktree.
///
/// Best-effort throughout: a folder that cannot be read and a file that cannot
/// be copied are both skipped, and the rest of the pass continues, because a
/// pass that failed halfway is worth more than one that refused.
pub fn import_legacy_storage(store: &fsa::RootFs, worktree: &fsa::RootFs) -> ImportOutcome {
    let mut out = ImportOutcome::default();
    import_tree(
        store,
        worktree,
        &worktree.path().join(LEGACY_COMMENTS_REL),
        &store.path().join(COMMENTS_DIRNAME),
        &mut out,
    );
    // The drafts the worktree still holds, which is what a statistics log has to
    // belong to before it is imported.
    let drafts = crate::drafts::draft_locations(worktree);
    let live: std::collections::HashSet<&str> =
        drafts.iter().map(|(id, _)| id.as_str()).collect();
    // `DRS-draft-storage.md` DRS-FR-ZLBK: deleting a draft takes its statistics
    // log, and success means it is gone. The committed copy stays in the
    // worktree because the pass removes nothing (RMS-FR-XRPT), so importing it
    // would resurrect the account of a draft nobody kept.
    import_tree_where(
        store,
        worktree,
        &worktree.path().join(LEGACY_STATISTICS_REL),
        &store.path().join(STATISTICS_DIRNAME),
        &mut out,
        &|name| {
            name.strip_suffix(".jsonl")
                .is_some_and(|draft_id| live.contains(draft_id))
        },
    );
    for (draft_id, dir) in drafts {
        import_tree(
            store,
            worktree,
            &dir.join(LEGACY_DRAFT_COMMENTS_DIR),
            &store
                .path()
                .join(DRAFTS_DIRNAME)
                .join(&draft_id)
                .join(COMMENTS_DIRNAME),
            &mut out,
        );
    }
    out
}

/// Copy one legacy folder into the store, recursively.
fn import_tree(
    store: &fsa::RootFs,
    worktree: &fsa::RootFs,
    src: &Path,
    dest: &Path,
    out: &mut ImportOutcome,
) {
    import_tree_where(store, worktree, src, dest, out, &|_| true);
}

/// [`import_tree`] over the entries of the **top level** a predicate admits.
///
/// The predicate reaches the folder's own entries and no deeper: what it exists
/// to decide is which per-draft statistics log belongs to a draft the worktree
/// still holds, and that is a property of the filename.
fn import_tree_where(
    store: &fsa::RootFs,
    worktree: &fsa::RootFs,
    src: &Path,
    dest: &Path,
    out: &mut ImportOutcome,
    admit: &dyn Fn(&str) -> bool,
) {
    let Ok(entries) = worktree.list_dir(src) else {
        // No legacy folder at all is nothing to import.
        return;
    };
    for entry in entries {
        if !admit(&entry.name) {
            continue;
        }
        // RMS-FR-SUAK: a Git artifact belongs to no store.
        if entry.name == GITATTRIBUTES {
            continue;
        }
        let src_path = src.join(&entry.name);
        let Ok(info) = worktree.file_info(&src_path) else {
            continue;
        };
        match info.kind {
            // A link is never followed here, on the same terms nothing else in
            // this application follows one.
            fsa::EntryKind::Symlink => continue,
            fsa::EntryKind::Dir => {
                import_tree(store, worktree, &src_path, &dest.join(&entry.name), out);
            }
            _ => {
                let dest_path = dest.join(&entry.name);
                if entry.name.ends_with(".jsonl") {
                    out.lines += merge_log(store, worktree, &src_path, &dest_path);
                } else if copy_absent(store, worktree, &src_path, &dest_path) {
                    out.files += 1;
                }
            }
        }
    }
}

/// RMS-FR-SUAK: append the lines the destination log does not already hold.
///
/// Deduplicated on the event's own `event_id`, which every line of both log
/// families carries (`CMS-comments-storage.md` CMS-FR-06,
/// `DSS-draft-statistics-storage.md` DSS-FR-PNUE), so a pass run twice appends
/// nothing the second time. A line carrying no readable identity is deduplicated
/// on its own text instead, which is the same guarantee for a line this build
/// cannot parse.
fn merge_log(
    store: &fsa::RootFs,
    worktree: &fsa::RootFs,
    src: &Path,
    dest: &Path,
) -> usize {
    let Ok(source) = worktree.read_text(src) else {
        return 0;
    };
    let existing = store.read_text(dest).unwrap_or_default();
    let mut held: HashSet<String> = existing.lines().map(line_identity).collect();

    let mut fresh: Vec<String> = Vec::new();
    for line in source.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let identity = line_identity(line);
        if held.insert(identity) {
            fresh.push(line.to_string());
        }
    }
    if fresh.is_empty() {
        return 0;
    }
    if ensure_dir(store, dest).is_err() {
        return 0;
    }
    match store.append_lines(dest, &fresh) {
        Ok(()) => fresh.len(),
        Err(_) => 0,
    }
}

/// The identity a log line deduplicates on.
fn line_identity(line: &str) -> String {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|value| {
            value
                .get("event_id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| line.to_string())
}

/// RMS-FR-SUAK: write an attachment the store does not already hold.
///
/// An attachment is named by the digest of its own bytes (`CMS-comments-storage.md`
/// CMS-FR-44), so a file already present under that name is the same file and is
/// left exactly as it is.
fn copy_absent(
    store: &fsa::RootFs,
    worktree: &fsa::RootFs,
    src: &Path,
    dest: &Path,
) -> bool {
    if store.file_info(dest).is_ok() {
        return false;
    }
    let Ok(bytes) = worktree.read_bytes(src) else {
        return false;
    };
    if ensure_dir(store, dest).is_err() {
        return false;
    }
    store.write_bytes_atomic(dest, &bytes).is_ok()
}

/// Create the folders a destination path needs, inside the store alone.
fn ensure_dir(store: &fsa::RootFs, dest: &Path) -> Result<(), String> {
    let Some(parent) = dest.parent() else {
        return Ok(());
    };
    let Ok(rel) = parent.strip_prefix(store.path()) else {
        // A destination outside the store is a bug rather than a folder to
        // create, and it is refused here as well as by the escape gate.
        return Err("destination escapes the store".to_string());
    };
    let mut walked = PathBuf::new();
    for component in rel.components() {
        walked.push(component);
        match store.create_dir_under(store.path(), &walked) {
            Ok(()) => {}
            Err(fsa::FsError::AlreadyExists { .. }) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// RMS-FR-XLTR / PST-FR-MHZB: schedule one import pass for the open project.
///
/// Runs on a thread of its own so it delays neither the open nor the worktree
/// change that scheduled it. A pass that cannot resolve either root does
/// nothing, and a pass that added nothing says nothing: an ordinary project has
/// no legacy folder to read, and a record per open would be noise in a bounded
/// buffer.
pub fn schedule_import<R: tauri::Runtime>(app: &tauri::AppHandle<R>)
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    use tauri::Manager as _;
    let Some(project) = app.try_state::<crate::project::ProjectState>() else {
        return;
    };
    let (Ok(worktree), Ok(store)) = (project.require_root(), project.require_store()) else {
        // RMS-FR-WGQS: a store that could not be resolved is reported where it
        // was resolved, and costs the import and nothing else.
        return;
    };
    let sink = app.clone();
    std::thread::spawn(move || {
        let outcome = import_legacy_storage(&store, &worktree);
        if outcome.is_empty() {
            return;
        }
        // Counts alone: RMS-FR-XRPT keeps every path, draft identity, and line
        // of content out of the record.
        crate::logging::log_info(
            &sink,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "committed conversation and statistics records imported into the repository store",
            crate::log_fields! { "lines" => outcome.lines, "files" => outcome.files },
        );
    });
}
