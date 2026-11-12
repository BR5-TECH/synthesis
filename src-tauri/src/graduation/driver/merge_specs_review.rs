//! The OpenSpec gate of a merge review (`GRL-graduation-loop.md` GRL-FR-ZDNL,
//! GRL-FR-ZFMZ, GRL-FR-AGOA; `GRD-graduation.md` GRD-FR-UJII).
//!
//! The archive of `OAR-openspec-archive.md` merges deltas into the live
//! specifications on the stream, so a conflict merge can join two edits of one
//! capability into a specification that OpenSpec refuses. Before each
//! `merge_review` turn in OpenSpec mode, the application validates the
//! specifications of the review checkout, the review reads the result, and a
//! `ready` over specifications that are not valid is never applied.

use std::path::Path;

use super::turns::note_blocked;
use super::*;
use crate::graduation::logs::{self, GraduationLogLevel, GraduationLogProducer, StructuredEvent};
use crate::graduation::{ReviewFinding, ReviewOutcome, ReviewSeverity, ReviewVerdict};
use crate::openspec::{OpenSpec, OpenSpecReadError, ProjectValidation, ValidationScope};

/// GRL-FR-ZDNL, GRL-FR-AGOA: the strict validation of the specifications of
/// the review checkout of a merge run in OpenSpec mode, `Ok(None)` for every
/// other run and while the mode is not active, or `Err` where the run now
/// rests `blocked` with `openspec_validation_unavailable` and no review turn
/// may be dispatched.
pub async fn specs_validation<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    run: &mut GraduationRun,
    checkout: &Path,
) -> Result<Option<ProjectValidation>, ()> {
    if !run.is_merge() {
        return Ok(None);
    }
    let active =
        app.try_state::<crate::project::ProjectState>().and_then(|state| state.require_root().ok());
    if !OpenSpec::mode_active(active.as_ref()) {
        return Ok(None);
    }
    let owned = app.clone();
    let root = checkout.to_path_buf();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let seam = owned.try_state::<OpenSpec>().ok_or(OpenSpecReadError::CliUnavailable(
            crate::openspec::OpenSpecCliState::NotFound,
        ))?;
        let location = crate::openspec::ChangeLocation::ReviewCheckout { root };
        let at = crate::openspec::change::working_copy(&owned, &location)?;
        seam.validate_project(
            &owned,
            &crate::logging::BUFFER,
            active.as_ref(),
            &at,
            ValidationScope::Specs,
        )
    })
    .await
    .unwrap_or(Err(OpenSpecReadError::CliOutputUnreadable));
    match outcome {
        Ok(validation) => {
            let _ = logs::emit(
                app,
                run,
                StructuredEvent::new(GraduationLogProducer::ReviewTurn, "openspec specs validated")
                    .with("valid", validation.valid)
                    .with("items", validation.items.len() as u64),
            );
            Ok(Some(validation))
        }
        // The mode went off between the two reads: the input carries none.
        Err(OpenSpecReadError::OpenSpecModeInactive) => Ok(None),
        Err(error) => {
            let _ = logs::emit(
                app,
                run,
                StructuredEvent::new(
                    GraduationLogProducer::ReviewTurn,
                    "openspec specs validation failed",
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
                    "The OpenSpec specifications of the merge could not be validated: {}.",
                    error.code()
                ),
                "Continue the run to validate the specifications and review the merge again.",
                phases::PART_REVIEW,
            );
            Err(())
        }
    }
}

/// GRL-FR-ZFMZ: a `ready` merge review while the specifications of its
/// checkout are not valid becomes a `revise` verdict of one `critical`
/// finding that names the first capability that is not valid. Every other
/// verdict, and every other run, keeps its verdict.
pub fn gate_merge_verdict(
    run: &GraduationRun,
    verdict: ReviewVerdict,
    validation: Option<&ProjectValidation>,
) -> ReviewVerdict {
    let Some(validation) = validation else {
        return verdict;
    };
    if !run.is_merge() || validation.valid || verdict.verdict != ReviewOutcome::Ready {
        return verdict;
    }
    let Some(item) =
        validation.first_invalid_spec().or_else(|| validation.items.iter().find(|i| !i.valid))
    else {
        return verdict;
    };
    let capability = item.id.clone();
    ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: format!(
            "The OpenSpec specification {capability} of the merged tree does not pass `openspec validate --specs --strict`, so the merge cannot be applied."
        ),
        findings: vec![ReviewFinding {
            // OPU-FR-EPXL: a finding the application writes has the kind `work`.
            kind: None,
            severity: ReviewSeverity::Critical,
            description: format!(
                "The OpenSpec specification {capability} does not validate after the merge: {} issue(s) stand.",
                item.issues.len()
            ),
            affected_files: vec![format!("openspec/specs/{capability}/spec.md")],
            correction: format!(
                "Reconcile openspec/specs/{capability}/spec.md so that `openspec validate --specs --strict` reports no issue for it."
            ),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openspec::{ProjectValidationItem, ValidationIssue};

    fn merge_run() -> GraduationRun {
        let mut run = GraduationRun::new_for_test("g1", "", "2026-11-10T09:00:00Z");
        run.merge = Some(crate::graduation::GraduationMergeData {
            name: "Merge feature".into(),
            stream_branch: "synthesis/stream/feature".into(),
            base_branch: "main".into(),
            base_tip: "a".into(),
            stream_tip: "b".into(),
            merge_base: "c".into(),
            snapshot_commit: "d".into(),
            publication: crate::streams::StreamMergePublication::Uncommitted,
            changed_paths: vec![],
            unresolved_paths: vec![],
            conflicts: vec![],
            result: None,
            pull: None,
        });
        run
    }

    fn ready() -> ReviewVerdict {
        ReviewVerdict { verdict: ReviewOutcome::Ready, rationale: "Fine.".into(), findings: vec![] }
    }

    fn item(id: &str, valid: bool) -> ProjectValidationItem {
        ProjectValidationItem {
            id: id.into(),
            kind: "spec".into(),
            valid,
            issues: if valid {
                vec![]
            } else {
                vec![ValidationIssue {
                    level: "ERROR".into(),
                    path: "requirements[0]".into(),
                    message: "Duplicate requirement".into(),
                }]
            },
        }
    }

    fn invalid() -> ProjectValidation {
        ProjectValidation {
            valid: false,
            items: vec![item("auth", true), item("ui/theme", false), item("zeta", false)],
        }
    }

    // GRL-FR-ZFMZ, OPU-FR-EPXL: a `ready` merge review over specifications
    // that are not valid becomes a `revise` of one critical `work` finding
    // that names the first capability that is not valid.
    #[test]
    fn a_ready_merge_review_over_invalid_specs_becomes_a_revise() {
        let gated = gate_merge_verdict(&merge_run(), ready(), Some(&invalid()));
        assert_eq!(gated.verdict, ReviewOutcome::Revise);
        assert_eq!(gated.findings.len(), 1);
        let finding = &gated.findings[0];
        assert_eq!(finding.severity, ReviewSeverity::Critical);
        assert_eq!(finding.kind, None);
        assert!(finding.description.contains("ui/theme"));
        assert!(!finding.description.contains("zeta"));
        assert_eq!(finding.affected_files, ["openspec/specs/ui/theme/spec.md"]);
    }

    // GRL-FR-ZFMZ: a valid validation, a missing one, a `revise`, and a run
    // that is not a merge run keep their verdict.
    #[test]
    fn the_merge_gate_keeps_every_other_verdict() {
        let valid = ProjectValidation { valid: true, items: vec![item("auth", true)] };
        assert_eq!(gate_merge_verdict(&merge_run(), ready(), Some(&valid)), ready());
        assert_eq!(gate_merge_verdict(&merge_run(), ready(), None), ready());
        let revise = ReviewVerdict {
            verdict: ReviewOutcome::Revise,
            rationale: "Lost a line.".into(),
            findings: vec![ReviewFinding {
                kind: None,
                severity: ReviewSeverity::Major,
                description: "The retry flag is gone.".into(),
                affected_files: vec!["src/queue.ts".into()],
                correction: "Keep it.".into(),
            }],
        };
        assert_eq!(gate_merge_verdict(&merge_run(), revise.clone(), Some(&invalid())), revise);
        let draft = GraduationRun::new_for_test("g2", "d2", "2026-11-10T09:00:00Z");
        assert_eq!(gate_merge_verdict(&draft, ready(), Some(&invalid())), ready());
    }
}
