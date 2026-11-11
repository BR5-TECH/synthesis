//! The finish of a rework run (`OCR-openspec-change-runs.md` OCR-FR-ZNJT
//! through OCR-FR-PBKH; `GOB-graduation-observability.md` GOB-FR-TEAY).
//!
//! A `ready` review of a rework run makes no commit. In its place the run
//! compares each file of the change folder of the reviewed tree with the same
//! path in the stream's working copy, and gives every difference to the author
//! as one change edit proposal in the request thread. Nothing of the stream is
//! written before the author accepts a hunk.

use std::path::{Path, PathBuf};

use super::drive::Step;
use super::turns::note_blocked;
use super::*;
use crate::change_proposals::{AppSites, NewChangeProposal, PathedHunk, RecordRefusal};
use crate::graduation::logs::{self, GraduationLogProducer, StructuredEvent};
use crate::graduation::{change_runs, RunSummary};
use crate::text_hunks::hunks::{self as text, ProposedHunk};

/// What a finish found, before anything is recorded.
struct Found {
    hunks: Vec<PathedHunk>,
    /// Files of the reviewed tree that are not UTF-8 text, which no hunk can
    /// carry.
    skipped: usize,
}

/// GXD-FR-GMDI / OCR-FR-PBKH: the finish, resumed alone. No turn is
/// dispatched and no pass is spent.
pub(super) fn finish_again<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
) -> Step {
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Review,
        observability::StageReason::ReviewStarted,
    );
    if transitions::transition(app, run, GraduationRunState::Reviewing).is_err() {
        return Step::Stop;
    }
    let rationale = run
        .run_summary
        .as_ref()
        .map(|summary| summary.description.clone())
        .or_else(|| run.observability.passes.last().and_then(|pass| pass.rationale.clone()))
        .unwrap_or_default();
    record(app, run, worktree, &rationale)
}

/// OCR-FR-ZNJT, GRD-FR-UUGP: a `ready` review of a rework run leads to the
/// finish. The run summary is kept first, so a finish that is blocked and
/// continued records the same proposal.
pub(super) fn finish<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
    summary: Option<RunSummary>,
    verdict: &ReviewVerdict,
) -> Step {
    // OCR-FR-WBZX: the rationale is the description of the run summary, or the
    // verdict's rationale where the review gave no summary.
    let rationale = summary
        .as_ref()
        .map(|summary| summary.description.clone())
        .unwrap_or_else(|| verdict.rationale.clone());
    run.run_summary = summary;
    let _ = crate::graduation::save_run(app, run);
    record(app, run, worktree, &rationale)
}

/// OCR-FR-ZNJT through OCR-FR-PBKH: compare, then record the proposal or the
/// comment, then complete the run; or rest it `blocked` at the phase `record`.
fn record<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
    rationale: &str,
) -> Step {
    // A discard or a pause that landed between the verdict and the finish
    // stands: the store answers, not the copy in hand.
    if crate::graduation::load_run(app, &run.id).is_ok_and(|stored| {
        stored.state.is_terminal() || stored.state == GraduationRunState::Interrupted
    }) {
        return Step::Stop;
    }
    let Some(data) = run.openspec.clone() else {
        return Step::Stop;
    };
    let thread_id = data.thread_id.clone().unwrap_or_default();
    let requester = data.requester.clone().unwrap_or_else(crate::comments::Participant::synthesis);
    let found = match compare(app, run, worktree, &data.change) {
        Ok(found) => found,
        Err(reason) => return block(app, run, change_runs::ERR_WRITE_FAILED, &reason),
    };
    if found.skipped > 0 {
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "graduation left files out of a rework proposal: they are not text",
            log_fields! { "run_id" => run.id.clone(), "files" => found.skipped as u64 },
        );
    }
    let store = match change_runs::machine_store(app) {
        Ok(store) => store,
        Err(reason) => return block(app, run, change_runs::ERR_WRITE_FAILED, &reason),
    };
    let hunk_count = found.hunks.len();
    let mut proposal_id = None;
    if !found.hunks.is_empty() {
        let recorded = crate::change_proposals::record_rework_proposal(
            app,
            &store,
            &AppSites { app },
            NewChangeProposal {
                stream_id: &run.stream_id,
                change: &data.change,
                hunks: &found.hunks,
                rationale,
                agent: &requester,
                thread_id: &thread_id,
            },
            &run.id,
        );
        match recorded {
            Ok(proposal) => proposal_id = Some(proposal.id),
            // The store found that the hunks change nothing as it would write
            // them, which is the finish that found no difference.
            Err(RecordRefusal::NoChange) => {}
            Err(RecordRefusal::ProposalPending) => {
                return block(app, run, change_runs::ERR_PROPOSAL_PENDING, "proposal_pending")
            }
            Err(RecordRefusal::ThreadResolved) => {
                return block(app, run, change_runs::ERR_DISCUSSION_RESOLVED, "discussion_resolved")
            }
            Err(other) => return block(app, run, change_runs::ERR_WRITE_FAILED, other.code()),
        }
    }
    // OCR-FR-KXJX: no difference is one comment of the fixed text.
    if proposal_id.is_none() {
        if let Err(code) = comment(app, &store, run, &data.change, &thread_id, &requester) {
            return block(app, run, &code, &code);
        }
    }
    complete(app, run, proposal_id, hunk_count)
}

/// OCR-FR-GWGE, GOB-FR-TEAY: the finish recorded what it found, so the run
/// enters `done` with the reason `finished` and moves to `completed`.
fn complete<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    proposal_id: Option<String>,
    hunk_count: usize,
) -> Step {
    if let Some(data) = run.openspec.as_mut() {
        data.proposal_id = proposal_id.clone();
    }
    run.checkpoint.resume_part = None;
    transitions::clear_block_count(run);
    let _ = crate::graduation::save_run(app, run);
    let recorded = StructuredEvent::new(GraduationLogProducer::Commit, "rework recorded")
        .with("hunks", hunk_count as u64)
        .with("proposal", proposal_id.is_some());
    let _ = logs::emit(app, run, recorded);
    let _ = transitions::enter_stage(
        app,
        run,
        observability::GraduationVisualStage::Done,
        observability::StageReason::Finished,
    );
    let _ = transitions::transition(app, run, GraduationRunState::Completed);
    // OCR-FR-QDOZ: the run id, the stream, the change, the phase, and the
    // hunk count. No hunk and no text of a file.
    logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation finished a rework run",
        log_fields! {
            "run_id" => run.id.clone(),
            "stream_id" => run.stream_id.clone(),
            "change" => run.openspec.as_ref().map(|d| d.change.clone()).unwrap_or_default(),
            "phase" => change_runs::PHASE_REWORK,
            "hunks" => hunk_count as u64,
            "proposal_id" => proposal_id.unwrap_or_default(),
        },
    );
    Step::Stop
}

/// OCR-FR-PBKH: rest the run `blocked` with the code of the refusal, at the
/// resume phase `record`. Continue retries the finish alone.
fn block<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    code: &str,
    reason: &str,
) -> Step {
    note_blocked(app, run, code);
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "graduation could not finish a rework run",
        log_fields! {
            "run_id" => run.id.clone(),
            "stream_id" => run.stream_id.clone(),
            "code" => code.to_string(),
            "reason" => reason.to_string(),
        },
    );
    let (message, clears_by) = match code {
        change_runs::ERR_PROPOSAL_PENDING => (
            "A change edit proposal of this change waits for the author's decision, so the result of the rework was not recorded.",
            "Decide the open proposal, then continue the run.",
        ),
        change_runs::ERR_DISCUSSION_RESOLVED => (
            "The request thread of this rework run is resolved, so the result of the rework was not recorded.",
            "Reopen the request thread, then continue the run.",
        ),
        _ => (
            "The result of the rework could not be recorded.",
            "Continue the run to record the result again.",
        ),
    };
    let _ = transitions::block(app, run, code, message, clears_by, phases::PART_RECORD);
    Step::Stop
}

/// OCR-FR-KXJX: one comment of the fixed text, into the request thread, as
/// the requester.
fn comment<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &crate::fs::RootFs,
    run: &GraduationRun,
    change: &str,
    thread_id: &str,
    requester: &crate::comments::Participant,
) -> Result<(), String> {
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: run.stream_id.clone(),
        change: change.to_string(),
    };
    let thread = crate::comments::locate_discussion_of(store, &target, thread_id)
        .ok_or_else(|| change_runs::ERR_WRITE_FAILED.to_string())?;
    if thread.resolved {
        return Err(change_runs::ERR_DISCUSSION_RESOLVED.to_string());
    }
    let appended = crate::comments::append_agent_comment_to(
        store,
        thread.log_ref(),
        thread_id,
        change_runs::nothing_to_change_comment(change),
        requester,
        &crate::notes::now_rfc3339(),
    )
    .map_err(|error| {
        if error == crate::comments::ERR_DISCUSSION_RESOLVED {
            change_runs::ERR_DISCUSSION_RESOLVED.to_string()
        } else {
            change_runs::ERR_WRITE_FAILED.to_string()
        }
    })?;
    crate::comments::emit_discussion_changed(app, &appended);
    Ok(())
}

/// OCR-FR-ZNJT, OCR-FR-UTFI: each file of the change folder of the reviewed
/// tree against the same path in the stream's working copy, as hunks in the
/// order of their paths.
fn compare<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
    worktree: &Path,
    change: &str,
) -> Result<Found, String> {
    let stream = crate::streams::stream_of(app, &run.stream_id)
        .ok_or_else(|| crate::graduation::ERR_UNKNOWN_STREAM.to_string())?;
    let copy = crate::fs::RootFs::new(stream.worktree(), crate::graduation::store_fs(app)?);
    let folder = format!("openspec/changes/{change}");
    let mut files =
        reviewed_files(worktree, run.base_commit.as_deref().unwrap_or_default(), &folder)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut found = Found { hunks: Vec::new(), skipped: 0 };
    for (rel, bytes) in files {
        let Ok(next) = String::from_utf8(bytes) else {
            found.skipped += 1;
            continue;
        };
        let path = PathBuf::from(&folder).join(&rel);
        let current = match copy.read_bytes(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Some(text),
                Err(_) => {
                    found.skipped += 1;
                    continue;
                }
            },
            Err(crate::fs::FsError::NotFound { .. }) => None,
            Err(error) => return Err(error.to_string()),
        };
        for hunk in hunks_of(current.as_deref(), &next) {
            found.hunks.push(PathedHunk { path: rel.clone(), hunk });
        }
    }
    Ok(found)
}

/// OCR-FR-UTFI: the line hunks of PCP-FR-CNVT, or one hunk over the whole
/// file where those cannot each be found once. A file the working copy does
/// not hold is one new-file hunk (CEP-FR-HRCX). Equal texts give none.
fn hunks_of(current: Option<&str>, next: &str) -> Vec<ProposedHunk> {
    let Some(current) = current else {
        return text::whole_document("", next);
    };
    let normal = crate::text_hunks::anchors::normalise_newlines;
    if normal(current) == normal(next) {
        return Vec::new();
    }
    let changes = crate::text_hunks::line_diff::line_changes(current, next);
    if !changes.is_empty() && text::plan(current, &changes, crate::notes::new_note_id).is_ok() {
        return changes;
    }
    text::whole_document(current, next)
}

/// GRL-FR-XNQU: the files below `folder` of the reviewed tree, which is the
/// run's change set over its seed commit, with their change-relative paths.
fn reviewed_files(
    worktree: &Path,
    base: &str,
    folder: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
    let tree = match crate::graduation::commit::run_tree(worktree, base)? {
        Some(derived) => repo.find_tree(derived.tree).map_err(|e| e.to_string())?,
        None => {
            let oid = git2::Oid::from_str(base).map_err(|e| e.to_string())?;
            repo.find_commit(oid).and_then(|c| c.tree()).map_err(|e| e.to_string())?
        }
    };
    let Some(sub) = tree
        .get_path(Path::new(folder))
        .ok()
        .and_then(|entry| entry.to_object(&repo).ok())
        .and_then(|object| object.into_tree().ok())
    else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    let mut failed = None;
    let _ = sub.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() != Some(git2::ObjectType::Blob) {
            return git2::TreeWalkResult::Ok;
        }
        let Ok(name) = entry.name() else {
            return git2::TreeWalkResult::Ok;
        };
        match repo.find_blob(entry.id()) {
            Ok(blob) => files.push((format!("{root}{name}"), blob.content().to_vec())),
            Err(error) => {
                failed = Some(error.to_string());
                return git2::TreeWalkResult::Abort;
            }
        }
        git2::TreeWalkResult::Ok
    });
    match failed {
        Some(reason) => Err(reason),
        None => Ok(files),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // OCR-FR-UTFI: equal texts give no hunk, a changed line gives a line hunk,
    // and a file the working copy does not hold gives one new-file hunk.
    #[test]
    fn a_file_becomes_line_hunks_or_one_new_file_hunk() {
        assert!(hunks_of(Some("a\nb\n"), "a\nb\n").is_empty());
        assert!(hunks_of(Some("a\r\nb\r\n"), "a\nb\n").is_empty());

        let changed = hunks_of(Some("one\ntwo\nthree\n"), "one\nTWO\nthree\n");
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].before.as_deref(), Some("two\n"));
        assert_eq!(changed[0].after.as_deref(), Some("TWO\n"));

        let new = hunks_of(None, "# Design\n");
        assert_eq!(new.len(), 1);
        // CEP-FR-HRCX: a new file's hunk is an `add` with no anchor.
        assert_eq!(new[0].kind, crate::text_hunks::anchors::HunkKind::Add);
        assert_eq!(new[0].before, None);
        assert_eq!((new[0].after_text.as_deref(), new[0].lead.as_deref()), (None, None));
        assert_eq!(new[0].after.as_deref(), Some("# Design\n"));
    }

    // OCR-FR-UTFI: where the line hunks cannot each be found once, the file is
    // one hunk over the whole text. A passage repeated with more context than a
    // hunk's lead and trail can hold on each side is such a file.
    #[test]
    fn a_file_of_repeated_lines_becomes_one_whole_file_hunk() {
        let filler = "p\n".repeat(text::LEAD_TRAIL_BOUND);
        let current = format!("{filler}x\n{filler}x\n{filler}");
        let next = format!("{filler}y\n{filler}x\n{filler}");
        let (current, next) = (current.as_str(), next.as_str());
        let lines = crate::text_hunks::line_diff::line_changes(current, next);
        assert!(
            text::plan(current, &lines, crate::notes::new_note_id).is_err(),
            "the line hunks of this file are not each found once"
        );
        let hunks = hunks_of(Some(current), next);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].kind, crate::text_hunks::anchors::HunkKind::Replace);
        assert_eq!(hunks[0].before.as_deref(), Some(current));
        assert_eq!(hunks[0].after.as_deref(), Some(next));
    }

    // OCR-FR-UTFI: two separate changed passages are two hunks.
    #[test]
    fn two_changed_passages_are_two_hunks() {
        let current = "a\nb\nc\nd\ne\nf\n";
        let next = "a\nB\nc\nd\nE\nf\n";
        assert_eq!(hunks_of(Some(current), next).len(), 2);
    }
}
