//! Joining a diverged branch with its upstream on the author's choice, and the
//! guards every write of a pull takes (`GTC-git.md` GTC-FR-NJUY .. GTC-FR-TRWC).
//!
//! **Git settles what Git can settle.** A merge Git settles lands one merge
//! commit at once. A rebase whose replay Git settles, alone or with the merge of
//! the two tips, lands the replayed chain at once. Anything else is handed to a
//! pull run (`GRD-graduation.md` GRD-FR-PLSB), which is the merge run of the
//! stream merge with the local branch as its base and the upstream as its
//! stream. Nothing here writes a branch or a worktree before the landing of
//! `crate::git::land_on_branch`.

use std::path::Path;

use tauri::{Emitter, Manager};

use super::commands::{project_key, project_root};
use super::update_git::{self, Resolutions};
use super::*;
use crate::git::{self as gtc, PullOutput, PullReconcileOutcome, PullStrategy, Upstream};
use crate::graduation::{MergeRunRequest, PullMergeSubject};
use crate::log_fields;
use crate::logging::{self, Domain};

/// GTC-FR-TRWC: take the guards of a pull of the worktree at `root`, which has
/// `branch` out, and hold the repository update guard until the hold drops.
///
/// `None` where no stream state is managed, which is a test without one.
pub fn begin_pull<'a, R: tauri::Runtime>(
    app: &'a tauri::AppHandle<R>,
    root: &Path,
    branch: &str,
) -> Result<Option<state::RepositoryUpdateHold<'a>>, String> {
    let worktree = crate::worktree::active_entry_for(root)?.path;
    if let Some((run_id, pull)) = crate::graduation::live_run_on_worktree(app, &worktree) {
        return Err(if pull {
            format!("{}: {run_id}", gtc::ERR_PULL_RUN_ACTIVE)
        } else {
            format!("{}: {run_id}", crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE)
        });
    }
    if let Some((stream_id, run_id)) = busy_stream_on_branch(app, branch) {
        return Err(format!("{}: {stream_id}, {run_id}", crate::worktree::ERR_WORK_STREAM_BUSY));
    }
    match app.try_state::<StreamState>() {
        Some(state) => state
            .inner()
            .begin_repository_update(&format!("pull:{worktree}"), Reconciliation::Pull)
            .map(Some),
        None => Ok(None),
    }
}

/// What Git alone made of a reconcile.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Joined {
    /// The branch stands on `tip` now, and the landing changed `paths`.
    Landed { tip: git2::Oid, paths: Vec<String> },
    /// Git could not settle it, and nothing was written.
    NeedsRun,
}

/// GTC-FR-SBLF: join the branch `up` names with its upstream through Git alone,
/// and land what Git settled in the worktree `holder` opens.
pub(super) fn join(
    main: &git2::Repository,
    holder: &git2::Repository,
    up: &Upstream,
    strategy: PullStrategy,
) -> Result<Joined, String> {
    let (ahead, behind) =
        main.graph_ahead_behind(up.local, up.remote).map_err(|e| e.to_string())?;
    if behind == 0 {
        return Ok(Joined::Landed { tip: up.local, paths: Vec::new() });
    }
    let land = |tip: git2::Oid| -> Result<Joined, String> {
        let paths = gtc::land_on_branch(holder, &up.branch, up.local, tip)?;
        Ok(Joined::Landed { tip, paths })
    };
    // A branch with nothing of its own takes the upstream as it stands, which is
    // what either strategy makes of it.
    if ahead == 0 {
        return land(up.remote);
    }
    match strategy {
        PullStrategy::Merge => {
            let mut prepared = merge::prepare_between(main, up.local, up.remote)?;
            if !prepared.conflicted_paths.is_empty() {
                return Ok(Joined::NeedsRun);
            }
            let tree = prepared.index.write_tree_to(main).map_err(|e| e.to_string())?;
            land(merge_commit(main, up, tree)?)
        }
        PullStrategy::Rebase => {
            let replay = update_git::replay(main, up.local, up.remote, &Resolutions::new())?;
            if replay.unresolved.is_empty() {
                return land(replay.tip);
            }
            // GRB-FR-EPYG: the merge of the two tips answers what the replay
            // could not, where it can.
            let mut prepared = merge::prepare_between(main, up.remote, up.local)?;
            if !prepared.conflicted_paths.is_empty() {
                return Ok(Joined::NeedsRun);
            }
            let tree = prepared.index.write_tree_to(main).map_err(|e| e.to_string())?;
            match update_git::replay_settled_by(main, up.local, up.remote, tree, &replay.unresolved)
            {
                Ok(landed) => land(landed.tip),
                Err(reason) if reason == update_git::REPLAY_UNSETTLED => Ok(Joined::NeedsRun),
                Err(reason) => Err(reason),
            }
        }
    }
}

/// GTC-FR-SBLF: the message of the merge commit a pull makes.
pub(super) fn merge_message(up: &Upstream) -> String {
    format!("Merge {} into {}", up.upstream, up.branch)
}

/// GTC-FR-SBLF: one merge commit holding `tree`, whose parents are the local
/// tip and then the upstream tip. No reference moves.
pub(super) fn merge_commit(
    main: &git2::Repository,
    up: &Upstream,
    tree: git2::Oid,
) -> Result<git2::Oid, String> {
    let local = main.find_commit(up.local).map_err(|e| e.to_string())?;
    let remote = main.find_commit(up.remote).map_err(|e| e.to_string())?;
    let tree = main.find_tree(tree).map_err(|e| e.to_string())?;
    let signature = main.signature().map_err(|e| e.to_string())?;
    main.commit(None, &signature, &signature, &merge_message(up), &tree, &[&local, &remote])
        .map_err(|e| e.to_string())
}

/// GTC-FR-NJUY: `reconcile_pull`, on the calling thread. Every outcome is
/// reported on the pull's output channels and ends in one terminal event.
pub fn reconcile_pull_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    strategy: PullStrategy,
    local_tip: &str,
    remote_tip: &str,
) -> PullReconcileOutcome {
    let emit = |name: &str, payload: serde_json::Value| {
        let _ = app.emit(name, payload);
    };
    let output = PullOutput::new(&emit);
    let root = match project_root(app) {
        Ok(root) => root,
        Err(error) => return finish(app, &output, PullReconcileOutcome::refused(error)),
    };
    let label = gtc::pull_label(&root);
    let work = || reconcile_in(app, &output, &root, strategy, local_tip, remote_tip);
    let outcome = match app.try_state::<crate::progress::ProgressRegistry>() {
        Some(registry) => crate::progress::attribute(
            app,
            registry.inner(),
            gtc::PROGRESS_KIND_GIT,
            &label,
            Some(root.clone()),
            || Ok::<_, ()>(work()),
        )
        .unwrap_or_default(),
        None => work(),
    };
    // GTC-FR-EGIP: the queue is offered the run only after the guard is gone,
    // because a claim is refused while a reconciliation holds it.
    if outcome.state == gtc::RECONCILE_HANDED_TO_RUN {
        crate::graduation::advance_all_queues(app);
    }
    finish(app, &output, outcome)
}

/// GTC-FR-OYDA: the line that states the outcome, the terminal event, and the
/// record that explains it.
fn finish<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    output: &PullOutput<'_>,
    outcome: PullReconcileOutcome,
) -> PullReconcileOutcome {
    match (&outcome.error, &outcome.run_title) {
        (Some(error), _) => output.line(&format!("Pull refused: {error}")),
        (None, Some(title)) => output.line(&format!("Git could not settle it: {title} started")),
        (None, None) => {
            let tip = outcome.commit.as_deref().unwrap_or_default();
            let tip: String = tip.chars().take(7).collect();
            output.line(&format!("{} at {tip}", capitalized(&outcome.state)));
        }
    }
    output.finished(outcome.error.is_none(), outcome.error.clone());
    let fields = log_fields! {
        "state" => &outcome.state,
        "updatedCount" => outcome.updated_paths.len(),
        "runId" => outcome.run_id.clone().unwrap_or_default(),
    };
    match &outcome.error {
        None => logging::log_info(
            app,
            &logging::BUFFER,
            &[Domain::Backend, Domain::Remote],
            "pull reconciled",
            fields,
        ),
        Some(error) => {
            let mut fields = fields;
            fields.insert("error".to_string(), serde_json::json!(error));
            logging::log_warn(
                app,
                &logging::BUFFER,
                &[Domain::Backend, Domain::Remote],
                "pull reconcile refused",
                fields,
            );
        }
    }
    outcome
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn reconcile_in<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    output: &PullOutput<'_>,
    root: &Path,
    strategy: PullStrategy,
    local_tip: &str,
    remote_tip: &str,
) -> PullReconcileOutcome {
    let holder = match crate::changes::open_repo(root) {
        Ok(repo) => repo,
        Err(error) => return PullReconcileOutcome::refused(error),
    };
    let up = match gtc::resolve_upstream(&holder) {
        Ok(up) => up,
        Err(error) => return PullReconcileOutcome::refused(error),
    };
    output.line(&format!("Pulling {} from {} by {}", up.branch, up.upstream, strategy.as_str()));
    let pinned =
        |up: &Upstream| up.local.to_string() == local_tip && up.remote.to_string() == remote_tip;
    if !pinned(&up) {
        return PullReconcileOutcome::refused(gtc::ERR_PULL_TIPS_MOVED.to_string());
    }
    let hold = match begin_pull(app, root, &up.branch) {
        Ok(hold) => hold,
        Err(error) => return PullReconcileOutcome::refused(error),
    };
    // Read again under the guard, so no write lands on a tip that moved
    // between the check above and the guard.
    let up = match gtc::resolve_upstream(&holder) {
        Ok(up) if pinned(&up) => up,
        _ => return PullReconcileOutcome::refused(gtc::ERR_PULL_TIPS_MOVED.to_string()),
    };
    let main = match git::primary_repo(root) {
        Ok(main) => main,
        Err(error) => return PullReconcileOutcome::refused(error),
    };
    let landed_state = match strategy {
        PullStrategy::Merge => gtc::RECONCILE_MERGED,
        PullStrategy::Rebase => gtc::RECONCILE_REBASED,
    };
    let outcome = match join(&main, &holder, &up, strategy) {
        Ok(Joined::Landed { tip, paths }) => PullReconcileOutcome {
            state: landed_state.to_string(),
            commit: Some(tip.to_string()),
            updated_paths: paths,
            ..PullReconcileOutcome::default()
        },
        Ok(Joined::NeedsRun) => match hand_off(app, root, &main, &up, strategy) {
            Ok((run_id, run_title)) => PullReconcileOutcome {
                state: gtc::RECONCILE_HANDED_TO_RUN.to_string(),
                run_id: Some(run_id),
                run_title: Some(run_title),
                ..PullReconcileOutcome::default()
            },
            Err(error) => PullReconcileOutcome::refused(error),
        },
        Err(error) => PullReconcileOutcome::refused(error),
    };
    drop(hold);
    outcome
}

/// GTC-FR-EGIP: hand an integration Git cannot settle to a pull run. Writes no
/// branch and no worktree; the snapshot ref goes if the run is not created.
fn hand_off<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &Path,
    main: &git2::Repository,
    up: &Upstream,
    strategy: PullStrategy,
) -> Result<(String, String), String> {
    merge_snapshot::refuse_unsupported_between(main, up.local, up.remote)?;
    crate::graduation::image_preflight(app)?;
    let entry = crate::worktree::active_entry_for(root)?;
    // GRD-FR-PQKY: a pull run on a stream's working copy waits in that stream's
    // queue.
    let (stream_id, stream_name) = stream_at_path(app, Path::new(&entry.path))
        .map(|stream| (stream.id, stream.name))
        .unwrap_or_default();
    let run_id = crate::graduation::new_run_id();
    let snapshot = merge_snapshot::capture(main, up.local, up.remote, &run_id)?;
    let request = MergeRunRequest {
        run_id: run_id.clone(),
        project_key: project_key(app),
        stream_id,
        stream_name,
        stream_branch: up.upstream.clone(),
        base_branch: up.branch.clone(),
        base_tip: up.local.to_string(),
        stream_tip: up.remote.to_string(),
        merge_base: snapshot.merge_base.to_string(),
        snapshot_commit: snapshot.commit.to_string(),
        publication: StreamMergePublication::Commit { message: merge_message(up) },
        changed_paths: snapshot.changed_paths.clone(),
        unresolved_paths: snapshot.unresolved_paths.clone(),
        conflicts: snapshot.conflicts.clone(),
        pull: Some((
            PullMergeSubject {
                branch: up.branch.clone(),
                upstream: up.upstream.clone(),
                worktree_path: entry.path.clone(),
                strategy,
            },
            entry.name.clone(),
        )),
    };
    match crate::graduation::enqueue_merge_run(app, request) {
        Ok(run) => {
            logging::log_info(
                app,
                &logging::BUFFER,
                &[Domain::Backend],
                "pull handed to a run",
                log_fields! {
                    "run_id" => run.id.clone(),
                    "strategy" => strategy.as_str(),
                    "unresolved" => snapshot.unresolved_paths.len() as i64,
                },
            );
            let title = run.merge.as_ref().map(|merge| merge.name.clone()).unwrap_or_default();
            Ok((run.id, title))
        }
        Err(reason) => {
            if let Err(release) = merge_snapshot::release_ref(main, &run_id) {
                logging::log_warn(
                    app,
                    &logging::BUFFER,
                    &[Domain::Backend],
                    "pull could not remove an unused snapshot",
                    log_fields! { "reason" => release },
                );
            }
            Err(reason)
        }
    }
}
