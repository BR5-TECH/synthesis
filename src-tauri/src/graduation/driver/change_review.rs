//! The review stage of an OpenSpec change run (`GRL-graduation-loop.md`
//! GRL-FR-BHGJ, GRL-FR-EQDL, GRL-FR-HSNY, GRL-FR-XNBQ, GRL-FR-KDYL).
//!
//! Before each review turn of an apply run the application reads the task
//! progress of the change and validates it in the review checkout, and the
//! review reads both as evidence. A `ready` that the evidence or the change set
//! does not support never ends the loop: it is replaced with a `revise` of one
//! critical finding that names the cause, as the propose gate does
//! (GRL-FR-HSZD).

use std::path::Path;

use super::turns::note_blocked;
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::graduation::{ReviewFinding, ReviewOutcome, ReviewSeverity, ReviewVerdict};
use crate::openspec::{ApplyInstructions, ChangeValidation, OpenSpec};

/// The one file of the change folder an apply run may change (GRL-FR-SZQK).
const TASKS_FILE: &str = "tasks.md";

/// GRL-FR-BHGJ, GRL-FR-KDYL: the task progress of an apply run's change in
/// the review checkout, `Ok(None)` for every other run, or `Err` where the run
/// now rests `blocked` and no review turn may be dispatched.
pub async fn apply_progress<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    checkout: &Path,
) -> Result<Option<ApplyInstructions>, ()> {
    if !run.is_change_apply() {
        return Ok(None);
    }
    let Some(change) = run.openspec.as_ref().map(|data| data.change.clone()) else {
        return Ok(None);
    };
    let owned = app.clone();
    let root = checkout.to_path_buf();
    let name = change.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let seam = owned
            .try_state::<OpenSpec>()
            .ok_or(crate::openspec::OpenSpecReadError::OpenSpecModeInactive)?;
        let active = owned
            .try_state::<crate::project::ProjectState>()
            .and_then(|state| state.require_root().ok());
        let location = crate::openspec::ChangeLocation::ReviewCheckout { root };
        let at = crate::openspec::change::working_copy(&owned, &location)?;
        seam.apply_instructions(&owned, &crate::logging::BUFFER, active.as_ref(), &at, &name)
    })
    .await
    .unwrap_or(Err(crate::openspec::OpenSpecReadError::CliOutputUnreadable));
    match outcome {
        Ok(progress) => {
            let _ = logs::emit(
                app,
                run,
                StructuredEvent::new(
                    GraduationLogProducer::ReviewTurn,
                    "openspec apply progress read",
                )
                .with("state", progress.state.clone())
                .with("complete", progress.complete)
                .with("total", progress.total),
            );
            Ok(Some(progress))
        }
        Err(error) => {
            let _ = logs::emit(
                app,
                run,
                StructuredEvent::new(
                    GraduationLogProducer::ReviewTurn,
                    "openspec apply progress failed",
                )
                .at_level(GraduationLogLevel::Warn)
                .with("code", error.code()),
            );
            let code = super::openspec_review::BLOCKER_VALIDATION_UNAVAILABLE;
            note_blocked(app, run, code);
            let _ = crate::graduation::transitions::block(
                app,
                run,
                code,
                format!(
                    "The task progress of the OpenSpec change {change} could not be read: {}.",
                    error.code()
                ),
                "Continue the run to read the task progress and review the change again.",
                phases::PART_REVIEW,
            );
            Err(())
        }
    }
}

/// GRL-FR-EQDL, GRL-FR-XNBQ: the verdict of an OpenSpec change run's review
/// after its gate. A verdict of every other run, and one that does not end
/// the loop as `ready`, is returned as it came. `worktree` is the run's
/// working directory, which the change set is read from.
pub fn gate_change_verdict(
    run: &GraduationRun,
    worktree: &Path,
    verdict: ReviewVerdict,
    ends_as_ready: bool,
    validation: Option<&ChangeValidation>,
    progress: Option<&ApplyInstructions>,
) -> ReviewVerdict {
    if !ends_as_ready || !run.is_change_run() {
        return verdict;
    }
    let Some(data) = run.openspec.as_ref() else {
        return verdict;
    };
    let folder = format!("openspec/changes/{}/", data.change);
    let causes = if run.is_change_apply() {
        apply_causes(run, &folder, validation, progress)
    } else {
        rework_causes(run, worktree, &folder)
    };
    if causes.is_empty() {
        return verdict;
    }
    ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: format!(
            "The work on the OpenSpec change {} cannot be accepted as it stands.",
            data.change
        ),
        findings: vec![ReviewFinding {
            severity: ReviewSeverity::Critical,
            description: causes.join(" "),
            affected_files: vec![folder],
            correction: "Correct each cause this finding names, then report the work again."
                .to_string(),
        }],
    }
}

/// GRL-FR-EQDL: a task that is not done, a change that does not validate,
/// and a write to a file of the change folder other than `tasks.md`.
fn apply_causes(
    run: &GraduationRun,
    folder: &str,
    validation: Option<&ChangeValidation>,
    progress: Option<&ApplyInstructions>,
) -> Vec<String> {
    let mut causes = Vec::new();
    if let Some(progress) = progress {
        let open = progress.tasks.iter().filter(|task| !task.done).count();
        if open > 0 {
            causes.push(format!("{open} task(s) of the change are not done."));
        }
    }
    if let Some(validation) = validation.filter(|v| !v.valid) {
        causes.push(format!(
            "The change does not pass `openspec validate --strict`: {} issue(s) stand.",
            validation.issues.len()
        ));
    }
    let tasks = format!("{folder}{TASKS_FILE}");
    let written: Vec<&str> = run
        .checkpoint
        .changed_paths
        .iter()
        .map(String::as_str)
        .filter(|path| path.starts_with(folder) && *path != tasks)
        .collect();
    if !written.is_empty() {
        causes.push(format!(
            "The work changes files of the change folder other than tasks.md: {}.",
            written.join(", ")
        ));
    }
    causes
}

/// GRL-FR-XNBQ: a path changed outside the change folder and the check
/// folder, and a deleted file of the change folder. The check folder is never
/// part of the change set (GRL-FR-DCKG).
fn rework_causes(run: &GraduationRun, worktree: &Path, folder: &str) -> Vec<String> {
    let mut causes = Vec::new();
    let outside: Vec<&str> = run
        .checkpoint
        .changed_paths
        .iter()
        .map(String::as_str)
        .filter(|path| !path.starts_with(folder))
        .collect();
    if !outside.is_empty() {
        causes.push(format!(
            "The work changes paths outside the change folder: {}.",
            outside.join(", ")
        ));
    }
    let deleted = deleted_in_folder(run, worktree, folder);
    if !deleted.is_empty() {
        causes
            .push(format!("The work deletes files of the change folder: {}.", deleted.join(", ")));
    }
    causes
}

/// The files of the change folder that the base commit holds and the run's
/// tree does not.
fn deleted_in_folder(run: &GraduationRun, worktree: &Path, folder: &str) -> Vec<String> {
    let Some(base) = run.base_commit.as_deref() else {
        return Vec::new();
    };
    let Ok(Some(derived)) = crate::graduation::commit::run_tree(worktree, base) else {
        return Vec::new();
    };
    let Ok(repo) = git2::Repository::open(worktree) else {
        return Vec::new();
    };
    let (Ok(base_tree), Ok(tree)) = (
        git2::Oid::from_str(base)
            .and_then(|oid| repo.find_commit(oid))
            .and_then(|commit| commit.tree()),
        repo.find_tree(derived.tree),
    ) else {
        return Vec::new();
    };
    derived
        .paths
        .into_iter()
        .filter(|path| path.starts_with(folder))
        .filter(|path| {
            base_tree.get_path(Path::new(path)).is_ok() && tree.get_path(Path::new(path)).is_err()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openspec::{ApplyTask, ValidationIssue};

    const CHANGE: &str = "add-logout-3f2a";

    fn run_of(phase: &str) -> GraduationRun {
        let mut run = GraduationRun::new_for_test("g1", "", "2026-11-10T09:00:00Z");
        run.openspec = Some(crate::graduation::RunOpenSpec::of_phase(phase, CHANGE));
        run
    }

    fn ready() -> ReviewVerdict {
        ReviewVerdict { verdict: ReviewOutcome::Ready, rationale: "Fine.".into(), findings: vec![] }
    }

    fn progress(done: &[bool]) -> ApplyInstructions {
        ApplyInstructions {
            state: "ready".into(),
            total: done.len() as u64,
            complete: done.iter().filter(|d| **d).count() as u64,
            tasks: done
                .iter()
                .enumerate()
                .map(|(i, d)| ApplyTask { description: format!("task {i}"), done: *d })
                .collect(),
            missing_artifacts: vec![],
        }
    }

    fn valid(valid: bool) -> ChangeValidation {
        ChangeValidation {
            valid,
            issues: if valid {
                vec![]
            } else {
                vec![ValidationIssue {
                    level: "ERROR".into(),
                    path: "specs/auth/spec.md".into(),
                    message: "No scenario".into(),
                }]
            },
        }
    }

    // GRL-FR-EQDL: a ready apply review on every task done, a valid change, and
    // no write to the change folder but tasks.md keeps its verdict.
    #[test]
    fn a_ready_apply_review_on_complete_work_is_kept() {
        let mut run = run_of("apply");
        run.checkpoint.changed_paths =
            vec!["src/logout.ts".into(), format!("openspec/changes/{CHANGE}/tasks.md")];
        let gated = gate_change_verdict(
            &run,
            Path::new("/nowhere"),
            ready(),
            true,
            Some(&valid(true)),
            Some(&progress(&[true, true])),
        );
        assert_eq!(gated, ready());
    }

    // GRL-FR-EQDL: a task that is not done, a change that does not validate,
    // and a write to the proposal each make the ready a revise of one critical
    // finding that names the cause.
    #[test]
    fn an_apply_ready_on_unfinished_or_invalid_work_becomes_a_revise() {
        let run = run_of("apply");
        let gated = gate_change_verdict(
            &run,
            Path::new("/nowhere"),
            ready(),
            true,
            Some(&valid(true)),
            Some(&progress(&[true, false])),
        );
        assert_eq!(gated.verdict, ReviewOutcome::Revise);
        assert_eq!(gated.findings.len(), 1);
        assert_eq!(gated.findings[0].severity, ReviewSeverity::Critical);
        assert!(gated.findings[0].description.contains("1 task(s)"));

        let gated = gate_change_verdict(
            &run,
            Path::new("/nowhere"),
            ready(),
            true,
            Some(&valid(false)),
            Some(&progress(&[true])),
        );
        assert!(gated.findings[0].description.contains("openspec validate"));

        let mut run = run_of("apply");
        run.checkpoint.changed_paths = vec![format!("openspec/changes/{CHANGE}/proposal.md")];
        let gated = gate_change_verdict(
            &run,
            Path::new("/nowhere"),
            ready(),
            true,
            Some(&valid(true)),
            Some(&progress(&[true])),
        );
        assert_eq!(gated.verdict, ReviewOutcome::Revise);
        assert!(gated.findings[0].description.contains("proposal.md"));
    }

    // GRL-FR-EQDL, GRL-FR-UQNV: a revise that does not end the loop is kept,
    // and a draft run and a propose run are not gated here.
    #[test]
    fn a_real_revise_and_other_runs_keep_their_verdict() {
        let run = run_of("apply");
        let kept = gate_change_verdict(
            &run,
            Path::new("/nowhere"),
            ready(),
            false,
            Some(&valid(false)),
            Some(&progress(&[false])),
        );
        assert_eq!(kept, ready());
        for run in
            [run_of("propose"), GraduationRun::new_for_test("g2", "d2", "2026-11-10T09:00:00Z")]
        {
            let kept = gate_change_verdict(
                &run,
                Path::new("/nowhere"),
                ready(),
                true,
                Some(&valid(false)),
                Some(&progress(&[false])),
            );
            assert_eq!(kept, ready());
        }
    }

    // GRL-FR-XNBQ: a rework that changes a path outside the change folder is
    // replaced with a revise; one inside the folder keeps its verdict.
    #[test]
    fn a_rework_ready_on_a_path_outside_the_folder_becomes_a_revise() {
        let mut run = run_of("rework");
        run.checkpoint.changed_paths = vec![format!("openspec/changes/{CHANGE}/design.md")];
        let kept = gate_change_verdict(&run, Path::new("/nowhere"), ready(), true, None, None);
        assert_eq!(kept, ready());

        run.checkpoint.changed_paths.push("src/main.rs".into());
        let gated = gate_change_verdict(&run, Path::new("/nowhere"), ready(), true, None, None);
        assert_eq!(gated.verdict, ReviewOutcome::Revise);
        assert!(gated.findings[0].description.contains("src/main.rs"));
    }
}
