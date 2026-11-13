//! Landings: where the work of a completed run stands on a branch
//! (`GRD-graduation.md` GRD-FR-PXQN, GRD-FR-UXXF, GRD-FR-BHFP;
//! `WKS-work-streams.md` WKS-FR-JXPV).
//!
//! A landing names a branch and a commit that branch must reach. A direct run
//! records one on its pinned branch when it completes. A stream run records one
//! on the base branch when its stream merges into that branch. The list of the
//! completed runs of a branch reads these landings against the branch tip, so a
//! run whose landing commit the branch does not reach any more is not listed.
//! A run with a revert commit that the branch reaches is not listed either
//! (GRD-FR-RVXK), because its work is not on the branch.

use super::*;

/// GRD-FR-PXQN: the work of a completed run is on a branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Landing {
    pub branch: String,
    /// The full object id the branch must reach.
    pub commit: String,
    /// RFC 3339 UTC, at second precision, so that the text sorts in time order.
    pub at: String,
}

/// GRD-FR-BHFP: the kind of a completed run of a branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletedRunKind {
    /// A run in a work stream.
    Draft,
    /// A run in the worktree the author stood in (GRD-FR-BSNI).
    Direct,
    /// OCR-FR-QMCG: an apply run of an OpenSpec change.
    ChangeApply,
}

/// GRD-FR-BHFP: one completed run whose work is on a branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCompletedRun {
    pub run_id: String,
    pub title: String,
    pub draft_id: String,
    pub draft_name: String,
    pub kind: CompletedRunKind,
    pub landing: Landing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_summary: Option<RunSummary>,
}

impl GraduationRun {
    /// GRD-FR-PXQN: the landing of this run on `branch`, where it has one.
    pub fn landing_on(&self, branch: &str) -> Option<&Landing> {
        self.landings.iter().find(|landing| landing.branch == branch)
    }

    /// Whether this run has a draft: not a merge run and not a PR summary run.
    pub fn is_draft_run(&self) -> bool {
        !self.is_merge() && !self.is_pr_summary()
    }
}

/// GRD-FR-PXQN: a direct run records a landing on its pinned branch, with its
/// last commit, in the write that completes it. A run that made no commit
/// records none.
pub(crate) fn record_direct_landing(run: &mut GraduationRun) {
    let Some(target) = run.direct_target.as_ref() else {
        return;
    };
    let Some(commit) = run.commits.last() else {
        return;
    };
    if run.landing_on(&target.branch).is_some() {
        return;
    }
    let landing = Landing {
        branch: target.branch.clone(),
        commit: commit.clone(),
        at: crate::notes::now_rfc3339(),
    };
    run.landings.push(landing);
}

/// GRD-FR-RVXK: record the revert of a run, with the last revert commit on the
/// branch of `worktree`. The caller saves the run. A revert that cannot be read
/// back is logged, and the run keeps no revert entry.
pub(crate) fn record_revert<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    worktree: &Path,
) {
    let read = || -> Result<Landing, String> {
        let repo = git2::Repository::open(worktree).map_err(|e| e.to_string())?;
        let head = repo.head().map_err(|e| e.to_string())?;
        let branch = head.shorthand().unwrap_or_default().to_string();
        let commit = head.peel_to_commit().map_err(|e| e.to_string())?.id().to_string();
        Ok(Landing { branch, commit, at: crate::notes::now_rfc3339() })
    };
    match read() {
        Ok(revert) => run.reverts.push(revert),
        Err(reason) => logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not record a revert",
            log_fields! { "run_id" => run.id.clone(), "reason" => reason },
        ),
    }
}

/// GRD-FR-UXXF / WKS-FR-JXPV: a stream merged into `base_branch` at `commit`.
/// Each completed draft run and each completed apply run (OCR-FR-QMCG) of the
/// stream with no landing on that branch records one. A rework run made no
/// commit, so it records none.
///
/// A run that made no commit records none (GRD-FR-PXQN). The call is
/// idempotent: a run that already has a landing on the branch keeps it, so a
/// resumed apply records nothing twice. It returns how many runs recorded a
/// landing. The caller holds no store hold: this takes it (GRD-FR-WSXA).
pub fn record_stream_landings<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    base_branch: &str,
    commit: &str,
) -> usize {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());
    let (Ok(fs), Ok(base)) = (store::store_fs(app), store::store_base(app)) else {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not open the run store to record landings",
            log_fields! { "stream_id" => stream_id.to_string() },
        );
        return 0;
    };
    let project_key = queue::project_key(app);
    let index = store::read_queue_index(&fs, &base, &project_key);
    let at = crate::notes::now_rfc3339();
    let mut recorded = 0;
    for entry in index.runs.iter().filter(|entry| {
        entry.stream_id == stream_id
            && entry.state == GraduationRunState::Completed
            && !entry.merge
            && !entry.pr_summary
    }) {
        let Ok(mut run) = runs::load_run(app, &entry.run_id) else {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation could not read a run to record its landing",
                log_fields! { "run_id" => entry.run_id.clone() },
            );
            continue;
        };
        if !run.is_draft_run() || run.commits.is_empty() || run.landing_on(base_branch).is_some() {
            continue;
        }
        run.landings.push(Landing {
            branch: base_branch.to_string(),
            commit: commit.to_string(),
            at: at.clone(),
        });
        match runs::save_run(app, &mut run) {
            Ok(()) => recorded += 1,
            Err(reason) => logging::log_error(
                app,
                &BUFFER,
                &[Domain::Backend],
                "graduation could not record a landing",
                log_fields! { "run_id" => run.id.clone(), "reason" => reason },
            ),
        }
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation recorded the landings of a stream merge",
        log_fields! {
            "stream_id" => stream_id.to_string(),
            "branch" => base_branch.to_string(),
            "runs" => recorded as i64,
        },
    );
    recorded
}

/// GRD-FR-BHFP: each completed run with a landing on `branch` whose commit the
/// branch tip reaches, oldest landing first.
///
/// It reads local objects alone and writes nothing. A branch that does not
/// exist returns `"unknown branch"`. The walk of the history runs off the
/// thread that serves the window.
#[tauri::command]
pub async fn list_branch_completed_runs<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    branch: String,
) -> Result<Vec<BranchCompletedRun>, String> {
    tauri::async_runtime::spawn_blocking(move || branch_completed_runs_blocking(&app, &branch))
        .await
        .map_err(|error| format!("the completed runs read did not finish: {error}"))?
}

/// The read of [`list_branch_completed_runs`], on the calling thread.
pub(crate) fn branch_completed_runs_blocking<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    branch: &str,
) -> Result<Vec<BranchCompletedRun>, String> {
    let refused = |reason: String| {
        logging::log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not list the completed runs of a branch",
            log_fields! { "branch" => branch.to_string(), "reason" => reason.clone() },
        );
        reason
    };
    let repo = pr_summary::project_repo(app).map_err(refused)?;
    let tip = crate::changes::resolve_branch_commit(&repo, branch).map_err(refused)?;
    let fs = store::store_fs(app).map_err(refused)?;
    let base = store::store_base(app).map_err(refused)?;
    let index = store::read_queue_index(&fs, &base, &queue::project_key(app));
    // The index says which runs are completed draft runs, so no other record is
    // opened (the non-functional requirement of the queue read).
    let candidates = index
        .runs
        .iter()
        .filter(|entry| {
            entry.state == GraduationRunState::Completed && !entry.merge && !entry.pr_summary
        })
        .filter_map(|entry| match store::read_run_record(&fs, &base, &entry.run_id) {
            Ok(run) => Some(run),
            Err(_) => {
                logging::log_warn(
                    app,
                    &BUFFER,
                    &[Domain::Backend],
                    "graduation could not read a completed run for a branch listing",
                    log_fields! { "run_id" => entry.run_id.clone() },
                );
                None
            }
        });
    let listed = completed_runs_on(&repo, tip, branch, candidates);
    logging::log_debug(
        app,
        &BUFFER,
        &[Domain::Backend],
        "graduation listed the completed runs of a branch",
        log_fields! { "branch" => branch.to_string(), "runs" => listed.len() as i64 },
    );
    Ok(listed)
}

/// GRD-FR-BHFP: the runs of `runs` with a landing on `branch` that `tip`
/// reaches and no revert that `tip` reaches, oldest landing first. Runs whose landings fall in the same second
/// keep the order `runs` gives them, which is the project's run order.
pub(crate) fn completed_runs_on(
    repo: &git2::Repository,
    tip: git2::Oid,
    branch: &str,
    runs: impl IntoIterator<Item = GraduationRun>,
) -> Vec<BranchCompletedRun> {
    let reaches = |commit: &str| {
        git2::Oid::from_str(commit)
            .is_ok_and(|oid| oid == tip || repo.graph_descendant_of(tip, oid).unwrap_or(false))
    };
    let mut listed: Vec<BranchCompletedRun> = runs
        .into_iter()
        .filter(|run| run.state == GraduationRunState::Completed && run.is_draft_run())
        .filter_map(|run| {
            let landing = run.landing_on(branch)?.clone();
            if !reaches(&landing.commit) {
                return None;
            }
            // GRD-FR-BHFP, GRD-FR-RVXK: a revert that the branch reaches takes
            // the run's work off the branch.
            if run.reverts.iter().any(|revert| reaches(&revert.commit)) {
                return None;
            }
            Some(BranchCompletedRun {
                kind: if run.is_direct() {
                    CompletedRunKind::Direct
                } else if run.is_change_apply() {
                    CompletedRunKind::ChangeApply
                } else {
                    CompletedRunKind::Draft
                },
                // OCR-FR-FZVF: an apply run is listed under its title.
                title: run.listing_name(),
                draft_id: run.input.draft_id.clone(),
                draft_name: run.input.draft_name.clone(),
                run_id: run.id,
                landing,
                run_summary: run.run_summary,
            })
        })
        .collect();
    listed.sort_by(|a, b| a.landing.at.cmp(&b.landing.at));
    listed
}
