//! Direct graduation: a run that works in the author's own worktree
//! (`GSU-graduation-start.md` GSU-FR-PVFP, `GRD-graduation.md` GRD-FR-BSNI).
//!
//! The run pins the worktree that is active when the author confirms, and the
//! branch it holds then. Everything after the start is the lifecycle of any
//! other run: the same queue rules, the same agent turns, the same review and
//! the same commit. What this module adds is the pinning, the preflight that
//! refuses a worktree the run could not start from, and the two checks that keep
//! the run from writing anywhere else — the dispatch gate (GRD-FR-XRDY) and the
//! commit guard (GRD-FR-VAUE).

use std::path::{Path, PathBuf};

use super::*;

/// GSU-FR-MZGD: what the active worktree says about a direct start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectGraduationPreflight {
    /// Absolute path of the active worktree.
    pub worktree_path: String,
    /// The directory's basename.
    pub worktree_name: String,
    /// The branch the worktree holds; absent when it is detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub is_detached: bool,
    /// The work stream the worktree is the working copy of, if it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<crate::worktree::WorktreeStream>,
    /// Every uncommitted path, complete.
    pub dirty_paths: Vec<String>,
}

/// GSU-FR-MZGD: read the active worktree for a direct start.
///
/// Read-only. It creates nothing, and the start uses the same reading, so the
/// dialog and the refusal cannot disagree about what is dirty.
#[tauri::command]
pub fn preflight_direct_graduation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<DirectGraduationPreflight, String> {
    read_preflight(&app).inspect_err(|reason| {
        logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "graduation could not read the active worktree for a direct start",
            log_fields! { "code" => typed_code(reason) },
        );
    })
}

/// GSU-FR-PVFP: start a direct run, or start nothing.
#[tauri::command]
pub fn start_direct_graduation<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    draft_id: String,
    expected_worktree: String,
    expected_branch: String,
) -> Result<GraduationRun, String> {
    let state = app.try_state::<GraduationState>();
    let _write = state.as_ref().map(|state| state.write_guard());

    let refuse = |reason: String| -> String {
        logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "graduation refused a direct start",
            log_fields! { "draft_id" => draft_id.clone(), "code" => typed_code(&reason) },
        );
        reason
    };

    let project_key = queue::project_key(&app);
    if project_key.is_empty() {
        return Err(refuse(ERR_NO_PROJECT_OPEN.to_string()));
    }
    // GSU-FR-IJEE: the draft checks come first.
    queue::require_unlocked_draft(&app, &draft_id).map_err(&refuse)?;
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .ok_or_else(|| refuse(ERR_NO_PROJECT_OPEN.to_string()))?;
    let record = crate::drafts::read_draft_prompt(&root, &draft_id).map_err(|reason| {
        refuse(if reason == crate::drafts::ERR_NOT_SINGLE_FILE {
            ERR_DRAFT_NOT_SINGLE_FILE.to_string()
        } else {
            ERR_DRAFT_NOT_FOUND.to_string()
        })
    })?;

    // GSU-FR-SZTZ: identity, branch, then cleanliness, each before the image
    // preflight and each before anything is created.
    let preflight = read_preflight(&app).map_err(&refuse)?;
    check_preflight(&preflight, &expected_worktree, &expected_branch).map_err(&refuse)?;
    let branch = preflight.branch.clone().unwrap_or_default();

    image_preflight(&app).map_err(&refuse)?;

    let input = CapturedGraduationInput {
        draft_id: draft_id.clone(),
        draft_name: record.draft.name.clone(),
        prompt: record.content.clone(),
        prompt_checksum: crate::fs::sha256_bytes(record.content.as_bytes()),
        captured_at: crate::notes::now_rfc3339(),
    };
    let target = DirectTarget {
        worktree_path: preflight.worktree_path.clone(),
        worktree_name: preflight.worktree_name.clone(),
        branch,
    };
    let (stream_id, stream_name) = preflight
        .stream
        .as_ref()
        .map(|stream| (stream.stream_id.clone(), stream.stream_name.clone()))
        .unwrap_or_default();
    // GSU-FR-ZTTS: a direct run asks no standing-work choice, so it records the
    // choice that commits nothing and carries no message.
    let mut run = commands::build_run(
        &project_key,
        stream_id,
        stream_name,
        Some(target),
        input,
        StandingWork::Keep,
        None,
    );
    crate::graduation::logs::initialize(&app, &mut run);
    runs::save_run(&app, &mut run).map_err(&refuse)?;
    // GSU-FR-RNOM: the run is enqueued, so the graduation-start draft commit is
    // asked for now and never for a refused start. Nothing waits for it.
    commands::commit_graduation_start(&app, &root, &draft_id, &record.draft.name);
    logging::log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "graduation enqueued a direct run",
        log_fields! {
            "run_id" => run.id.clone(),
            "queue" => if run.stream_id.is_empty() { "worktree" } else { "stream" },
        },
    );
    events::announce_queue(&app, &run);
    let _ = crate::drafts::announce(&app, crate::drafts::DraftChange::to(&draft_id, Vec::new()));
    scheduler::dispatch_pending(&app);
    Ok(run)
}

/// GSU-FR-SZTZ: the checks a direct start makes against what the active
/// worktree reports, in the order the specification names.
pub(super) fn check_preflight(
    preflight: &DirectGraduationPreflight,
    expected_worktree: &str,
    expected_branch: &str,
) -> Result<(), String> {
    // GTC-FR-31: the identity guard, as every other write bound to a checkout
    // takes it.
    if crate::changes::canonicalize_lenient(Path::new(expected_worktree))
        != crate::changes::canonicalize_lenient(Path::new(&preflight.worktree_path))
    {
        return Err(crate::git::worktree_identity_error(
            expected_worktree,
            &preflight.worktree_path,
        ));
    }
    let Some(branch) = preflight.branch.as_deref() else {
        return Err(ERR_DIRECT_WORKTREE_DETACHED.to_string());
    };
    if branch != expected_branch {
        return Err(ERR_DIRECT_BRANCH_CHANGED.to_string());
    }
    if !preflight.dirty_paths.is_empty() {
        return Err(format!(
            "{ERR_DIRECT_WORKTREE_DIRTY}: {}",
            preflight.dirty_paths.join(", ")
        ));
    }
    Ok(())
}

fn read_preflight<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<DirectGraduationPreflight, String> {
    let root = app
        .try_state::<ProjectState>()
        .and_then(|state| state.require_root().ok())
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let entry = crate::worktree::active_entry_for(&root).map_err(|reason| {
        if reason == crate::changes::ERR_NOT_A_REPO {
            ERR_NOT_A_GIT_REPOSITORY.to_string()
        } else {
            reason
        }
    })?;
    let mut entries = [entry];
    crate::streams::decorate_worktrees(app, &mut entries);
    let [entry] = entries;
    let dirty_paths = crate::streams::uncommitted_paths_of(Path::new(&entry.path));
    Ok(DirectGraduationPreflight {
        worktree_name: entry.name,
        worktree_path: entry.path,
        is_detached: entry.is_detached || entry.branch.is_none(),
        branch: entry.branch,
        stream: entry.stream,
        dirty_paths,
    })
}

// ---------------------------------------------------------------------------
// Dispatch and commit guards
// ---------------------------------------------------------------------------

/// The branch a worktree holds, or `None` where it is detached or gone.
pub(crate) fn branch_of(worktree: &Path) -> Option<String> {
    if !worktree.is_dir() {
        return None;
    }
    crate::worktree::head_of(worktree).0
}

/// GRD-FR-XRDY: whether a direct run may be dispatched now.
///
/// A worktree whose directory is gone is let through: the loop blocks the run
/// with a typed cause, which says more than a hold that waits for a branch of a
/// directory that is not there.
pub(super) fn dispatch_hold(target: &DirectTarget) -> Option<TargetHold> {
    let path = Path::new(&target.worktree_path);
    if !path.is_dir() {
        return None;
    }
    let actual = branch_of(path);
    if actual.as_deref() == Some(target.branch.as_str()) {
        return None;
    }
    Some(TargetHold {
        code: HOLD_TARGET_BRANCH_CHANGED.to_string(),
        expected_branch: target.branch.clone(),
        actual_branch: actual,
    })
}

/// GRD-FR-XRDY: keep the run's record of its hold equal to the hold now.
///
/// Written only where the value changed, so a dispatch that is offered again and
/// again for a held run rewrites nothing.
pub(super) fn record_hold<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run_id: &str,
    hold: Option<TargetHold>,
) {
    let Ok(mut run) = runs::load_run(app, run_id) else {
        return;
    };
    if run.target_hold == hold || run.state != GraduationRunState::Queued {
        return;
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        if hold.is_some() {
            "graduation withheld a direct run's dispatch: the pinned branch is not checked out"
        } else {
            "graduation released a direct run's hold: the pinned branch is checked out"
        },
        log_fields! { "run_id" => run_id.to_string() },
    );
    run.target_hold = hold;
    let _ = runs::save_run(app, &mut run);
    events::announce_queue(app, &run);
}

/// GRD-FR-VAUE: whether a direct run's commit may be made in this worktree now.
///
/// A stream run is bound by the stream's own working copy, which holds one
/// branch for good, so only a direct run is checked.
pub(super) fn commit_target_ok(run: &GraduationRun, worktree: &Path) -> bool {
    match &run.direct_target {
        Some(target) => branch_of(worktree).as_deref() == Some(target.branch.as_str()),
        None => true,
    }
}

/// GRD-FR-BSNI: the working copy a run writes in.
///
/// A direct run's is its pinned worktree, and a stream run's is the stream's.
/// `None` where the directory is gone.
pub fn target_worktree<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &GraduationRun,
) -> Option<PathBuf> {
    // GRD-FR-KZPT: a merge run works in the worktree it owns, and never in the
    // stream's working copy.
    if run.is_merge() {
        let path = store::store_base(app).ok()?.merge_worktree(&run.id);
        return path.is_dir().then_some(path);
    }
    match &run.direct_target {
        Some(target) => {
            let path = PathBuf::from(&target.worktree_path);
            path.is_dir().then_some(path)
        }
        None => crate::streams::worktree_of(app, &run.stream_id),
    }
}

/// WTC-FR-FBJQ: refuse a switch of worktree or branch while a dispatched direct
/// run stands.
///
/// Read from the run order index and from the claims held in memory. A direct
/// run is claimed before its index entry is saved as dispatched, so the claim
/// closes that window. A queued run that was never dispatched blocks nothing.
pub fn require_no_dispatched_direct_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
    let claimed = app
        .try_state::<GraduationState>()
        .and_then(|state| state.direct_claim());
    match claimed.or_else(|| queue::dispatched_direct_run(app)) {
        Some(run_id) => {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "a switch of worktree or branch was refused: a direct graduation run is working",
                log_fields! { "run_id" => run_id.clone() },
            );
            Err(format!("{}: {run_id}", crate::worktree::ERR_DIRECT_GRADUATION_ACTIVE))
        }
        None => Ok(()),
    }
}

/// WTC-FR-UMGY: offer every queue a dispatch.
///
/// Called after a switch of worktree or branch, and after a run ends, so a
/// direct run held for its branch starts when the branch is back and a run
/// refused by the project's limit starts when a place is free.
pub fn advance_all_queues<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    scheduler::dispatch_pending(app);
}

/// The typed code at the head of a refusal, which carries no path or content.
fn typed_code(reason: &str) -> String {
    reason
        .split(|c| c == ':' || c == ' ')
        .next()
        .unwrap_or_default()
        .to_string()
}
