//! The review stage of a propose run (`GRL-graduation-loop.md` GRL-FR-CDNI,
//! GRL-FR-HSZD, GRL-FR-KDYL).
//!
//! Before each review turn the application validates the change in the
//! review checkout, which is the tree a `ready` verdict commits
//! (GRL-FR-XNQU). The result is evidence the review reads, and a `ready` on a
//! change that does not validate never ends the loop.

use std::path::Path;

use super::turns::note_blocked;
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::graduation::{ReviewFinding, ReviewOutcome, ReviewSeverity, ReviewVerdict};
use crate::openspec::{ChangeValidation, OpenSpec};

/// GRL-FR-KDYL: the blocker code of a validation that cannot run.
pub const BLOCKER_VALIDATION_UNAVAILABLE: &str = "openspec_validation_unavailable";

/// GRL-FR-CDNI, GRL-FR-KDYL: the validation of a propose run's change in the
/// review checkout, `Ok(None)` for every other run, or `Err` where the run
/// now rests `blocked` and no review turn may be dispatched.
pub async fn validation<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    checkout: &Path,
) -> Result<Option<ChangeValidation>, ()> {
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
        seam.validate_change(&owned, &crate::logging::BUFFER, active.as_ref(), &at, &name)
    })
    .await
    .unwrap_or(Err(crate::openspec::OpenSpecReadError::CliOutputUnreadable));
    match outcome {
        Ok(validation) => {
            let _ = logs::emit(
                app,
                run,
                StructuredEvent::new(
                    GraduationLogProducer::ReviewTurn,
                    "openspec change validated",
                )
                .with("valid", validation.valid)
                .with("issues", validation.issues.len() as u64),
            );
            Ok(Some(validation))
        }
        Err(error) => {
            let _ = logs::emit(
                app,
                run,
                StructuredEvent::new(
                    GraduationLogProducer::ReviewTurn,
                    "openspec validation failed",
                )
                .at_level(GraduationLogLevel::Warn)
                .with("code", error.code()),
            );
            note_blocked(app, run, BLOCKER_VALIDATION_UNAVAILABLE);
            let _ = crate::graduation::transitions::block(
                app,
                run,
                BLOCKER_VALIDATION_UNAVAILABLE,
                format!("The OpenSpec change {change} could not be validated: {}.", error.code()),
                "Continue the run to validate the change and review it again.",
                phases::PART_REVIEW,
            );
            Err(())
        }
    }
}

/// GRL-FR-HSZD: a `ready` verdict, or one that ends the loop as `ready` under
/// GRL-FR-UQNV, on a change that does not validate becomes a `revise` verdict
/// with one critical finding that names the change. `ends_as_ready` says
/// whether the verdict would end the loop.
pub fn gate_verdict(
    run: &GraduationRun,
    verdict: ReviewVerdict,
    ends_as_ready: bool,
    validation: Option<&ChangeValidation>,
) -> ReviewVerdict {
    let (Some(data), Some(validation)) = (run.openspec.as_ref(), validation) else {
        return verdict;
    };
    if validation.valid || !ends_as_ready {
        return verdict;
    }
    let folder = format!("openspec/changes/{}/", data.change);
    ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: format!(
            "The OpenSpec change {} does not pass `openspec validate --strict`, so it cannot be accepted.",
            data.change
        ),
        findings: vec![ReviewFinding {
            severity: ReviewSeverity::Critical,
            description: format!(
                "The OpenSpec change {} does not validate: {} issue(s) stand.",
                data.change,
                validation.issues.len()
            ),
            affected_files: vec![folder],
            correction: format!(
                "Run `openspec validate {} --strict` and fix every issue it reports.",
                data.change
            ),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openspec::ValidationIssue;

    fn propose_run() -> GraduationRun {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-11-10T09:00:00Z");
        run.openspec = Some(crate::graduation::openspec_start::propose_data("add-logout-3f2a"));
        run
    }

    fn ready() -> ReviewVerdict {
        ReviewVerdict { verdict: ReviewOutcome::Ready, rationale: "Fine.".into(), findings: vec![] }
    }

    fn invalid() -> ChangeValidation {
        ChangeValidation {
            valid: false,
            issues: vec![ValidationIssue {
                level: "ERROR".into(),
                path: "specs/auth/spec.md".into(),
                message: "No scenario".into(),
            }],
        }
    }

    // GRL-FR-HSZD: a `ready` on a change that does not validate becomes a
    // `revise` with one critical finding that names the change.
    #[test]
    fn a_ready_on_an_invalid_change_becomes_a_revise() {
        let gated = gate_verdict(&propose_run(), ready(), true, Some(&invalid()));
        assert_eq!(gated.verdict, ReviewOutcome::Revise);
        assert_eq!(gated.findings.len(), 1);
        assert_eq!(gated.findings[0].severity, ReviewSeverity::Critical);
        assert!(gated.findings[0].description.contains("add-logout-3f2a"));
        assert_eq!(gated.findings[0].affected_files, ["openspec/changes/add-logout-3f2a/"]);
    }

    // GRL-FR-HSZD: an advisory end, a `revise` of minor findings, is gated the
    // same way; a `revise` that does not end the loop stays as it is.
    #[test]
    fn an_advisory_end_is_gated_and_a_real_revise_is_kept() {
        let advisory = ReviewVerdict {
            verdict: ReviewOutcome::Revise,
            rationale: "Small things.".into(),
            findings: vec![ReviewFinding {
                severity: ReviewSeverity::Minor,
                description: "Typo.".into(),
                affected_files: vec![],
                correction: "Fix it.".into(),
            }],
        };
        let gated = gate_verdict(&propose_run(), advisory.clone(), true, Some(&invalid()));
        assert_eq!(gated.findings[0].severity, ReviewSeverity::Critical);
        let kept = gate_verdict(&propose_run(), advisory.clone(), false, Some(&invalid()));
        assert_eq!(kept, advisory);
    }

    // GRL-FR-HSZD: a valid change, and a run that is not a propose run, keep
    // their verdict.
    #[test]
    fn a_valid_change_and_a_draft_run_keep_their_verdict() {
        let valid = ChangeValidation { valid: true, issues: vec![] };
        assert_eq!(gate_verdict(&propose_run(), ready(), true, Some(&valid)), ready());
        let draft = GraduationRun::new_for_test("g2", "d2", "2026-11-10T09:00:00Z");
        assert_eq!(gate_verdict(&draft, ready(), true, Some(&invalid())), ready());
        assert_eq!(gate_verdict(&propose_run(), ready(), true, None), ready());
    }
}
