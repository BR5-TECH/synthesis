//! The archive of one change: the CLI, the commit, the restore, and the
//! store move (`OAR-openspec-archive.md` OAR-FR-IAKL, OAR-FR-VAGH,
//! OAR-FR-LZLS, OAR-FR-BLBJ, OAR-FR-EUXZ, OAR-FR-KBDD, OAR-FR-MSOM,
//! OAR-FR-HQAI).

use std::time::Instant;

use super::*;
use crate::openspec::OpenSpecReadError;

/// OAR-FR-BLBJ: whether `path` is below one of the three folders an archive
/// of `change` writes.
fn archive_path(path: &str, change: &str, archived_as: Option<&str>) -> bool {
    let change_folder = format!("openspec/changes/{change}/");
    let in_archive = archived_as
        .map(|name| path.starts_with(&format!("openspec/changes/archive/{name}/")))
        .unwrap_or_else(|| path.starts_with("openspec/changes/archive/"));
    path.starts_with(&change_folder) || path.starts_with("openspec/specs/") || in_archive
}

/// OAR-FR-BLBJ, OAR-FR-KBDD: every path below the folders of the archive of
/// `change` that differs from the branch head in the working copy, both sides
/// of a move included, in text order. `archived_as` is `None` after an archive
/// that did not succeed, where every path below `openspec/changes/archive/`
/// may be one the CLI wrote.
fn archive_writes(
    at: &RootFs,
    change: &str,
    archived_as: Option<&str>,
) -> Result<Vec<String>, String> {
    let entries = crate::git::working_tree_status(at)?;
    let mut paths: Vec<String> = entries
        .into_iter()
        .flat_map(|entry| std::iter::once(entry.path).chain(entry.previous_path))
        .filter(|path| archive_path(path, change, archived_as))
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// OAR-FR-KBDD: whether `dir`, relative to the working copy, is the root of
/// one of the three folders an archive of `change` writes. A restore removes
/// no such root.
fn folder_root(dir: &str, change: &str) -> bool {
    dir == "openspec/specs"
        || dir == "openspec/changes/archive"
        || dir == format!("openspec/changes/{change}")
}

/// OAR-FR-KBDD: Git keeps no folder, so a folder below the three folders that
/// held only paths the restore removed was made by the archive too. Each such
/// folder that is now empty is removed, nearest first, so that a later archive
/// of the change does not find its archive folder standing. It answers the
/// count of folders removed.
fn prune_emptied(at: &RootFs, change: &str, removed: &[String]) -> usize {
    let mut pruned = 0;
    for path in removed {
        let mut dir = std::path::Path::new(path).parent();
        while let Some(rel) = dir {
            let text = rel.to_string_lossy().replace('\\', "/");
            if text.is_empty()
                || folder_root(&text, change)
                || !archive_path(&format!("{text}/"), change, None)
            {
                break;
            }
            // A folder that holds an entry is `NotEmpty`, and its parents
            // hold it, so the walk ends there.
            if at.access().delete_path(at.path().join(rel), false).is_err() {
                break;
            }
            pruned += 1;
            dir = rel.parent();
        }
    }
    pruned
}

/// OAR-FR-KBDD: return each path the archive wrote to the branch head. A
/// tracked path gets its content at the head, and an untracked path is
/// removed. No other path is changed.
///
/// The restore is judged by the working copy and not by the index: after it,
/// no path below the three folders may differ from the head. An index that
/// cannot be written, for example while Git's own `index.lock` is held, is
/// told, and is no failure where the working copy reads clean, because the
/// failed commit did not write the index either.
///
/// It answers whether the working copy reads clean below the three folders
/// after it (OFP-FR-IJYA).
fn restore<R: tauri::Runtime>(cx: &ArchiveContext<'_, R>, change: &str) -> bool {
    let started = Instant::now();
    let mut index_written = true;
    let mut pruned = 0;
    let rolled = archive_writes(&cx.at, change, None).and_then(|paths| {
        if paths.is_empty() {
            return Ok(0);
        }
        let (outcome, flush) = crate::git::rollback_named_paths(&cx.at, &paths)?;
        index_written = flush.is_none();
        let removed: Vec<String> =
            outcome.entries.iter().flat_map(|e| e.removed_paths.iter().cloned()).collect();
        pruned = prune_emptied(&cx.at, change, &removed);
        Ok(paths.len())
    });
    // What still differs from the head after the restore.
    let left = archive_writes(&cx.at, change, None).map(|paths| paths.len());
    let millis = started.elapsed().as_millis() as u64;
    match (rolled, left) {
        (Ok(paths), Ok(0)) => {
            logging::log_info(
                cx.app,
                &BUFFER,
                &[Domain::Backend],
                "openspec archive restored the working copy",
                log_fields! {
                    "stream_id" => cx.stream_id.to_string(),
                    "change" => change.to_string(),
                    "paths" => paths,
                    "folders" => pruned,
                    "indexWritten" => index_written,
                    "durationMs" => millis,
                },
            );
            true
        }
        (rolled, left) => {
            logging::log_warn(
                cx.app,
                &BUFFER,
                &[Domain::Backend],
                "openspec archive could not restore the working copy",
                log_fields! {
                    "stream_id" => cx.stream_id.to_string(),
                    "change" => change.to_string(),
                    "code" => "restore_failed",
                    "paths" => rolled.unwrap_or(0),
                    "left" => left.ok(),
                    "indexWritten" => index_written,
                    "durationMs" => millis,
                },
            );
            false
        }
    }
}

/// OAR-FR-EUXZ: the refusal `archive_failed` of `change` with `code` as its
/// cause and the diagnostics of the failure (OFP-FR-ZDEE). The record holds
/// the code and no diagnostic.
fn refusal<R: tauri::Runtime>(
    cx: &ArchiveContext<'_, R>,
    change: &str,
    code: &str,
    diagnostics: &[crate::openspec::CliDiagnostic],
) -> Refusal {
    logging::log_warn(
        cx.app,
        &BUFFER,
        &[Domain::Backend],
        "openspec archive failed",
        log_fields! {
            "stream_id" => cx.stream_id.to_string(),
            "change" => change.to_string(),
            "code" => code.to_string(),
        },
    );
    let mut failure = OpenSpecFailure::new(ERR_ARCHIVE_FAILED).for_change(change);
    failure.cause = Some(code.to_string());
    failure.diagnostics = diagnostics.to_vec();
    failure.into()
}

/// OAR-FR-EUXZ, OAR-FR-KBDD: a refusal of the merge once the working copy is
/// restored.
fn fail<R: tauri::Runtime>(
    cx: &ArchiveContext<'_, R>,
    change: &str,
    code: &str,
    diagnostics: &[crate::openspec::CliDiagnostic],
) -> Refusal {
    let refused = refusal(cx, change, code, diagnostics);
    restore(cx, change);
    refused
}

/// OAR-FR-IAKL, OFP-FR-IJYA: the archive process, and where its outcome is
/// transient, one more after the restore of the first is complete. A restore
/// that fails ends the archive with its first outcome.
fn run_archive<R: tauri::Runtime>(
    cx: &ArchiveContext<'_, R>,
    seam: &OpenSpec,
    exe: &std::path::Path,
    change: &str,
) -> Result<crate::openspec::ArchiveAnswer, Refusal> {
    let first = match seam.archive_change(cx.app, &BUFFER, exe, cx.at.path(), change) {
        Ok(answer) => return Ok(answer),
        Err(failure) => failure,
    };
    let restored = restore(cx, change);
    if !first.is_transient() || !restored {
        return Err(refusal(cx, change, &first.code(), first.diagnostics()));
    }
    seam.pause_before_archive_retry(cx.app, &BUFFER, &first);
    seam.archive_change(cx.app, &BUFFER, exe, cx.at.path(), change)
        .map_err(|second| fail(cx, change, &second.code(), second.diagnostics()))
}

/// OAR-FR-IAKL through OAR-FR-MSOM: archive `change`, commit what the archive
/// wrote, and move the change's records in the store.
pub(super) fn archive_one<R: tauri::Runtime>(
    cx: &ArchiveContext<'_, R>,
    seam: &OpenSpec,
    change: &str,
) -> Result<ArchivedChange, Refusal> {
    let started = Instant::now();
    // OAR-FR-IAKL, OAR-FR-EUXZ: the CLI that OSM-FR-EZKW resolves now. A mode
    // that went off and a CLI that is not usable start no archive process.
    let exe = match seam.ready_in(cx.app, &BUFFER, cx.active.as_ref(), &cx.at) {
        Ok(exe) => exe,
        Err(error) => {
            let code = match &error {
                OpenSpecReadError::NoOpenProject | OpenSpecReadError::OpenSpecModeInactive => {
                    REASON_MODE_INACTIVE
                }
                other => other.code(),
            };
            return Err(fail(cx, change, code, error.diagnostics()));
        }
    };
    let answer = run_archive(cx, seam, &exe, change)?;
    logging::log_info(
        cx.app,
        &BUFFER,
        &[Domain::Backend],
        "openspec change archived",
        log_fields! {
            "stream_id" => cx.stream_id.to_string(),
            "change" => change.to_string(),
            "archivedAs" => answer.archived_as.clone(),
            "durationMs" => started.elapsed().as_millis() as u64,
        },
    );

    // OAR-FR-LZLS, OAR-FR-BLBJ: one commit of exactly what the archive wrote.
    let committed = archive_writes(&cx.at, change, Some(&answer.archived_as))
        .and_then(|paths| crate::git::commit_exact_paths(&cx.at, &commit_message(change), &paths));
    let commit = match committed {
        Ok(commit) => commit,
        Err(_) => return Err(fail(cx, change, CODE_COMMIT_FAILED, &[])),
    };
    logging::log_info(
        cx.app,
        &BUFFER,
        &[Domain::Backend],
        "openspec archive committed",
        log_fields! {
            "stream_id" => cx.stream_id.to_string(),
            "change" => change.to_string(),
            "archivedAs" => answer.archived_as.clone(),
            "commit" => commit.commit_id.clone(),
            "paths" => commit.committed_paths.len(),
            "durationMs" => started.elapsed().as_millis() as u64,
        },
    );

    // OAR-FR-MSOM, OAR-FR-KSZI: the records move with the change, the notice
    // record of OFP-FR-HVHV included. A move that fails is told and keeps the
    // commit.
    move_change_records(cx.app, cx.stream_id, change, &answer.archived_as);
    Ok(ArchivedChange { change: change.to_string(), archived_as: answer.archived_as })
}
