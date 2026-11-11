//! The sessions, the scratch folder, and the rests of the plan turns of an
//! apply run (`OPU-openspec-plan-updates.md` OPU-FR-WPSR, OPU-FR-DTYE,
//! OPU-FR-SKTV, OPU-FR-WJKF, OPU-FR-WINO, OPU-FR-MCSG, OPU-FR-QUST), driven
//! with the agent scripted and the OpenSpec CLI faked.

use super::change_plan_continue::*;
use super::change_plan_updates::*;
use super::change_runs::*;
use super::*;
use crate::graduation::plan_updates::{self as plan};
use crate::graduation::FindingKind;

fn no_scratch_in(tree: &BTreeMap<String, String>) -> bool {
    !tree.keys().any(|path| path.starts_with(".synthesis-plan"))
}

// OPU-FR-WPSR, OPU-FR-SKTV, OPU-FR-DTYE: Continue from a `plan_update` rest
// of the origin `review` dispatches the work turn of the same pass with the
// purpose `escalation_answer` in the session of the plan correction turn,
// not of the work turn or of the review. It carries the `review` context with
// the reason, and the `work` findings alone as `loop_instruction`.
#[test]
fn continue_from_a_review_plan_update_resumes_the_plan_correction_turn() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let work = Turn::work().writing("src/logout.ts", "x\n").with_session("w1");
    let review = plan_review().with_session("r1");
    let correcting =
        plan_update(Vec::new()).writing(&plan_file("design.md"), "# Design\n").with_session("p1");
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![work, review, correcting]));
    let rest = rested.rest.clone().expect("the rest");
    assert_eq!((rest.reason.as_str(), rest.origin.as_deref()), ("plan_update", Some("review")));
    let proposal_id = rest.proposal_id.clone().unwrap();
    accept(&fx, &proposal_id, &hunk_id(&fx, &proposal_id, "design.md"), None).expect("accepted");

    let continued = continue_of(&fx, &rested).expect("continued");
    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    let done = fx.drive(&continued, dispatch.clone());
    let next = &dispatch.of_part("work")[0];
    assert_eq!(next.purpose, "escalation_answer");
    assert_eq!(next.pass, 1);
    assert_eq!(next.resume_session.as_deref(), Some("p1"), "the plan correction turn's session");
    assert_eq!(next.input["plan_update"]["origin"], "review");
    assert_eq!(next.input["plan_update"]["reason"], REASON);
    let instruction = next.instruction_field("loop_instruction").unwrap_or_default();
    assert!(instruction.contains("The button is wrong."), "{instruction}");
    assert!(!instruction.contains("The design is wrong."), "{instruction}");
    assert_eq!(done.state, GraduationRunState::Completed);
}

// OPU-FR-DTYE, OPU-FR-FPRX, OPU-FR-FDCW: a pause while the plan correction
// turn runs keeps the saved correction. Continue dispatches the same plan
// correction turn again, in the same pass, with no new review and no second
// entry of the findings in the review history.
#[test]
fn a_paused_plan_correction_turn_runs_again_on_continue() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let paused = {
        let (app, id) = (fx.app.clone(), run.id.clone());
        Turn::work().doing(move || {
            crate::graduation::pause_graduation_run(app.clone(), id.clone()).expect("paused");
        })
    };
    let first = ScriptedDispatch::new(vec![Turn::work().with_session("w1"), plan_review(), paused]);
    let rested = fx.drive(&run, first);
    assert_eq!(rested.state, GraduationRunState::Interrupted);
    let saved = rested.checkpoint.plan.correction.clone().expect("the correction is kept");
    assert_eq!(saved.step, plan::PlanCorrectionStep::Plan);

    let continued = fx.continue_run(&rested);
    let correcting = plan_update(Vec::new()).writing(&plan_file("design.md"), "# Design\n");
    let next = ScriptedDispatch::new(vec![correcting]);
    let done = fx.drive(&continued, next.clone());
    assert_eq!(next.seen().len(), 1);
    assert!(next.of_part("review").is_empty(), "no new review");
    let again = &next.of_part("work")[0];
    assert_eq!(again.pass, 1);
    assert_eq!(
        again.instruction_field("correction"),
        crate::graduation::driver::prompt_section(
            &crate::graduation::driver::REFUSALS,
            plan::REFUSAL_PLAN_CORRECTION
        )
    );
    let instruction = again.instruction_field("loop_instruction").unwrap_or_default();
    assert!(instruction.contains("The design is wrong."), "{instruction}");
    assert_eq!(done.pass(), 1);
    assert_eq!(done.checkpoint.review_history.len(), 2);
    let rest = done.rest.expect("the rest");
    assert_eq!((rest.reason.as_str(), rest.origin.as_deref()), ("plan_update", Some("review")));
}

// OPU-FR-WJKF, OPU-FR-VKBO: a `plan_update_invalid` rest keeps the scratch
// folder of the refused plan update, and the ordinary work turn that its
// Continue dispatches starts with the folder removed.
#[test]
fn a_stale_scratch_folder_is_removed_before_an_ordinary_work_turn() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let refused = || plan_update(questions()).writing(".synthesis-plan/src/main.ts", "x\n");
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![refused(), refused()]));
    assert_eq!(rested.rest.as_ref().map(|r| r.reason.as_str()), Some("plan_update_invalid"));
    assert!(stream.worktree().join(".synthesis-plan/src/main.ts").is_file());

    let continued = continue_of(&fx, &rested).expect("continued");
    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    fx.drive(&continued, dispatch.clone());
    let work = &dispatch.of_part("work")[0];
    assert_eq!(work.purpose, "resume");
    assert!(no_scratch_in(&work.tree), "{:?}", work.tree.keys().collect::<Vec<_>>());
}

// OPU-FR-WINO: a scratch folder that a work turn left is in no review
// checkout and in no `hidden_paths`, with the line of `info/exclude` in place
// and also where it is absent. An ignored path of the project is still named.
#[test]
fn the_review_does_not_see_the_scratch_folder() {
    for unhidden in [false, true] {
        let (fx, _, _) = change_fixture(ALL_DONE);
        let stream = change_stream(&fx, "editor");
        let run = apply_run(&fx, &stream);
        let unhide = {
            let root = stream.worktree();
            move || {
                if !unhidden {
                    return;
                }
                let repo = git2::Repository::open(&root).unwrap();
                let exclude = repo.commondir().join("info/exclude");
                let text = std::fs::read_to_string(&exclude).unwrap_or_default();
                std::fs::write(&exclude, text.replace(plan::scratch::EXCLUDE_LINE, "")).unwrap();
            }
        };
        let work = Turn::work()
            .doing(unhide)
            .writing(&change_path("tasks.md"), "- [x] Add the button\n- [x] Wire the call\n")
            .writing("target/build.log", "built\n")
            .writing(&plan_file("design.md"), "# Design\n");
        let dispatch = ScriptedDispatch::new(vec![work, Turn::ready()]);
        let done = fx.drive(&run, dispatch.clone());
        assert_eq!(done.state, GraduationRunState::Completed, "{:?}", done.blocker);
        let review = &dispatch.of_part("review")[0];
        assert!(no_scratch_in(&review.tree), "unhidden {unhidden}: {:?}", review.tree.keys());
        let hidden: Vec<String> = review.input["hidden_paths"]
            .as_array()
            .map(|paths| paths.iter().filter_map(|p| p.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        assert!(hidden.iter().any(|p| p.starts_with("target")), "{hidden:?}");
        assert!(!hidden.iter().any(|p| p.starts_with(".synthesis-plan")), "{hidden:?}");
    }
}

// OPU-FR-MCSG: the comment of a `plan_findings` rest holds the `plan`
// findings in the order of the verdict, numbered, each with its correction,
// and no `work` finding.
#[test]
fn the_plan_findings_comment_keeps_the_order_of_the_verdict() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let review = Turn::answering(Answer::Verdict(ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: "The plan cannot be built.".into(),
        findings: vec![
            finding(FindingKind::Plan, ReviewSeverity::Major, "The design"),
            finding(FindingKind::Work, ReviewSeverity::Minor, "The button"),
            finding(FindingKind::Plan, ReviewSeverity::Minor, "The schema"),
        ],
    }));
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![Turn::work(), review, Turn::work()]));
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.findings.map(|f| f.len()), Some(3));
    let opened = thread(&fx, &stream.id, rest.thread_id.as_deref().unwrap());
    assert_eq!(opened.comments[0].author, crate::comments::Participant::synthesis());
    let body = &opened.comments[0].body;
    let design = body.find("1. The design is wrong.\n   Correction: Correct The design.");
    let schema = body.find("2. The schema is wrong.\n   Correction: Correct The schema.");
    assert!(design.is_some() && schema.is_some() && design < schema, "{body}");
    assert!(!body.contains("The button"), "{body}");
}

// OPU-FR-QUST: `answer_graduation_escalation` is refused with
// `run_state_not_permitted` at a `plan_findings` rest and at a
// `plan_update_invalid` rest, and records nothing.
#[test]
fn an_escalation_answer_is_refused_at_every_plan_rest() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let findings = rested_on_findings(&fx, &stream);
    let (fx2, _, _) = change_fixture(ALL_DONE);
    let stream2 = change_stream(&fx2, "editor");
    let run = apply_run(&fx2, &stream2);
    let empty = || plan_update(Vec::new());
    let invalid = fx2.drive(&run, ScriptedDispatch::new(vec![empty(), empty()]));
    assert_eq!(invalid.rest.as_ref().map(|r| r.reason.as_str()), Some("plan_update_invalid"));
    for (fx, rested) in [(&fx, &findings), (&fx2, &invalid)] {
        let refused = crate::graduation::answer_graduation_escalation(
            fx.app.clone(),
            rested.id.clone(),
            vec![GraduationEscalationAnswer {
                position: 1,
                answer: "x".into(),
                summary: "x".into(),
                note: None,
            }],
        );
        assert!(refused.unwrap_err().starts_with("run_state_not_permitted"));
        let reloaded = fx.reload(&rested.id);
        assert_eq!(reloaded.rest, rested.rest);
        assert!(reloaded.checkpoint.decisions.is_empty());
    }
}

// OPU-FR-FDCW: only a `revise` verdict routes plan findings to a plan
// correction turn. A `ready` verdict that holds a `plan` finding is no verdict
// the loop can read, so the review is asked again and no plan correction turn
// is dispatched.
#[test]
fn a_ready_verdict_with_a_plan_finding_gets_no_plan_correction_turn() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let work = Turn::work()
        .writing(&change_path("tasks.md"), "- [x] Add the button\n- [x] Wire the call\n");
    let ready = Turn::answering(Answer::Verdict(ReviewVerdict {
        verdict: ReviewOutcome::Ready,
        rationale: "The work is done.".into(),
        findings: vec![finding(FindingKind::Plan, ReviewSeverity::Minor, "The task text")],
    }));
    let dispatch = ScriptedDispatch::new(vec![work, ready, Turn::ready()]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed, "{:?}", done.rest);
    assert_eq!(dispatch.of_part("work").len(), 1, "no plan correction turn");
    assert_eq!(dispatch.of_part("review").len(), 2, "the review is asked again");
    assert!(done.checkpoint.plan.correction.is_none());
}
