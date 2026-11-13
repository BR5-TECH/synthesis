//! The plan updates of an apply run, in the loop
//! (`OPU-openspec-plan-updates.md` OPU-FR-RMZN through OPU-FR-QXUE,
//! OPU-FR-MCSG, OPU-FR-BKZX; `../ai/GRL-graduation-loop.md` GRL-FR-RXJF),
//! driven with the agent scripted and the OpenSpec CLI faked. Continue and
//! the stores are in `change_plan_continue.rs`.
//!
//! OPU-FR-IBWG: every scenario runs with no container runtime, no OpenSpec
//! CLI, and no network.

use super::change_runs::*;
use super::*;
use crate::graduation::plan_updates::{self as plan, PlanRefusalCause};
use crate::graduation::FindingKind;
use crate::tools::agent_exec::protocol::{
    AgentEscalation, AgentEscalationQuestion, AgentProposedResponse, EscalationKind,
};

pub(super) const REASON: &str = "The design names a sessions table that the schema does not hold.";

/// An apply run that waits in its queue, started on the production path.
pub(super) fn apply_run(fx: &Fixture, stream: &crate::streams::WorkStream) -> GraduationRun {
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let run = start_apply(fx, stream).expect("an apply start");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    run
}

pub(super) fn change_path(file: &str) -> String {
    format!("openspec/changes/{CHANGE}/{file}")
}

/// OPU-FR-BNFL: where the agent proposes the whole new text of `file`.
pub(super) fn plan_file(file: &str) -> String {
    format!(".synthesis-plan/openspec/changes/{CHANGE}/{file}")
}

/// The first question, with one option, and the second, with none.
pub(super) fn questions() -> Vec<AgentEscalationQuestion> {
    vec![
        AgentEscalationQuestion {
            question: "Keep the old cookie name?".into(),
            options: vec![AgentProposedResponse {
                answer: "Keep the name session_id.".into(),
                summary: "Keep the name".into(),
                description: "Old clients keep working.".into(),
            }],
        },
        AgentEscalationQuestion {
            question: "Which table holds the sessions?".into(),
            options: vec![],
        },
    ]
}

/// The vendor session of the turn that gives a plan update.
pub(super) const UPDATE_SESSION: &str = "u1";

/// OPU-FR-YBLE: a plan update with these questions, in the session
/// [`UPDATE_SESSION`].
pub(super) fn plan_update(questions: Vec<AgentEscalationQuestion>) -> Turn {
    Turn::answering(Answer::PlanUpdate(AgentEscalation {
        kind: Some(EscalationKind::PlanUpdate),
        reason: REASON.into(),
        questions,
    }))
    .with_session(UPDATE_SESSION)
}

/// The session id that the run will resume after the rest it holds.
pub(super) fn rest_session(run: &GraduationRun) -> Option<String> {
    run.checkpoint.plan.rest_session.as_ref().and_then(|s| s.session_id.clone())
}

/// The work turn that implements a part, marks a task, and gives a plan
/// update that edits the proposal and adds a design.
pub(super) fn updating() -> Turn {
    plan_update(questions())
        .writing("src/logout.ts", "export const logout = () => {};\n")
        .writing(&change_path("tasks.md"), "- [x] Add the button\n- [ ] Wire the call\n")
        .writing(&plan_file("proposal.md"), "# Why\n\nLogout is missing, and so is the table.\n")
        .writing(&plan_file("design.md"), "# Design\n\nAdd a sessions table.\n")
}

pub(super) fn finding(kind: FindingKind, severity: ReviewSeverity, text: &str) -> ReviewFinding {
    ReviewFinding {
        kind: Some(kind),
        severity,
        description: format!("{text} is wrong."),
        affected_files: vec![],
        correction: format!("Correct {text}."),
    }
}

/// A review that finds the plan wrong, beside one finding about the work.
pub(super) fn plan_review() -> Turn {
    Turn::answering(Answer::Verdict(ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: "The plan cannot be built.".into(),
        findings: vec![
            finding(FindingKind::Plan, ReviewSeverity::Major, "The design"),
            finding(FindingKind::Work, ReviewSeverity::Minor, "The button"),
        ],
    }))
}

pub(super) fn store(fx: &Fixture) -> crate::fs::RootFs {
    fx.app.state::<crate::project::ProjectState>().require_store().unwrap()
}

pub(super) fn project_root(fx: &Fixture) -> crate::fs::RootFs {
    fx.app.state::<crate::project::ProjectState>().require_root().unwrap()
}

pub(super) fn proposals(
    fx: &Fixture,
    stream_id: &str,
) -> Vec<crate::change_proposals::ChangeEditProposal> {
    crate::change_proposals::list_proposals_impl(&store(fx), stream_id, CHANGE)
}

pub(super) fn hunks(
    fx: &Fixture,
    proposal_id: &str,
) -> Vec<crate::change_proposals::ChangeEditHunk> {
    crate::change_proposals::load_hunks_impl(
        &store(fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        proposal_id,
    )
    .expect("hunks")
    .hunks
}

pub(super) fn backend_records() -> Vec<crate::logging::LogRecord> {
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..crate::logging::LogFilter::default()
    };
    crate::logging::BUFFER.query(&filter, None, 5000).unwrap().records
}

/// OPU-FR-VGFJ: no record of this run holds the reason, a question, an
/// answer, a note, or the text of a planning file.
pub(super) fn assert_no_text_logged(run_id: &str) {
    let texts = [
        REASON,
        "Keep the old cookie name?",
        "Which table holds",
        "Add a sessions table",
        "Keep the name session_id.",
        "Old clients matter.",
        "A new sessions table.",
        "Keep the old reason.",
    ];
    for record in backend_records()
        .iter()
        .filter(|r| r.fields.get("run_id") == Some(&serde_json::json!(run_id)))
    {
        let line = serde_json::to_string(record).unwrap();
        for text in texts {
            assert!(!line.contains(text), "a record holds text: {line}");
        }
    }
}

// OPU-FR-RMZN, OPU-FR-UNLM, OPU-FR-UOYJ, OPU-FR-QUST, OPU-FR-YBLE,
// OPU-FR-MBIY, OPU-FR-MCEZ, OPU-FR-QXUE, OPU-FR-VXVO, OPU-FR-OQRA,
// OPU-FR-FOPN, OPU-FR-VGFJ, GRL-FR-RXJF, CMS-FR-CSCD, CEP-FR-QDJH,
// ESU-FR-VTAZ, EAC-FR-18: a plan
// update of the work turn ends the turn without a review. It opens a plan
// thread as Synthesis whose first comment is the reason, reserves the
// question set with the summary of each option as its value (a question with
// no option included), and records the plan proposal there. The run rests on
// `plan_update` with no escalation, the scratch folder is gone, and the run's
// own work stays uncommitted.
#[test]
fn a_plan_update_of_the_work_turn_is_recorded_in_a_plan_thread() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let head = head_of(&stream.worktree());
    let run = apply_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![updating()]);
    let rested = fx.drive(&run, dispatch.clone());

    assert_eq!(rested.state, GraduationRunState::AwaitingAuthor);
    assert!(rested.escalation.is_none(), "a plan update holds no escalation");
    assert!(dispatch.of_part("review").is_empty(), "no review");
    let rest = rested.rest.clone().expect("the rest");
    assert_eq!(rest.reason, plan::REST_PLAN_UPDATE);
    assert_eq!(rest.origin.as_deref(), Some("work"));
    assert_eq!(rest.plan_reason.as_deref(), Some(REASON));
    assert_eq!(rest.findings, None);
    let given = rest.questions.clone().expect("the questions");
    assert_eq!(given.len(), 2);
    assert_eq!(given[0].options[0].answer, "Keep the name session_id.");

    let thread_id = rest.thread_id.clone().expect("the plan thread");
    let discussion = thread(&fx, &stream.id, &thread_id);
    assert_eq!(discussion.comments[0].author, crate::comments::Participant::synthesis());
    assert_eq!(discussion.comments[0].body, REASON);

    let set = crate::comments::read_question_set(&store(&fx), &project_root(&fx), &thread_id)
        .expect("the question set");
    assert_eq!(Some(set.set_id.clone()), rest.set_id);
    assert_eq!(set.asked_by, crate::comments::Participant::synthesis());
    assert_eq!(set.questions[0].text, "Keep the old cookie name?");
    assert_eq!(set.questions[0].options[0].value, "Keep the name");
    assert_eq!(
        set.questions[0].options[0].description.as_deref(),
        Some("Old clients keep working.")
    );
    assert!(set.questions[1].options.is_empty(), "a question with no option");

    let recorded = proposals(&fx, &stream.id);
    assert_eq!(recorded.len(), 1);
    assert_eq!(Some(recorded[0].id.clone()), rest.proposal_id);
    assert_eq!(recorded[0].thread_id, thread_id);
    assert_eq!(recorded[0].rationale, REASON);
    assert_eq!(recorded[0].agent, crate::comments::Participant::synthesis());
    let edits = hunks(&fx, &recorded[0].id);
    let paths: Vec<&str> = edits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["design.md", "proposal.md"]);
    assert!(edits[0].new_file);
    assert!(!paths.contains(&"tasks.md"), "the marks of the run are no edit");

    let worktree = stream.worktree();
    assert!(!worktree.join(".synthesis-plan").exists(), "the scratch folder is gone");
    assert!(worktree.join("src/logout.ts").is_file(), "the work stays");
    assert!(rested.commits.is_empty());
    assert_eq!(head_of(&worktree), head, "nothing is committed");
    assert_no_text_logged(&run.id);
}

// OPU-FR-QXUE: a plan update with no question reserves no set, and one with
// no edit records no proposal. Each still rests the run.
#[test]
fn a_plan_update_takes_only_the_steps_it_needs() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let edits_alone = plan_update(Vec::new()).writing(&plan_file("design.md"), "# Design\n");
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![edits_alone]));
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.set_id, None);
    assert!(rest.proposal_id.is_some());
    let thread_id = rest.thread_id.unwrap();
    assert!(
        crate::comments::read_question_set(&store(&fx), &project_root(&fx), &thread_id).is_none()
    );

    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![plan_update(questions())]));
    let rest = rested.rest.expect("the rest");
    assert!(rest.set_id.is_some());
    assert_eq!(rest.proposal_id, None);
    assert!(proposals(&fx, &stream.id).is_empty());
}

// OPU-FR-RFFH, ESU-FR-ATVP: a plan update from a turn that may not give one
// is refused as `escalation_shape`, and the answer correction follows: the
// work turn of a draft run, and the review turn of an apply run.
#[test]
fn a_plan_update_of_another_turn_is_refused_and_corrected() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state.");
    let dispatch =
        ScriptedDispatch::new(vec![plan_update(questions()), Turn::work(), Turn::ready()]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed);
    let corrected = &dispatch.of_part("work")[1];
    let expected = crate::graduation::driver::prompt_section(
        &crate::graduation::driver::REFUSALS,
        "answer_escalation_shape",
    );
    assert_eq!(corrected.instruction_field("correction"), expected);
    assert!(done.rest.is_none());

    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let implementing = Turn::work()
        .writing(&change_path("tasks.md"), "- [x] Add the button\n- [x] Wire the call\n");
    let dispatch =
        ScriptedDispatch::new(vec![implementing, plan_update(questions()), Turn::ready()]);
    let done = fx.drive(&run, dispatch.clone());
    assert_eq!(done.state, GraduationRunState::Completed, "{:?}", done.rest);
    assert_eq!(dispatch.of_part("review")[1].instruction_field("correction"), expected);
}

// OPU-FR-XLVF, OPU-FR-XSRD, OPU-FR-WJKF, OPU-FR-VKBO, GRL-FR-LCSB: a refused
// plan update gets one plan update correction turn of part `work`. It carries
// the `plan_update_invalid` text, resumes the refused turn's session, keeps
// the scratch folder, carries no milestone block, adds a work turn, and keeps
// the pass. A valid answer is recorded with the origin `work`, and the rest
// keeps the correction turn's session for Continue.
#[test]
fn a_refused_plan_update_gets_one_correction_turn() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let outside =
        plan_update(questions()).writing(".synthesis-plan/src/main.ts", "x\n").with_session("w1");
    let corrected = plan_update(questions())
        .with_session("c1")
        .doing({
            let root = stream.worktree();
            move || {
                let _ = std::fs::remove_dir_all(root.join(".synthesis-plan/src"));
            }
        })
        .writing(&plan_file("design.md"), "# Design\n");
    let dispatch = ScriptedDispatch::new(vec![outside, corrected]);
    let rested = fx.drive(&run, dispatch.clone());

    let works = dispatch.of_part("work");
    assert_eq!(works.len(), 2);
    let correction = &works[1];
    assert_eq!(
        correction.instruction_field("correction"),
        crate::graduation::driver::prompt_section(
            &crate::graduation::driver::REFUSALS,
            plan::REFUSAL_PLAN_UPDATE_INVALID
        )
    );
    assert_eq!(correction.resume_session.as_deref(), Some("w1"), "the refused turn's session");
    assert!(correction.tree.contains_key(".synthesis-plan/src/main.ts"), "the folder is kept");
    assert!(!correction.instruction.contains("## Milestones"));
    assert_eq!(correction.pass, 1);
    assert_eq!(rested.work_turns, 2);
    assert_eq!(rested.pass(), 1);
    // OPU-FR-WPSR: Continue resumes the turn that gave the plan update.
    assert_eq!(rest_session(&rested).as_deref(), Some("c1"));
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.reason, plan::REST_PLAN_UPDATE);
    assert_eq!(rest.origin.as_deref(), Some("work"));
}

// OPU-FR-CLYW, OPU-FR-XLVF: a plan update correction turn that does not give
// a valid plan update rests the run on `plan_update_invalid` with the cause
// of the last refusal. Nothing is recorded in a discussion, and nothing is
// committed. Each cause of the rules is the cause of its rest.
#[test]
fn a_correction_that_gives_no_valid_plan_update_rests_on_plan_update_invalid() {
    let cases = [
        (
            plan_file("design.md").replace(CHANGE, "other"),
            "x\n",
            PlanRefusalCause::PlanPathOutsideChange,
        ),
        (plan_file("proposal.md"), "", PlanRefusalCause::PlanDeletesFile),
    ];
    for (path, text, cause) in cases {
        let (fx, _, _) = change_fixture(READY);
        let stream = change_stream(&fx, "editor");
        let head = head_of(&stream.worktree());
        let run = apply_run(&fx, &stream);
        let refused = || plan_update(questions()).writing(&path, text);
        let dispatch = ScriptedDispatch::new(vec![refused(), refused()]);
        let rested = fx.drive(&run, dispatch.clone());
        assert_eq!(rested.state, GraduationRunState::AwaitingAuthor);
        let rest = rested.rest.expect("the rest");
        assert_eq!(rest.reason, plan::REST_PLAN_UPDATE_INVALID);
        assert_eq!(rest.cause.map(|c| c.as_str()), Some(cause.as_str()));
        assert_eq!(rest.thread_id, None);
        let target = crate::comments::DiscussionTarget::Change {
            stream_id: stream.id.clone(),
            change: CHANGE.to_string(),
        };
        assert!(crate::comments::list_discussions_in(&store(&fx), &target).is_empty());
        assert!(proposals(&fx, &stream.id).is_empty());
        assert_eq!(head_of(&stream.worktree()), head);
    }

    // PlanFileNotText, and an empty plan update, the last after a success.
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let binary = plan_update(Vec::new()).doing({
        let at = stream.worktree().join(plan_file("design.md"));
        move || {
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(&at, [0xff, 0xfe]).unwrap();
        }
    });
    let dispatch = ScriptedDispatch::new(vec![binary, Turn::work()]);
    let rested = fx.drive(&run, dispatch);
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.cause.map(|c| c.as_str()), Some("plan_file_not_text"), "the saved cause");

    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![plan_update(Vec::new()), plan_update(Vec::new())]);
    let rested = fx.drive(&run, dispatch);
    assert_eq!(rested.rest.unwrap().cause.map(|c| c.as_str()), Some("plan_update_empty"));
}

// OPU-FR-XSRD: a plan update correction turn may answer an ordinary
// escalation, which rests the run on the escalation and spends the
// correction.
#[test]
fn a_correction_may_answer_an_ordinary_escalation() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let escalate = Turn::answering(Answer::Escalate(vec!["May the table be new?".into()]));
    let dispatch = ScriptedDispatch::new(vec![plan_update(Vec::new()), escalate]);
    let rested = fx.drive(&run, dispatch);
    assert_eq!(rested.state, GraduationRunState::AwaitingAuthor);
    assert!(rested.escalation.is_some());
    assert!(rested.rest.is_none());
    assert!(rested.checkpoint.plan.correction.is_none());
}

// OPU-FR-EPXL, OPU-FR-FDCW, OPU-FR-DTYE, OPU-FR-FPRX, OPU-FR-QKLX,
// GRL-FR-RXJF, GRL-FR-LCSB, GXD-FR-WRVH: a verdict that holds a `plan`
// finding starts no pass. Its findings join the review history, and one plan
// correction turn of part `work` follows: it carries the `plan_correction`
// text, the `plan` findings alone as `loop_instruction`, no milestone block,
// and the session of the last work turn, not the session of the review. Its
// plan update is recorded with the origin `review` and every finding of the
// verdict, and the rest keeps the plan correction turn's session.
#[test]
fn a_plan_finding_gets_a_plan_correction_turn() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let work = Turn::work().writing("src/logout.ts", "x\n").with_session("w1");
    let correcting =
        plan_update(Vec::new()).writing(&plan_file("design.md"), "# Design\n").with_session("p1");
    let review = plan_review().with_session("r1");
    let dispatch = ScriptedDispatch::new(vec![work, review, correcting]);
    let rested = fx.drive(&run, dispatch.clone());

    let works = dispatch.of_part("work");
    assert_eq!(works.len(), 2, "no further pass");
    let correction = &works[1];
    assert_eq!(correction.pass, 1);
    assert_eq!(
        correction.instruction_field("correction"),
        crate::graduation::driver::prompt_section(
            &crate::graduation::driver::REFUSALS,
            plan::REFUSAL_PLAN_CORRECTION
        )
    );
    let instruction = correction.instruction_field("loop_instruction").unwrap_or_default();
    assert!(instruction.contains("The design is wrong."), "{instruction}");
    assert!(!instruction.contains("The button"), "{instruction}");
    assert!(!correction.instruction.contains("## Milestones"));
    assert_eq!(correction.resume_session.as_deref(), Some("w1"), "not the review's session");
    assert_eq!(rested.pass(), 1);
    assert_eq!(rested.work_turns, 2);
    assert_eq!(rested.checkpoint.review_history.len(), 2);
    // OPU-FR-WPSR: Continue resumes the plan correction turn.
    assert_eq!(rest_session(&rested).as_deref(), Some("p1"));

    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.reason, plan::REST_PLAN_UPDATE);
    assert_eq!(rest.origin.as_deref(), Some("review"));
    assert_eq!(rest.findings.map(|f| f.len()), Some(2));
}

// OPU-FR-QKLX, OPU-FR-MCSG: a plan correction turn that answers anything
// but a valid plan update rests the run on `plan_findings` with every
// finding. The plan thread opens as Synthesis with the fixed text and the
// description and correction of each `plan` finding. No review runs, no
// plan update correction turn follows, and nothing is committed. A success,
// an ordinary escalation, a refused plan update, and a reported failure are
// each such an answer.
#[test]
fn a_plan_correction_without_a_plan_update_rests_on_plan_findings() {
    let answers = [
        Turn::work(),
        Turn::answering(Answer::Escalate(vec!["Which?".into()])),
        // A refused plan update: a file outside the change.
        plan_update(questions()).writing(".synthesis-plan/src/main.ts", "x\n"),
        Turn::answering(Answer::ReportedFailure("The plan cannot be read.".into())),
    ];
    for answer in answers {
        let (fx, _, _) = change_fixture(READY);
        let stream = change_stream(&fx, "editor");
        let head = head_of(&stream.worktree());
        let run = apply_run(&fx, &stream);
        let dispatch = ScriptedDispatch::new(vec![Turn::work(), plan_review(), answer]);
        let rested = fx.drive(&run, dispatch.clone());
        assert_eq!(rested.state, GraduationRunState::AwaitingAuthor);
        assert!(rested.escalation.is_none());
        assert_eq!(dispatch.of_part("review").len(), 1);
        assert_eq!(dispatch.of_part("work").len(), 2, "no plan update correction turn");
        assert!(rested.checkpoint.plan.correction.is_none());
        let rest = rested.rest.expect("the rest");
        assert_eq!(rest.reason, plan::REST_PLAN_FINDINGS);
        assert_eq!(rest.origin.as_deref(), Some("review"));
        assert_eq!(rest.findings.map(|f| f.len()), Some(2));
        assert_eq!(rest.proposal_id, None);
        let thread_id = rest.thread_id.expect("the plan thread");
        let opened = thread(&fx, &stream.id, &thread_id);
        assert_eq!(opened.comments.len(), 1);
        assert_eq!(opened.comments[0].author, crate::comments::Participant::synthesis());
        let body = opened.comments[0].body.clone();
        assert!(body.starts_with(&plan::plan_findings_comment(CHANGE)), "{body}");
        assert!(body.contains("The design is wrong.") && body.contains("Correct The design."));
        assert!(!body.contains("The button"), "{body}");
        assert_eq!(head_of(&stream.worktree()), head);
    }
}

// OPU-FR-FDCW, GRL-FR-UQNV: one minor `plan` finding is not advisory: the
// run does not end, and the plan correction turn follows.
#[test]
fn a_minor_plan_finding_is_not_advisory() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let minor = Turn::answering(Answer::Verdict(ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: "A small plan fault.".into(),
        findings: vec![finding(FindingKind::Plan, ReviewSeverity::Minor, "The task text")],
    }));
    let dispatch = ScriptedDispatch::new(vec![Turn::work(), minor, Turn::work()]);
    let rested = fx.drive(&run, dispatch);
    assert_ne!(rested.state, GraduationRunState::Completed);
    assert_eq!(rested.rest.map(|r| r.reason).as_deref(), Some(plan::REST_PLAN_FINDINGS));
}

// OPU-FR-EPXL: a `plan` finding of a review of a draft run is read as
// `work`, so two minor ones still end the loop as advisory remarks.
#[test]
fn a_plan_finding_of_another_run_is_read_as_work() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state.");
    let minor = Turn::answering(Answer::Verdict(ReviewVerdict {
        verdict: ReviewOutcome::Revise,
        rationale: "Small things.".into(),
        findings: vec![finding(FindingKind::Plan, ReviewSeverity::Minor, "The wording")],
    }));
    let done = fx.drive(&run, ScriptedDispatch::new(vec![Turn::work(), minor]));
    assert_eq!(done.state, GraduationRunState::Completed);
}

/// A side effect that records a proposal of another thread on the change, so
/// that the step that records the plan proposal is refused with
/// `proposal_pending`.
pub(super) fn pend_other_proposal(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    run: &GraduationRun,
) -> impl Fn() + Send + Sync + 'static {
    let (app, stream_id, run_id) = (fx.app.clone(), stream.id.clone(), run.id.clone());
    move || {
        let store = app.state::<crate::project::ProjectState>().require_store().unwrap();
        let target = crate::comments::DiscussionTarget::Change {
            stream_id: stream_id.clone(),
            change: CHANGE.to_string(),
        };
        let other = crate::comments::open_discussion_unchecked(
            &store,
            &target,
            None,
            "Earlier.".into(),
            Vec::new(),
            &crate::comments::Participant::synthesis(),
            "2026-11-11T09:00:00Z",
        )
        .unwrap();
        let hunk = crate::change_proposals::PathedHunk {
            path: "proposal.md".into(),
            hunk: crate::graduation::driver::hunks_of(
                Some("# Why\n\nLogout is missing.\n"),
                "# Why\n\nGone.\n",
            )
            .remove(0),
        };
        crate::change_proposals::record_plan_proposal(
            &app,
            &store,
            &crate::change_proposals::AppSites { app: &app },
            crate::change_proposals::NewChangeProposal {
                stream_id: &stream_id,
                change: CHANGE,
                hunks: std::slice::from_ref(&hunk),
                rationale: "Earlier.",
                agent: &crate::comments::Participant::synthesis(),
                thread_id: &other.id,
            },
            &run_id,
        )
        .expect("a pending proposal");
    }
}

// OPU-FR-BKZX, GXD-FR-GMDI: a recording step that the store refuses rests
// the run `blocked` with the store's code at the resume phase `record`. The
// thread step is kept, and Continue takes the first step that is not done,
// with no turn and no pass.
#[test]
fn a_refused_recording_step_is_continued_alone() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    // Another agent's proposal waits on the change before the plan update.
    let pend = pend_other_proposal(&fx, &stream, &run);
    let dispatch = ScriptedDispatch::new(vec![updating().doing(pend)]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    assert_eq!(blocked.blocker.as_ref().unwrap().code, "proposal_pending");
    assert_eq!(blocked.checkpoint.resume_part.as_deref(), Some("record"));
    let recording = blocked.checkpoint.plan.recording.clone().expect("the recording");
    assert!(recording.thread_id.is_some() && recording.set_id.is_some());
    assert!(stream.worktree().join(".synthesis-plan").is_dir(), "the scratch folder stays");

    let earlier = proposals(&fx, &stream.id);
    crate::change_proposals::decline_change_edit_proposal(
        fx.app.clone(),
        earlier[0].id.clone(),
        None,
        fx.app.state(),
        fx.app.state(),
    )
    .expect("declined");
    let continued = fx.continue_run(&blocked);
    let rested = fx.drive(&continued, dispatch.clone());
    assert_eq!(dispatch.seen().len(), 1, "no turn is dispatched");
    assert_eq!(rested.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(rested.pass(), 1);
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.thread_id, recording.thread_id, "the thread step is not taken again");
    assert_eq!(rest.set_id, recording.set_id);
    assert!(rest.proposal_id.is_some());
    // OPU-FR-FOPN: the resumed recording removes the scratch folder.
    assert!(!stream.worktree().join(".synthesis-plan").exists());
}

// OPU-FR-WINO: the repository hides the scratch folder before the first
// turn, and the change set and the commit leave it out also where that line
// is absent.
#[test]
fn the_scratch_folder_is_no_part_of_the_change_set_or_the_commit() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let unhide = {
        let root = stream.worktree();
        move || {
            let repo = git2::Repository::open(&root).unwrap();
            let exclude = repo.commondir().join("info/exclude");
            let text = std::fs::read_to_string(&exclude).unwrap_or_default();
            assert!(text.lines().any(|l| l == plan::scratch::EXCLUDE_LINE), "{text}");
            std::fs::write(&exclude, text.replace(plan::scratch::EXCLUDE_LINE, "")).unwrap();
        }
    };
    let work = Turn::work()
        .doing(unhide)
        .writing(&change_path("tasks.md"), "- [x] Add the button\n- [x] Wire the call\n")
        .writing(&plan_file("design.md"), "# Design\n");
    let done = fx.drive(&run, ScriptedDispatch::new(vec![work, Turn::ready()]));
    assert_eq!(done.state, GraduationRunState::Completed, "{:?}", done.blocker);
    assert!(!done.checkpoint.changed_paths.iter().any(|p| p.starts_with(".synthesis-plan")));
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    assert!(tree.get_path(Path::new(".synthesis-plan")).is_err());
}

// GRL-FR-QSMY, OPU-FR-XSRD, OPU-FR-DTYE: `refusals.md` holds the two plan
// corrections. Each tells the turn to change no file outside the scratch
// folder and to answer a plan update. `plan_correction` names
// `loop_instruction` as the plan findings, and `plan_update_invalid` states
// the rules of a plan update.
#[test]
fn the_plan_corrections_say_what_to_send_instead() {
    let section = |key: &str| {
        crate::graduation::driver::prompt_section(&crate::graduation::driver::REFUSALS, key)
            .unwrap_or_else(|| panic!("refusals.md holds {key}"))
    };
    for key in [plan::REFUSAL_PLAN_UPDATE_INVALID, plan::REFUSAL_PLAN_CORRECTION] {
        let text = section(key);
        assert!(text.contains("Change no file outside `.synthesis-plan/`"), "{key}: {text}");
        assert!(text.contains("`kind: \"plan_update\"`"), "{key}: {text}");
        assert!(!text.contains("## Milestones"), "{key}");
    }
    let invalid = section(plan::REFUSAL_PLAN_UPDATE_INVALID);
    for rule in ["regular file", "UTF-8", "cannot delete a file", "at least one question"] {
        assert!(invalid.contains(rule), "{rule}: {invalid}");
    }
    assert!(section(plan::REFUSAL_PLAN_CORRECTION)
        .contains("`input.loop_instruction` holds the findings about the plan"));
}

// OPU-FR-VGFJ: the recording and the refusal of a plan update are records of
// the `backend` domain with the run, the change, the origin, the counts, and
// the ids or the cause.
#[test]
fn a_plan_update_reports_its_ids_counts_and_cause() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let outside = plan_update(questions()).writing(".synthesis-plan/x.md", "x\n");
    let corrected = plan_update(questions()).doing({
        let root = stream.worktree();
        move || {
            let _ = std::fs::remove_file(root.join(".synthesis-plan/x.md"));
        }
    });
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![outside, corrected]));
    let rest = rested.rest.expect("the rest");
    let records = backend_records();
    let of_run = |message: &str| {
        records
            .iter()
            .find(|r| {
                r.message == message && r.fields.get("run_id") == Some(&serde_json::json!(run.id))
            })
            .unwrap_or_else(|| panic!("the record {message:?}"))
            .clone()
    };
    let refused = of_run("graduation refused a plan update");
    assert_eq!(refused.level, crate::logging::LogLevel::Warn);
    assert!(refused.domains.contains(&crate::logging::Domain::Backend));
    assert_eq!(refused.fields.get("cause"), Some(&serde_json::json!("plan_path_outside_change")));
    let recorded = of_run("graduation recorded a plan rest");
    assert_eq!(recorded.level, crate::logging::LogLevel::Info);
    assert_eq!(recorded.fields.get("change"), Some(&serde_json::json!(CHANGE)));
    assert_eq!(recorded.fields.get("origin"), Some(&serde_json::json!("work")));
    assert_eq!(recorded.fields.get("questions"), Some(&serde_json::json!(2)));
    assert_eq!(recorded.fields.get("set_id"), Some(&serde_json::json!(rest.set_id.unwrap())));
    assert_eq!(recorded.fields.get("thread_id"), Some(&serde_json::json!(rest.thread_id.unwrap())));
    assert_no_text_logged(&run.id);
}
