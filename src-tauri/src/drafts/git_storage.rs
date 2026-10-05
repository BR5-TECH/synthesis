//! What Git is told about draft storage (`DRS-draft-storage.md` DRS-FR-DGMI,
//! DRS-FR-KORQ, DRS-FR-BKFG, DRS-FR-PEAY, DRS-FR-NXWO, DRS-FR-ZIVL,
//! DRS-FR-WYIN, DRS-FR-JDRY, DRS-FR-XPQI, DRS-FR-QHHY, DRS-FR-HRIB,
//! DRS-FR-VECL, DRS-FR-AFSU).
//!
//! Three things live here, and all are about the repository rather than about
//! a draft:
//!
//! - the drafts root's `.gitattributes`, which marks the append-only JSONL logs
//!   for union merge;
//! - the drafts root's `.gitignore`, which keeps the **private draft storage**
//!   of each draft out of Git (DRS-FR-JDRY); and
//! - which paths under the drafts root a draft-event commit of
//!   `PST-project-storage.md` PST-FR-DQZT may name.
//!
//! This module opens no repository, chooses no moment, and writes no commit
//! message. It hands a draft event to the one committer the application has
//! and returns.

use crate::fs as fsa;
use crate::storage_floor::commit::DraftEvent;

use super::{
    ASSETS_DIR, CONVERSATION_FILE, DRAFTS_REL, FILES_DIR, HISTORY_DIR, PROPOSALS_DIR,
    PUBLICATION_FILE, RECORD_FILE,
};

/// DRS-FR-DGMI: the one entry the drafts root's attributes file carries.
///
/// The pattern carries **no slash**, so Git applies it at every depth beneath
/// the drafts root (DRS-FR-PEAY).
const GITATTRIBUTES_ENTRY: &str = "*.jsonl merge=union";

/// The attributes file, relative to the drafts root.
const GITATTRIBUTES_REL: &str = ".gitattributes";

/// The ignore file, relative to the drafts root (DRS-FR-JDRY).
const GITIGNORE_REL: &str = ".gitignore";

/// DRS-FR-JDRY: a directory name shaped like a generated draft id,
/// `<11 hex>-<4 hex>-<8 hex>`, as a Git wildmatch.
///
/// Spelled out one character class at a time, because a pattern that matched
/// any name would ignore a drafts folder the author named `history` or
/// `proposals`, and that folder is the author's (DRS-FR-XPQI).
fn draft_id_glob() -> String {
    let hex = "[0-9a-fA-F]";
    format!("{}-{}-{}", hex.repeat(11), hex.repeat(4), hex.repeat(8))
}

/// DRS-FR-JDRY: the entries the drafts root's ignore file carries, one per item
/// of private draft storage (DRS-FR-ZIVL).
///
/// The leading `**/` reaches a draft at any depth, the root included, because
/// a draft may be filed in any drafts folder (DRS-FR-29).
fn gitignore_entries() -> Vec<String> {
    let id = draft_id_glob();
    [
        format!("{HISTORY_DIR}/"),
        format!("{PROPOSALS_DIR}/"),
        PUBLICATION_FILE.to_string(),
        CONVERSATION_FILE.to_string(),
    ]
    .into_iter()
    .map(|item| format!("**/{id}/{item}"))
    .collect()
}

/// DRS-FR-DGMI / DRS-FR-JDRY: guarantee the drafts root's `.gitattributes` and
/// `.gitignore` carry their entries, preserving every entry already in them
/// (DRS-FR-BKFG).
///
/// The files are **read every time**. Remembering the roots already done would
/// buy a session in which a file cannot come back: a branch switch, a merge, or
/// a `git clean` removes it, and every later write takes the remembered fast
/// path.
pub fn ensure_git_files(root: &fsa::RootFs) -> Result<(), String> {
    ensure_entries(root, GITATTRIBUTES_REL, &[GITATTRIBUTES_ENTRY.to_string()])?;
    ensure_entries(root, GITIGNORE_REL, &gitignore_entries())
}

/// DRS-FR-JDRY / DRS-FR-WYIN: [`ensure_git_files`] for a writer of private
/// draft storage, which must not fail because of it.
///
/// A proposal, a history entry, or a publication record that landed is not
/// taken back because the ignore file could not be written. The next write
/// tries again, and until then the private file reads as untracked rather than
/// being committed: no commit of the application names it (DRS-FR-VECL).
pub fn ensure_private_ignored(root: &fsa::RootFs) {
    if ensure_git_files(root).is_err() {
        // A fixed message and no error text: the error names the file it
        // could not write, and a record carries no path (PST-FR-KGRW).
        crate::storage_floor::commit::record(
            crate::logging::LogLevel::Warn,
            "draft ignore rules not written",
            crate::log_fields! { "reason" => "ensure_failed" },
        );
    }
}

/// Append each missing entry to one file beneath the drafts root, leaving the
/// rest of it exactly as it was.
fn ensure_entries(root: &fsa::RootFs, rel: &str, entries: &[String]) -> Result<(), String> {
    let path = fsa::resolve_under(root, format!("{DRAFTS_REL}/{rel}")).map_err(|e| e.to_string())?;
    let existing = match root.read_text(&path) {
        Ok(text) => text,
        Err(fsa::FsError::NotFound { .. }) => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    let missing: Vec<&String> = entries
        .iter()
        .filter(|entry| !existing.lines().any(|line| line.trim() == entry.as_str()))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    for entry in missing {
        updated.push_str(entry);
        updated.push('\n');
    }
    root.write_text_atomic(&path, &updated).map_err(|e| e.to_string())
}

/// DRS-FR-QHHY: hand one draft event to the committer of
/// `PST-project-storage.md` PST-FR-DQZT.
///
/// This module chooses no moment and writes no message: the caller names the
/// event, and the committer composes the message (PST-FR-YWXF). The call
/// returns at once and cannot fail (DRS-FR-HRIB). It is taken against the
/// **active worktree's** root, which may be a linked worktree (DRS-FR-AFSU).
pub fn commit_event(root: &fsa::RootFs, draft_id: &str, draft_name: &str, event: DraftEvent) {
    crate::storage_floor::commit::draft_event(root, draft_id, draft_name, event);
}

/// DRS-FR-ZIVL / DRS-FR-VECL: whether a path beneath the drafts root is one a
/// draft-event commit for `draft_id` may name.
///
/// `rel` is relative to `.synthesis/drafts/`. The drafts root's two Git files
/// are named by every event. Inside a draft, an event names the **committed
/// draft storage** of that one draft alone — `draft.toml`, `files/`, and
/// `assets/` — wherever the draft is filed, so a draft moved between folders
/// has both of its locations named. A deletion names every path under that
/// draft's folder but a transient one, which is how a private file an earlier
/// build committed reaches the repository as a deletion (DRS-FR-PIWL).
pub fn is_draft_event_rel(rel: &str, draft_id: &str, deletion: bool) -> bool {
    let Some(owned) = segments_of(rel) else {
        return false;
    };
    let segments: Vec<&str> = owned.iter().map(String::as_str).collect();
    match segments.as_slice() {
        [] => false,
        [GITATTRIBUTES_REL] | [GITIGNORE_REL] => true,
        _ => (0..segments.len().saturating_sub(1)).any(|index| {
            segments[index] == draft_id && {
                let rest = &segments[index + 1..];
                if deletion {
                    !rest.iter().any(|name| is_transient_name(name))
                } else {
                    is_committed_inside_draft(rest)
                }
            }
        }),
    }
}

/// A path's segments, in one spelling, or `None` for a path that climbs out.
fn segments_of(rel: &str) -> Option<Vec<String>> {
    let rel = rel.replace('\\', "/");
    let segments: Vec<String> = rel
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(str::to_string)
        .collect();
    if segments.iter().any(|segment| segment == "..") {
        return None;
    }
    Some(segments)
}

/// DRS-FR-ZIVL: whether a name is one of the transient ones this application
/// writes on its way to a durable file.
///
/// The atomic write's `.<name>.tmp.<pid>.<nanos>`, recognised by the predicate
/// the scan already recognises it by, and the `.<name>.synthesis-rename` a
/// case-only rename parks an entry under (`crate::drafts`).
fn is_transient_name(name: &str) -> bool {
    crate::scanning::is_atomic_tmp_name(name)
        || (name.starts_with('.') && name.ends_with(".synthesis-rename"))
}

/// DRS-FR-ZIVL: whether a path inside one draft's folder is **committed draft
/// storage**: the record, the prompt, and the images. The proposals, the
/// history, the publication store, and the conversation log are private draft
/// storage and are named by no commit but a deletion (DRS-FR-WYIN).
fn is_committed_inside_draft(rest: &[&str]) -> bool {
    if rest.iter().any(|name| is_transient_name(name)) {
        return false;
    }
    match rest {
        [] => false,
        [RECORD_FILE] => true,
        [folder, rest @ ..] => !rest.is_empty() && matches!(*folder, FILES_DIR | ASSETS_DIR),
    }
}

#[cfg(test)]
mod tests;
