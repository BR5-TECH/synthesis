//! One update of one stream, end to end (GRB-FR-BQNF, GRB-FR-WGPS).
//!
//! An update brings the stream branch up to a **pinned revision** of its base
//! branch. Git settles what Git can settle; what is left is one semantic turn
//! over the unsettled paths alone, in a throwaway checkout, at most three times
//! for one request. Nothing of the stream branch moves until a turn has left a
//! tree that carries no conflict marker, and nothing of the base branch moves
//! at all.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

use super::artifact::{MergeArtifact, MergeConflict};
use super::attempts::{cleanup, AttemptMark};
use super::update_git::{Replay, Resolutions};
use super::update_progress::UpdateProgress;
use super::update_record::{StreamUpdateOutcome, StreamUpdateStrategy};
use super::*;
use crate::graduation::driver::GraduationDispatch;
use crate::log_fields;
use crate::logging::{self, Domain};
use crate::tools::agent_exec::protocol::AgentOutcome;
use crate::tools::agent_exec::{AgentExecution, AgentExecutionError, ProcessOutcome};

/// What the sync half of one attempt found.
enum Plan {
    /// The stream branch already holds the pinned revision.
    NothingToUpdate,
    /// Git settled every path and the result is already on the stream branch.
    Updated { updated_paths: Vec<String> },
    /// A turn is needed, and the checkout it stands in is ready.
    Staged {
        conflicts: Vec<MergeConflict>,
        merge_base: String,
        /// The two revisions the checkout's merged tree was built from.
        base_oid: git2::Oid,
        stream_oid: git2::Oid,
        merged_tree: git2::Oid,
        /// The replayed chain a `rebase_source` update would land, with the
        /// paths its steps could not settle. `merge_source` lands one merge
        /// commit instead and carries none of this.
        replay: Option<(git2::Oid, Vec<String>)>,
        /// GRB-FR-WGPS: how many paths the replay itself could not settle. Not
        /// the question set — that is the tip comparison's — but what a reader
        /// of the log needs to tell a hard replay from a hard merge.
        replay_unresolved: usize,
    },
}

/// What one update request settled, and what stood in its way.
pub(super) struct UpdateReport {
    pub(super) outcome: Result<StreamUpdateOutcome, String>,
    pub(super) conflicts: Vec<StreamMergeConflict>,
}

/// GRB-FR-BQNF: update one stream, at the author's request.
pub(super) async fn run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    strategy: StreamUpdateStrategy,
    base_revision: String,
    decisions: Vec<StreamMergeDecision>,
    dispatch: Arc<dyn GraduationDispatch<R>>,
) -> UpdateReport {
    let mut conflicts: Vec<StreamMergeConflict> = Vec::new();
    let outcome = drive(
        app,
        stream_id,
        strategy,
        &base_revision,
        &decisions,
        dispatch,
        &mut conflicts,
    )
    .await;
    UpdateReport { outcome, conflicts }
}

async fn drive<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    strategy: StreamUpdateStrategy,
    base_revision: &str,
    decisions: &[StreamMergeDecision],
    dispatch: Arc<dyn GraduationDispatch<R>>,
    seen: &mut Vec<StreamMergeConflict>,
) -> Result<StreamUpdateOutcome, String> {
    let buffer = &crate::logging::BUFFER;
    let fs = commands::store_fs(app)?;
    let store = commands::store(app)?;
    let root = commands::project_root(app)?;
    git::primary_repo(&root)?;
    let key = commands::project_key(app);
    let stream = store::read_stream(&fs, &store, stream_id)
        .filter(|stream| stream.project_key == key)
        .ok_or_else(|| {
            commands::refuse(app, "work stream not found", stream_id, ERR_UNKNOWN_STREAM.to_string())
        })?;

    // WKS-FR-HLGN / WKS-FR-TSOA: the repository update guard, held across the
    // pinned-revision check and every write the strategy makes, so the base
    // branch cannot advance between the check and the execution and a merge of
    // any stream of this repository cannot run beside this update.
    let state = app.try_state::<StreamState>();
    let hold = match state.as_ref() {
        Some(state) => match state.begin_repository_update(stream_id, Reconciliation::Update) {
            Ok(hold) => Some(hold),
            Err(reason) => {
                return Err(commands::refuse(
                    app,
                    "work stream update refused",
                    stream_id,
                    reason,
                ))
            }
        },
        None => None,
    };
    let cancellation = hold
        .as_ref()
        .map(|hold| hold.cancellation())
        .unwrap_or_default();

    // WKS-FR-NRQT: refused while a run holds the stream. Read again under the
    // guard: a run may have claimed the stream between the record being loaded
    // and the hold being taken.
    let stream = store::read_stream(&fs, &store, stream_id)
        .filter(|stream| stream.project_key == key)
        .unwrap_or(stream);
    if stream.is_busy() {
        return Err(commands::refuse(
            app,
            "work stream update refused",
            stream_id,
            ERR_STREAM_BUSY.to_string(),
        ));
    }

    let pinned = git2::Oid::from_str(base_revision)
        .map_err(|_| commands::refuse(app, "work stream update refused", stream_id, ERR_STALE_BASE_REVISION.to_string()))?;

    // GRB-FR-CLRO: from here the update really runs, so it becomes visible.
    let progress = UpdateProgress::begin(app, &key, stream_id, &stream.name, strategy);

    let mut conflicted: Vec<String> = Vec::new();
    let mut last_attempt = String::new();
    let mut a_turn_ran = false;
    for turn in 1..=UPDATE_ATTEMPTS_MAX {
        if cancellation.is_cancelled() {
            return Err(cancelled(app, stream_id));
        }
        let attempt_id = artifact::new_attempt_id();
        let artifact = MergeArtifact::new(&store.root(), &attempt_id);
        last_attempt = attempt_id.clone();
        let _attempt = state.as_ref().map(|state| {
            state.begin_attempt(&attempt_id);
            AttemptMark {
                state,
                attempt_id: attempt_id.clone(),
            }
        });

        let planned = match plan(app, &fs, &stream, strategy, pinned, &artifact) {
            Ok(planned) => planned,
            Err(reason) => {
                cleanup(app, &fs, &stream, &artifact);
                return Err(commands::refuse(app, "work stream update refused", stream_id, reason));
            }
        };
        match planned {
            Plan::NothingToUpdate => {
                cleanup(app, &fs, &stream, &artifact);
                logging::log_info(
                    app,
                    buffer,
                    &[Domain::Backend],
                    "work stream update found nothing to update",
                    log_fields! { "stream_id" => stream_id.to_string() },
                );
                return Ok(StreamUpdateOutcome::NothingToUpdate);
            }
            Plan::Updated { updated_paths } => {
                cleanup(app, &fs, &stream, &artifact);
                logging::log_info(
                    app,
                    buffer,
                    &[Domain::Backend],
                    "work stream updated",
                    log_fields! {
                        "stream_id" => stream_id.to_string(),
                        "attempt_id" => attempt_id.clone(),
                        "strategy" => strategy.as_str(),
                        "paths" => updated_paths.len() as i64,
                        "semantic_turns" => (turn - 1) as i64,
                    },
                );
                commands::announce(app, &key);
                return Ok(StreamUpdateOutcome::Updated {
                    attempt_id,
                    updated_paths,
                    semantic_turns: turn - 1,
                });
            }
            Plan::Staged {
                conflicts,
                merge_base,
                base_oid,
                stream_oid,
                merged_tree,
                replay,
                replay_unresolved,
            } => {
                // GRB-FR-WGPS: one turn for the whole update, whatever number of
                // commits a replay rewrote.
                logging::log_info(
                    app,
                    buffer,
                    &[Domain::Backend],
                    "work stream update started a semantic turn",
                    log_fields! {
                        "stream_id" => stream_id.to_string(),
                        "attempt_id" => attempt_id.clone(),
                        "strategy" => strategy.as_str(),
                        "conflicted" => conflicts.len() as i64,
                        "replay_unresolved" => replay_unresolved as i64,
                        "turn" => turn as i64,
                    },
                );
                *seen = conflicts
                    .iter()
                    .map(|conflict| StreamMergeConflict {
                        path: conflict.path.clone(),
                        base_change: conflict.base_change.clone(),
                        stream_change: conflict.stream_change.clone(),
                    })
                    .collect();
                progress.turn(turn, &attempt_id, &conflicts);
                let outcome = semantic_turn::execute_semantic_turn(
                    app,
                    &key,
                    &attempt_id,
                    &artifact,
                    semantic_turn::SemanticTurnInput {
                        stream_name: stream.name.clone(),
                        stream_branch: stream.branch.clone(),
                        base_branch: stream.base_branch.clone(),
                        merge_base_revision: merge_base,
                        conflicts: conflicts.clone(),
                        decisions: decisions.to_vec(),
                    },
                    cancellation.clone(),
                    dispatch.as_ref(),
                )
                .await;
                a_turn_ran |= outcome.is_ok();
                if cancellation.is_cancelled() {
                    cleanup(app, &fs, &stream, &artifact);
                    return Err(cancelled(app, stream_id));
                }
                let staged = StagedAt {
                    base_oid,
                    stream_oid,
                    merged_tree,
                    replay,
                };
                match settle(app, &fs, &stream, strategy, &artifact, &conflicts, &staged, outcome) {
                    Settled::Updated(updated_paths) => {
                        cleanup(app, &fs, &stream, &artifact);
                        logging::log_info(
                            app,
                            buffer,
                            &[Domain::Backend],
                            "work stream updated after a semantic turn",
                            log_fields! {
                                "stream_id" => stream_id.to_string(),
                                "attempt_id" => attempt_id.clone(),
                                "strategy" => strategy.as_str(),
                                "paths" => updated_paths.len() as i64,
                                "semantic_turns" => turn as i64,
                            },
                        );
                        commands::announce(app, &key);
                        return Ok(StreamUpdateOutcome::Updated {
                            attempt_id,
                            updated_paths,
                            semantic_turns: turn,
                        });
                    }
                    Settled::Escalated { reason, questions } => {
                        cleanup(app, &fs, &stream, &artifact);
                        logging::log_warn(
                            app,
                            buffer,
                            &[Domain::Backend],
                            "work stream update escalated to the author",
                            log_fields! {
                                "stream_id" => stream_id.to_string(),
                                "attempt_id" => attempt_id.clone(),
                                "questions" => questions.len() as i64,
                            },
                        );
                        return Ok(StreamUpdateOutcome::Escalated {
                            attempt_id,
                            reason,
                            questions,
                        });
                    }
                    Settled::Unsettled(paths) => {
                        conflicted = paths;
                        cleanup(app, &fs, &stream, &artifact);
                    }
                }
            }
        }
    }

    logging::log_warn(
        app,
        buffer,
        &[Domain::Backend],
        "work stream update spent every attempt",
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "attempt_id" => last_attempt.clone(),
            "conflicted" => conflicted.len() as i64,
            "a_turn_ran" => a_turn_ran,
        },
    );
    if a_turn_ran && !conflicted.is_empty() {
        return Ok(StreamUpdateOutcome::Conflicted {
            attempt_id: last_attempt,
            conflicted_paths: conflicted,
        });
    }
    Err(format!(
        "{ERR_UPDATE_ATTEMPTS_EXHAUSTED}: {}",
        conflicted.join(", ")
    ))
}

/// GRB-FR-TXVL: the author stopped the update. Neither branch moved.
fn cancelled<R: tauri::Runtime>(app: &tauri::AppHandle<R>, stream_id: &str) -> String {
    commands::refuse(
        app,
        "work stream update cancelled",
        stream_id,
        ERR_UPDATE_CANCELLED.to_string(),
    )
}

/// The sync half of one attempt: every Git act, and no await.
fn plan<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    stream: &WorkStream,
    strategy: StreamUpdateStrategy,
    pinned: git2::Oid,
    artifact: &MergeArtifact,
) -> Result<Plan, String> {
    let root = commands::project_root(app)?;
    let main = git::primary_repo(&root)?;

    // WKS-FR-KFVJ: before any clean check and before any write.
    update_git::pinned_tip_holds(&main, &stream.base_branch, &pinned.to_string())?;

    let base_worktree = git::worktree_holding(&main, &stream.base_branch)
        .and_then(|repo| repo.workdir().map(PathBuf::from));
    // PST-FR-RONA: the stream's working copy is what the update checks out, so
    // every draft changed there is saved before the clean check. The base
    // worktree is only read, and a save there would move the pinned revision.
    let access = commands::store_fs(app)?;
    crate::storage_floor::save::save_drafts_before_checkout(&access, &stream.worktree())?;
    update_git::refuse_dirty_for_update(&stream.worktree(), base_worktree.as_deref())?;
    // The tree this update will write, resolved before anything is computed: a
    // refusal that waited for the apply would arrive after a replay and, on a
    // conflicting update, after an agent turn had run for an hour.
    update_git::refuse_unwritable_stream(&main, stream)?;

    let head = update_git::branch_tip(&main, &stream.branch)
        .ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    // GRB-FR-NFEB: a stream that already holds the pinned revision.
    let behind = main
        .graph_ahead_behind(head, pinned)
        .map(|(_, behind)| behind)
        .unwrap_or(0);
    if behind == 0 {
        return Ok(Plan::NothingToUpdate);
    }

    // The replay is computed before the merge comparison for `rebase_source`,
    // because its unsettled set is what decides whether a turn is needed at all
    // (GRB-FR-WGPS). Nothing it writes is referenced by any branch.
    let replay: Option<Replay> = match strategy {
        StreamUpdateStrategy::RebaseSource => {
            Some(update_git::replay(&main, head, pinned, &Resolutions::new())?)
        }
        StreamUpdateStrategy::MergeSource => None,
    };

    // GRB-FR-RMKD: a replay that settled every step on its own is the rebase,
    // and it lands as it stands. Nothing was chosen for the author.
    if let (StreamUpdateStrategy::RebaseSource, Some(replay)) = (&strategy, &replay) {
        if replay.unresolved.is_empty() {
            let updated_paths = update_git::written_paths(&main, head, replay.tree);
            update_git::apply_replay(&main, stream, pinned, replay.tip, replay.tree)?;
            return Ok(Plan::Updated { updated_paths });
        }
    }

    // GRB-FR-EPYG: **Git settles what Git can settle.** The three-side
    // comparison of the two tips answers the paths a replay could not, where it
    // can: a commit that added a line and a later one that removed it leave the
    // tip with nothing to decide. A replay whose questions this comparison
    // answers is replayed again with those answers, so every commit it lands
    // carries what Git settled rather than a side this module chose.
    let mut prepared = merge::prepare_between(&main, pinned, head)?;
    if prepared.conflicted_paths.is_empty() {
        let tree = prepared.index.write_tree_to(&main).map_err(|e| e.to_string())?;
        let updated_paths = update_git::written_paths(&main, head, tree);
        match (&strategy, &replay) {
            (StreamUpdateStrategy::RebaseSource, Some(replay)) => {
                match update_git::replay_settled_by(&main, head, pinned, tree, &replay.unresolved) {
                    Ok(landed) => {
                        // The paths reported are the paths the **landed** tree
                        // writes, which for a replay is not always the tip
                        // comparison's own tree.
                        let landed_paths = update_git::written_paths(&main, head, landed.tree);
                        update_git::apply_replay(&main, stream, pinned, landed.tip, landed.tree)?;
                        return Ok(Plan::Updated {
                            updated_paths: landed_paths,
                        });
                    }
                    // A replay the tip comparison could not answer is a question
                    // for an agent rather than a failure of the request.
                    Err(reason) if reason == update_git::REPLAY_UNSETTLED => {
                        return stage_turn(fs, &main, stream, &mut prepared, artifact, Some(replay));
                    }
                    Err(reason) => return Err(reason),
                }
            }
            _ => update_git::commit_merge_into_stream(&main, stream, head, pinned, tree)?,
        }
        return Ok(Plan::Updated { updated_paths });
    }

    stage_turn(fs, &main, stream, &mut prepared, artifact, replay.as_ref())
}

/// GRB-FR-WGPS: build the throwaway checkout **one** semantic turn stands in.
///
/// One turn for the whole update, whatever number of commits a replay rewrote.
/// The question set is the three-side comparison of the two tips, which is the
/// combined set of everything neither Git nor the replay could settle.
fn stage_turn(
    fs: &fsa::FsAccess,
    main: &git2::Repository,
    stream: &WorkStream,
    prepared: &mut merge::Prepared,
    artifact: &MergeArtifact,
    replay: Option<&Replay>,
) -> Result<Plan, String> {
    let _ = stream;
    let staged = semantic::stage(fs, main, prepared, artifact)
        .map_err(|reason| format!("{ERR_ARTIFACT_GENERATION_FAILED}: {reason}"))?;
    Ok(Plan::Staged {
        conflicts: staged.conflicts,
        merge_base: staged.merge_base,
        base_oid: staged.base_oid,
        stream_oid: staged.stream_oid,
        merged_tree: staged.merged_tree,
        replay_unresolved: replay.map(|r| r.unresolved.len()).unwrap_or(0),
        replay: replay.map(|r| (r.tip, r.unresolved.clone())),
    })
}

/// The two revisions one attempt's checkout was built from, and the chain a
/// rebase would land.
struct StagedAt {
    base_oid: git2::Oid,
    stream_oid: git2::Oid,
    merged_tree: git2::Oid,
    /// The replayed chain and the paths its steps could not settle, where the
    /// strategy is a rebase.
    replay: Option<(git2::Oid, Vec<String>)>,
}

/// What one settled turn left.
enum Settled {
    Updated(Vec<String>),
    Escalated {
        reason: String,
        questions: Vec<crate::graduation::GraduationEscalationQuestion>,
    },
    Unsettled(Vec<String>),
}

/// The sync half after the turn: read what it left, and apply it or nothing.
fn settle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    stream: &WorkStream,
    strategy: StreamUpdateStrategy,
    artifact: &MergeArtifact,
    conflicts: &[MergeConflict],
    staged: &StagedAt,
    outcome: Result<AgentExecution, AgentExecutionError>,
) -> Settled {
    let unsettled: Vec<String> = conflicts.iter().map(|c| c.path.clone()).collect();
    let execution = match outcome {
        Ok(execution) => execution,
        Err(_) => return Settled::Unsettled(unsettled),
    };
    if execution.process_outcome != ProcessOutcome::Completed {
        return Settled::Unsettled(unsettled);
    }
    let Some(response) = execution.response.as_ref() else {
        return Settled::Unsettled(unsettled);
    };
    if response.outcome == AgentOutcome::EscalationRequired {
        return match semantic_turn::escalation_of(response) {
            Some((reason, questions)) => Settled::Escalated { reason, questions },
            None => Settled::Unsettled(unsettled),
        };
    }
    if response.outcome != AgentOutcome::Success {
        return Settled::Unsettled(unsettled);
    }

    let Ok(tree) = semantic::harvest(fs, artifact, staged.merged_tree, conflicts) else {
        warn(app, stream, "what the turn left could not be read back");
        return Settled::Unsettled(unsettled);
    };
    let Ok(root) = commands::project_root(app) else {
        return Settled::Unsettled(unsettled);
    };
    let Ok(main) = git::primary_repo(&root) else {
        return Settled::Unsettled(unsettled);
    };

    // GRB-FR-TXVL: a turn may run for hours. What it corrected answers the two
    // revisions the checkout was built from, so an attempt that finds either
    // has moved is abandoned rather than landed on a stream it does not
    // describe. The pinned revision is checked as a branch position too, which
    // is what WKS-FR-KFVJ asks of every write of this update.
    let head = match update_git::branch_tip(&main, &stream.branch) {
        Some(head) if head == staged.stream_oid => head,
        _ => {
            warn(app, stream, "the stream branch moved while the turn ran");
            return Settled::Unsettled(unsettled);
        }
    };
    if update_git::pinned_tip_holds(&main, &stream.base_branch, &staged.base_oid.to_string())
        .is_err()
    {
        warn(app, stream, "the base branch left the pinned revision");
        return Settled::Unsettled(unsettled);
    }

    let base_worktree = git::worktree_holding(&main, &stream.base_branch)
        .and_then(|repo| repo.workdir().map(PathBuf::from));
    if update_git::refuse_dirty_for_update(&stream.worktree(), base_worktree.as_deref()).is_err() {
        warn(app, stream, "a working copy was dirtied while the turn ran");
        return Settled::Unsettled(unsettled);
    }

    // GRB-FR-WGPS: **one** pass for the complete rebase. What the turn settled
    // is replayed through every step that asked about it, so each commit the
    // author reads carries the decision rather than a side this module chose.
    // The chain is built before anything is validated, because the tree it lands
    // is what must be validated — a replay answers the turn's result only for
    // the paths its own steps asked about, and a turn edits what it likes.
    let landed = match (strategy, staged.replay.as_ref()) {
        (StreamUpdateStrategy::RebaseSource, Some((_, replay_unresolved))) => {
            match update_git::replay_settled_by(
                &main,
                head,
                staged.base_oid,
                tree,
                replay_unresolved,
            ) {
                Ok(landed) => Some(landed),
                Err(reason) => {
                    let stage = if reason == update_git::REPLAY_UNSETTLED {
                        "the replay is still unsettled after the turn"
                    } else {
                        "the replay could not be taken again after the turn"
                    };
                    warn(app, stream, stage);
                    return Settled::Unsettled(unsettled);
                }
            }
        }
        _ => None,
    };
    // A rebase that dropped any of what the turn wrote is abandoned rather than
    // landed: the author would otherwise be told the turn's work was applied.
    // A replay answers the turn only for the paths its own steps asked about,
    // and a turn edits whatever it must to leave a result that holds together.
    if let Some(landed) = landed.as_ref() {
        let dropped = update_git::content_differences(&main, landed.tree, tree);
        if !dropped.is_empty() {
            warn(app, stream, "the replayed chain does not hold what the turn wrote");
            return Settled::Unsettled(dropped);
        }
    }

    // Every path the landed tree changes is read for a leftover marker, not
    // only the paths a question named: a marker in the author's history is the
    // one thing this must never do.
    let landed_tree = landed.as_ref().map(|landed| landed.tree).unwrap_or(tree);
    let written = update_git::written_paths(&main, head, landed_tree);
    let left = semantic::unsettled(fs, artifact, &written);
    if !left.is_empty() {
        warn(app, stream, "the turn left a conflict marker standing");
        return Settled::Unsettled(left);
    }

    let applied = match landed {
        Some(landed) => {
            update_git::apply_replay(&main, stream, staged.base_oid, landed.tip, landed.tree)
        }
        None => update_git::commit_merge_into_stream(&main, stream, head, staged.base_oid, tree),
    };
    match applied {
        Ok(()) => Settled::Updated(written),
        Err(reason) => {
            // The turn may have run for an hour and left a good tree; a write
            // that failed after it is the one outcome nothing else records.
            logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[Domain::Backend],
                "work stream update could not apply what its turn settled",
                log_fields! {
                    "stream_id" => stream.id.clone(),
                    "strategy" => strategy.as_str(),
                    "reason" => reason,
                },
            );
            Settled::Unsettled(unsettled)
        }
    }
}

/// WKS-FR-DHOP: why one attempt settled nothing, where nothing else records it.
fn warn<R: tauri::Runtime>(app: &tauri::AppHandle<R>, stream: &WorkStream, stage: &str) {
    logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[Domain::Backend],
        "work stream update settled nothing this attempt",
        log_fields! { "stream_id" => stream.id.clone(), "stage" => stage.to_string() },
    );
}
