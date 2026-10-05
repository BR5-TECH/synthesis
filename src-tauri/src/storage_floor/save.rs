//! The save commit a stream checkout waits for (`PST-project-storage.md`
//! PST-FR-RONA, `WKS-work-streams.md` WKS-FR-JVLM).
//!
//! A save of a draft's prompt makes no commit (PST-FR-DQZT), so a committed
//! draft the author edited stands modified in its worktree. A stream merge or
//! update checks a worktree out with force, and a forced checkout reverts every
//! modified tracked file to the tree it writes. So before the checkout, each
//! changed draft of that worktree is committed under its own `draft: save`
//! message, and the operation waits for it. A save that is not taken refuses
//! the operation: losing the author's prompt is never the alternative.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use super::commit::{message, DraftEvent};
use crate::fs::{FsAccess, RootFs};
use crate::log_fields;
use crate::logging::LogLevel;

/// PST-FR-RONA: the typed refusal of an operation whose save was not taken.
pub const ERR_DRAFT_SAVE_FAILED: &str = "draft_save_failed";

/// PST-FR-RONA: commit every draft of `worktree` whose committed draft storage
/// differs from `HEAD`, one commit per draft, before a checkout overwrites it.
///
/// Returns how many save commits were taken. A worktree that is no repository,
/// or holds no changed draft, takes none and is not refused. Every file is
/// reached through `access`, the instance the open project shares (FSA-FR-21).
pub fn save_drafts_before_checkout(access: &Arc<FsAccess>, worktree: &Path) -> Result<usize, String> {
    let Ok(repo) = git2::Repository::open(worktree) else {
        return Ok(0);
    };
    let groups = changed_drafts(&repo);
    if groups.is_empty() {
        return Ok(0);
    }
    let mut saved = 0;
    for ((prefix, draft_id), paths) in groups {
        let project = if prefix.is_empty() {
            worktree.to_path_buf()
        } else {
            worktree.join(&prefix)
        };
        let root = RootFs::new(project, Arc::clone(access));
        // A worktree the shared instance does not cover cannot be read, so it
        // cannot be saved; the checkout that would follow is refused rather
        // than allowed to revert what could not be saved (FSA-FR-21).
        if root.file_info(root.path()).is_err() {
            super::commit::record(
                LogLevel::Warn,
                "draft save before checkout refused",
                log_fields! { "reason" => "worktree_not_covered", "saved" => saved },
            );
            return Err(ERR_DRAFT_SAVE_FAILED.to_string());
        }
        let name = crate::drafts::draft_record(&root, &draft_id)
            .map(|record| record.name)
            .unwrap_or_else(|_| draft_id.clone());
        match commit_one(&repo, &root, &message(DraftEvent::Saved, &name), &paths) {
            Ok(true) => saved += 1,
            Ok(false) => {}
            Err(reason) => {
                // A fixed token, and no path or name: the record travels to
                // the Logs panel and to any file a user exports (PST-FR-KGRW).
                super::commit::record(
                    LogLevel::Warn,
                    "draft save before checkout refused",
                    log_fields! { "reason" => reason, "saved" => saved },
                );
                return Err(ERR_DRAFT_SAVE_FAILED.to_string());
            }
        }
    }
    super::commit::record(
        LogLevel::Info,
        "drafts saved before checkout",
        log_fields! { "saved" => saved },
    );
    Ok(saved)
}

/// Every tracked path of committed draft storage that differs from `HEAD`,
/// grouped by the project prefix it stands under and the draft it belongs to.
///
/// No rename is paired: each path is read on its own terms, so a moved draft
/// is saved at both of its locations (`DRS-draft-storage.md` DRS-FR-VECL).
fn changed_drafts(repo: &git2::Repository) -> BTreeMap<(String, String), Vec<String>> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false).include_ignored(false);
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let Ok(statuses) = repo.statuses(Some(&mut opts)) else {
        return groups;
    };
    // The drafts root's own Git files ride with the first save of their
    // project, as every draft-event commit may name them (DRS-FR-VECL).
    let mut root_files: Vec<(String, String)> = Vec::new();
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let Some((prefix, rel)) = split_at_drafts(path) else {
            continue;
        };
        let named = format!("{}/{rel}", super::DRAFTS_REL);
        if is_root_git_file(&rel) {
            root_files.push((prefix, named));
            continue;
        }
        let Some(draft_id) = draft_of(&rel) else { continue };
        groups.entry((prefix, draft_id)).or_default().push(named);
    }
    for (prefix, named) in root_files {
        if let Some((_, paths)) = groups.iter_mut().find(|((p, _), _)| *p == prefix) {
            paths.push(named);
        }
    }
    groups
}

/// The drafts root's `.gitattributes` and `.gitignore` (DRS-FR-BKFG).
pub(crate) fn is_root_git_file(rel: &str) -> bool {
    matches!(rel, ".gitattributes" | ".gitignore")
}

/// A repository-relative path as the project prefix it stands under and its
/// path beneath that project's drafts root, where it has one.
pub(crate) fn split_at_drafts(path: &str) -> Option<(String, String)> {
    let marker = format!("{}/", super::DRAFTS_REL);
    let mut start = 0;
    loop {
        let tail = &path[start..];
        if let Some(rel) = tail.strip_prefix(&marker) {
            let prefix = path[..start].trim_end_matches('/').to_string();
            return Some((prefix, rel.to_string()));
        }
        let next = tail.find('/')?;
        start += next + 1;
    }
}

/// The draft whose committed draft storage a path beneath the drafts root is,
/// if any (`DRS-draft-storage.md` DRS-FR-ZIVL).
fn draft_of(rel: &str) -> Option<String> {
    rel.split('/')
        .find(|segment| crate::drafts::is_draft_event_rel(rel, segment, false) && !segment.starts_with('.'))
        .map(str::to_string)
}

/// One save commit under the index lock: `Ok(true)` taken, `Ok(false)` nothing
/// left to commit, `Err` a fixed reason the save was not taken.
fn commit_one(
    repo: &git2::Repository,
    root: &RootFs,
    message: &str,
    paths: &[String],
) -> Result<bool, &'static str> {
    if repo.state() != git2::RepositoryState::Clean {
        return Err("repository_busy");
    }
    if repo.signature().is_err() {
        return Err("no_authoring_identity");
    }
    let Ok(Some(_lock)) = crate::git::index_lock::IndexLock::try_acquire(repo) else {
        return Err("index_held");
    };
    match crate::git::commit_exact_paths(root, message, paths) {
        Ok(_) => Ok(true),
        Err(reason) if reason == crate::git::ERR_NOTHING_TO_COMMIT => Ok(false),
        Err(_) => Err("commit_failed"),
    }
}

#[cfg(test)]
mod tests;
