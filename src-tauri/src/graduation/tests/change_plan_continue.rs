//! Continue from a plan rest, and what the stores of other modules do while
//! one stands (`OPU-openspec-plan-updates.md` OPU-FR-QUST, OPU-FR-EUZU,
//! OPU-FR-WOKJ, OPU-FR-TPYT through OPU-FR-ZBXF; `CEP-change-edit-proposals.md`
//! CEP-FR-YYZZ, CEP-FR-KGOL, CEP-FR-DQAJ; `CMS-comments-storage.md`
//! CMS-FR-ECWC), driven with the agent scripted and the OpenSpec CLI faked.

use super::change_plan_updates::*;
use super::change_runs::*;
use super::*;
use crate::change_proposals::{ChangeEditProposal, RecordRefusal};
use crate::comments::QuestionAnswer;
use crate::graduation::plan_updates::{self as plan};

pub(super) fn continue_of(fx: &Fixture, run: &GraduationRun) -> Result<GraduationRun, String> {
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let continued = crate::graduation::continue_graduation_run(fx.app.clone(), run.id.clone());
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    continued.map_err(String::from)
}

pub(super) fn answers() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            question_position: 1,
            option_position: Some(1),
            option_value: Some("Keep the name".into()),
            own_answer: None,
            note: Some("Old clients matter.".into()),
        },
        QuestionAnswer {
            question_position: 2,
            option_position: None,
            option_value: None,
            own_answer: Some("A new sessions table.".into()),
            note: None,
        },
    ]
}

pub(super) fn submit(fx: &Fixture, rest: &GraduationRest) -> Result<(), String> {
    crate::comments::submit_discussion_question_answers(
        fx.app.clone(),
        rest.thread_id.clone().unwrap(),
        rest.set_id.clone().unwrap(),
        answers(),
        fx.app.state(),
        fx.app.state(),
    )
    .map(|_| ())
}

pub(super) fn hunk_id(fx: &Fixture, proposal_id: &str, path: &str) -> String {
    hunks(fx, proposal_id).into_iter().find(|h| h.path == path).expect("the hunk").hunk.id
}

pub(super) fn accept(
    fx: &Fixture,
    proposal_id: &str,
    hunk: &str,
    feedback: Option<&str>,
) -> Result<(), String> {
    crate::change_proposals::accept_change_edit_hunk(
        fx.app.clone(),
        proposal_id.to_string(),
        hunk.to_string(),
        feedback.map(str::to_string),
        fx.app.state(),
        fx.app.state(),
    )
    .map(|_| ())
}

pub(super) fn reject(
    fx: &Fixture,
    proposal_id: &str,
    hunk: &str,
    feedback: Option<&str>,
) -> Result<(), String> {
    crate::change_proposals::reject_change_edit_hunk(
        fx.app.clone(),
        proposal_id.to_string(),
        hunk.to_string(),
        feedback.map(str::to_string),
        fx.app.state(),
        fx.app.state(),
    )
    .map(|_| ())
}

pub(super) fn plan_proposal(fx: &Fixture, rest: &GraduationRest) -> ChangeEditProposal {
    crate::change_proposals::read_change_proposal(&store(fx), rest.proposal_id.as_deref().unwrap())
        .expect("the plan proposal")
}

/// An apply run at rest on the plan update of [`updating`].
pub(super) fn rested_on_update(fx: &Fixture, stream: &crate::streams::WorkStream) -> GraduationRun {
    let run = apply_run(fx, stream);
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![updating()]));
    assert_eq!(rested.rest.as_ref().map(|r| r.reason.as_str()), Some(plan::REST_PLAN_UPDATE));
    rested
}

/// A turn that completes the work once the plan is decided.
pub(super) fn finishing() -> Turn {
    Turn::work()
        .writing(&change_path("tasks.md"), "- [x] Add the button\n- [x] Wire the call\n")
        .writing("src/wire.ts", "export const wire = 1;\n")
}

// OPU-FR-QUST: the author's answers to an escalation are refused at a plan
// rest with `run_state_not_permitted`, and nothing is recorded.
#[test]
fn an_escalation_answer_is_refused_at_a_plan_rest() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_update(&fx, &stream);
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
    assert_eq!(fx.reload(&rested.id).rest, rested.rest);
    assert!(fx.reload(&rested.id).checkpoint.decisions.is_empty());
}

// OPU-FR-THOZ, OPU-FR-EVBZ, CEP-FR-KGOL, OPU-FR-TPYT, OPU-FR-GYHC,
// OPU-FR-PXDY, OPU-FR-CAGI, OPU-FR-ZNRR, OPU-FR-NQBK, OPU-FR-YNXV,
// OPU-FR-FSCV, OPU-FR-YTKN, OPU-FR-WPSR, OPU-FR-SKTV, OPU-FR-EUZU,
// CEP-FR-DQAJ, GXD-FR-OEPX, GRD-FR-YBUM, GRL-FR-LAVF, OPU-FR-VGFJ: Continue
// refuses, in
// order, while the questions stand, while the edits wait, and while OpenSpec
// cannot be read, each changing nothing. A decision waits on the questions,
// and the submission hands its answers to the rest first. Then Continue enters
// the answers and the hunk decisions in the ledger, moves the base to the plan
// base, resets the used time, and the work turn of the same pass takes the
// decisions in the session of the turn that asked. The run's commit holds its
// own work alone, and the planning gate passes over the accepted edits. The
// hand-over, each refusal, and the Continue are logged with no text.
#[test]
fn continue_from_a_plan_update_takes_the_decisions_on_the_plan_base() {
    let (fx, runner, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let base = head_of(&stream.worktree());
    let mut rested = rested_on_update(&fx, &stream);
    rested.turn_time_used_ms = 5_000;
    rested.checkpoint.continue_check_correction = Some(GraduationCheckCause::ChecksRed);
    crate::graduation::save_run(&fx.app, &mut rested).unwrap();
    let rest = rested.rest.clone().unwrap();
    let proposal_id = rest.proposal_id.clone().unwrap();

    let record = |fx: &Fixture| serde_json::to_value(fx.reload(&rested.id)).unwrap();
    let before = record(&fx);
    assert_eq!(continue_of(&fx, &rested).unwrap_err(), "plan_questions_pending");
    assert_eq!(record(&fx), before, "a refusal changes nothing");

    let design = hunk_id(&fx, &proposal_id, "design.md");
    let reason = hunk_id(&fx, &proposal_id, "proposal.md");
    assert_eq!(accept(&fx, &proposal_id, &design, None).unwrap_err(), "plan_questions_pending");
    assert_eq!(reject(&fx, &proposal_id, &reason, None).unwrap_err(), "plan_questions_pending");
    let held =
        crate::change_proposals::hold_in_process(&fx.app, &store(&fx), &proposal_id, &design, true);
    assert_eq!(held.unwrap_err(), "plan_questions_pending");

    submit(&fx, &rest).expect("the answers are submitted");
    assert_eq!(fx.reload(&rested.id).rest.unwrap().answers, Some(answers()));
    assert!(crate::comments::read_question_set(
        &store(&fx),
        &project_root(&fx),
        rest.thread_id.as_deref().unwrap()
    )
    .is_none());

    let before = record(&fx);
    assert_eq!(continue_of(&fx, &rested).unwrap_err(), "proposal_pending");
    assert_eq!(record(&fx), before, "a refusal changes nothing");
    accept(&fx, &proposal_id, &design, Some("Right.")).expect("accepted");
    reject(&fx, &proposal_id, &reason, Some("Keep the old reason.")).expect("rejected");
    let closed = plan_proposal(&fx, &rest);
    let close = closed.commit.clone().expect("the close commit is recorded");
    assert_eq!(head_of(&stream.worktree()), close);

    // OPU-FR-PXDY, OFP-FR-QLQJ, OFP-FR-ZDEE: a read that fails refuses
    // Continue with an `OpenSpecFailure` whose cause is the read's code and
    // whose diagnostics are the read's.
    let stalled = b"Node stalled".to_vec();
    runner.set("validate", Err(crate::openspec::ProcessError::TimedOut { stderr: stalled }));
    let before = record(&fx);
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let refused =
        crate::graduation::continue_graduation_run(fx.app.clone(), rested.id.clone()).unwrap_err();
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    assert!(refused.to_string().starts_with("openspec_validation_unavailable:"), "{refused}");
    let failure = refused.openspec().expect("an OpenSpec failure");
    assert_eq!(failure.code, "openspec_validation_unavailable");
    assert_eq!(failure.cause.as_deref(), Some("cli_timed_out"));
    assert_eq!(failure.change.as_deref(), Some(CHANGE));
    assert_eq!(failure.diagnostics[0].message, "Node stalled");
    assert_eq!(record(&fx), before, "a refusal changes nothing");
    runner.set("validate", Ok(crate::openspec::tests::ok(VALID)));

    let continued = continue_of(&fx, &rested).expect("continued");
    assert_eq!(continued.state, GraduationRunState::Queued);
    assert!(continued.rest.is_none());
    // OPU-FR-VGFJ: the hand-over, the refusals, and the Continue each log
    // under `backend` with the ids and the counts or the code.
    let of_run = |message: &str| -> Vec<crate::logging::LogRecord> {
        backend_records()
            .into_iter()
            .filter(|r| r.message == message)
            .filter(|r| r.fields.get("run_id") == Some(&serde_json::json!(rested.id)))
            .collect()
    };
    let handed = of_run("graduation took the answers of a plan update");
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].level, crate::logging::LogLevel::Info);
    assert_eq!(handed[0].fields.get("answers"), Some(&serde_json::json!(2)));
    assert_eq!(handed[0].fields.get("set_id"), Some(&serde_json::json!(rest.set_id)));
    let refusals: Vec<serde_json::Value> = of_run("graduation refused to continue a plan rest")
        .into_iter()
        .inspect(|r| assert_eq!(r.level, crate::logging::LogLevel::Warn))
        .map(|r| r.fields.get("code").cloned().unwrap_or_default())
        .collect();
    assert_eq!(refusals.len(), 3, "{refusals:?}");
    assert_eq!(refusals[0], serde_json::json!("plan_questions_pending"));
    assert_eq!(refusals[1], serde_json::json!("proposal_pending"));
    assert!(refusals[2].as_str().unwrap().starts_with("openspec_validation_unavailable:"));
    let accepted = of_run("graduation continued a plan rest");
    assert_eq!(accepted.len(), 1);
    let fields = &accepted[0].fields;
    assert_eq!(accepted[0].level, crate::logging::LogLevel::Info);
    assert!(accepted[0].domains.contains(&crate::logging::Domain::Backend));
    assert_eq!(fields.get("change"), Some(&serde_json::json!(CHANGE)));
    assert_eq!(fields.get("origin"), Some(&serde_json::json!("work")));
    assert_eq!(fields.get("proposal_id"), Some(&serde_json::json!(proposal_id)));
    assert_eq!(fields.get("entries"), Some(&serde_json::json!(4)));
    assert_eq!(fields.get("plan_base"), Some(&serde_json::json!(true)));
    assert_eq!(continued.turn_time_used_ms, 0);
    assert_eq!(continued.pass(), 1);
    // GRL-FR-LAVF: the turn that takes the decisions starts a gate episode.
    assert_eq!(continued.checkpoint.check_gate, None);
    assert_eq!(continued.checkpoint.continue_check_correction, None);
    let entries: Vec<(&str, &str, &str, Option<&str>)> = continued
        .checkpoint
        .decisions
        .iter()
        .map(|d| (d.question.as_str(), d.answer.as_str(), d.summary.as_str(), d.note.as_deref()))
        .collect();
    assert_eq!(
        entries,
        vec![
            (
                "Keep the old cookie name?",
                "Keep the name session_id.",
                "Keep the name",
                Some("Old clients matter.")
            ),
            (
                "Which table holds the sessions?",
                "A new sessions table.",
                "A new sessions table.",
                None
            ),
            ("design.md edit 1", "accepted", "accepted", Some("Right.")),
            ("proposal.md edit 1", "rejected", "rejected", Some("Keep the old reason.")),
        ]
    );
    assert!(continued.checkpoint.decisions.iter().all(|d| d.turn == "work" && d.pass == 1));

    let plan_base = continued.base_commit.clone().unwrap();
    assert_ne!(plan_base, base);
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let made = repo.find_commit(git2::Oid::from_str(&plan_base).unwrap()).unwrap();
    assert_eq!(made.parent_id(0).unwrap().to_string(), base);
    assert_eq!(
        repo.refname_to_id(&plan::plan_base::ref_name(&rested.id)).unwrap().to_string(),
        plan_base
    );
    assert!(made.tree().unwrap().get_path(Path::new(&change_path("design.md"))).is_ok());

    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    let done = fx.drive(&continued, dispatch.clone());
    let work = &dispatch.of_part("work")[0];
    assert_eq!(work.purpose, "escalation_answer");
    assert_eq!(work.pass, 1);
    assert_eq!(work.resume_session.as_deref(), Some(UPDATE_SESSION));
    assert_eq!(
        work.input["plan_update"],
        serde_json::json!({ "origin": "work", "reason": REASON, "hunks": [
            { "path": "design.md", "position": 1, "outcome": "accepted" },
            { "path": "proposal.md", "position": 1, "outcome": "rejected" },
        ] })
    );
    assert_eq!(work.input["openspec_validation"]["valid"], true);
    assert_eq!(work.input["openspec_apply_progress"]["state"], "all_done");
    assert_eq!(work.list_len("decisions"), 4);

    assert_eq!(done.state, GraduationRunState::Completed, "{:?}", done.observability.passes.last());
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_id(0).unwrap().to_string(), close, "the run commits on the plan commit");
    let diff = repo
        .diff_tree_to_tree(
            Some(&head.parent(0).unwrap().tree().unwrap()),
            Some(&head.tree().unwrap()),
            None,
        )
        .unwrap();
    let mut paths: Vec<String> = diff
        .deltas()
        .filter_map(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec![change_path("tasks.md"), "src/logout.ts".to_string(), "src/wire.ts".to_string()]
    );
    // OPU-FR-FSCV: the private ref goes when the run ends.
    assert!(repo.find_reference(&plan::plan_base::ref_name(&rested.id)).is_err());
    assert_no_text_logged(&rested.id);
}

// OPU-FR-DMCX, CEP-FR-XVXQ: a hunk of the plan proposal may edit `tasks.md`.
// Its close commit takes the file as it stands in the working copy, with the
// task that the run marked done, and holds that file alone: the run's other
// work stays out of it.
#[test]
fn an_accepted_tasks_edit_carries_the_marks_of_the_run() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let marked = "- [x] Add the button\n- [ ] Wire the call\n";
    let update = plan_update(Vec::new())
        .writing("src/logout.ts", "export const logout = () => {};\n")
        .writing(&change_path("tasks.md"), marked)
        .writing(&plan_file("tasks.md"), &format!("{marked}- [ ] Add the table\n"));
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![update]));
    let rest = rested.rest.clone().unwrap();
    let proposal_id = rest.proposal_id.clone().unwrap();
    accept(&fx, &proposal_id, &hunk_id(&fx, &proposal_id, "tasks.md"), None).expect("accepted");
    let close = plan_proposal(&fx, &rest).commit.expect("committed");
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    let commit = repo.find_commit(git2::Oid::from_str(&close).unwrap()).unwrap();
    let tree = commit.tree().unwrap();
    let blob =
        tree.get_path(Path::new(&change_path("tasks.md"))).unwrap().to_object(&repo).unwrap();
    let text = String::from_utf8(blob.as_blob().unwrap().content().to_vec()).unwrap();
    let working = std::fs::read_to_string(stream.worktree().join(change_path("tasks.md"))).unwrap();
    assert_eq!(text, working, "the file as it stands in the working copy");
    assert_eq!(text, format!("{marked}- [ ] Add the table\n"));
    let parent = commit.parent(0).unwrap().tree().unwrap();
    let diff = repo.diff_tree_to_tree(Some(&parent), Some(&tree), None).unwrap();
    let paths: Vec<String> = diff
        .deltas()
        .filter_map(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
        .collect();
    assert_eq!(paths, vec![change_path("tasks.md")], "the close commit holds tasks.md alone");
    assert!(stream.worktree().join("src/logout.ts").is_file(), "the run's work stays");
}

/// An apply run at rest on `plan_findings`.
pub(super) fn rested_on_findings(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
) -> GraduationRun {
    let run = apply_run(fx, stream);
    let rested = fx.drive(
        &run,
        ScriptedDispatch::new(vec![Turn::work().with_session("s1"), plan_review(), Turn::work()]),
    );
    assert_eq!(rested.rest.as_ref().map(|r| r.reason.as_str()), Some(plan::REST_PLAN_FINDINGS));
    rested
}

pub(super) fn record_into(
    fx: &Fixture,
    stream_id: &str,
    thread_id: &str,
) -> Result<ChangeEditProposal, RecordRefusal> {
    let hunk = crate::change_proposals::PathedHunk {
        path: "design.md".into(),
        hunk: crate::graduation::driver::hunks_of(None, "# Design\n").remove(0),
    };
    crate::change_proposals::record_change_proposal(
        &fx.app,
        &store(fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        crate::change_proposals::NewChangeProposal {
            stream_id,
            change: CHANGE,
            hunks: std::slice::from_ref(&hunk),
            rationale: "The design.",
            agent: &crate::comments::Participant::synthesis(),
            thread_id,
        },
    )
}

pub(super) fn other_thread(fx: &Fixture, stream_id: &str) -> String {
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream_id.to_string(),
        change: CHANGE.to_string(),
    };
    crate::comments::open_discussion_unchecked(
        &store(fx),
        &target,
        None,
        "Another thread.".into(),
        Vec::new(),
        &crate::comments::Participant::local_human(),
        "2026-11-11T09:00:00Z",
    )
    .unwrap()
    .id
}

// OPU-FR-DEEB, OPU-FR-WOKJ, CEP-FR-YYZZ, OCR-FR-RRUU, PCE-FR-ITEV: while an
// apply run of the change is not terminal, a recording on the change is
// refused with `apply_running`. At a `plan_findings` rest with no plan
// proposal, a recording into the plan thread is admitted, and the run writes
// its id on the rest; after that, the plan thread is refused too.
#[test]
fn a_recording_on_the_change_is_refused_while_the_apply_run_stands() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_findings(&fx, &stream);
    let rest = rested.rest.clone().unwrap();
    let other = other_thread(&fx, &stream.id);
    assert_eq!(record_into(&fx, &stream.id, &other).unwrap_err(), RecordRefusal::ApplyRunning);
    assert_eq!(RecordRefusal::ApplyRunning.code(), "apply_running");

    let admitted =
        record_into(&fx, &stream.id, rest.thread_id.as_deref().unwrap()).expect("admitted");
    assert_eq!(fx.reload(&rested.id).rest.unwrap().proposal_id, Some(admitted.id.clone()));
    crate::change_proposals::decline_change_edit_proposal(
        fx.app.clone(),
        admitted.id.clone(),
        None,
        fx.app.state(),
        fx.app.state(),
    )
    .expect("declined");
    // CEP-FR-DQAJ: a proposal that closes with no accepted hunk records no
    // commit.
    let declined =
        crate::change_proposals::read_change_proposal(&store(&fx), &admitted.id).unwrap();
    assert_eq!(declined.commit, None);
    assert_eq!(
        record_into(&fx, &stream.id, rest.thread_id.as_deref().unwrap()).unwrap_err(),
        RecordRefusal::ApplyRunning
    );

    // Once the run has ended, the change takes proposals again.
    crate::graduation::discard_graduation_run(fx.app.clone(), rested.id.clone())
        .expect("discarded");
    assert!(record_into(&fx, &stream.id, &other).is_ok());
}

// OPU-FR-TZBD, OPU-FR-EUZU: Continue from a `plan_findings` rest with no plan
// proposal dispatches a work turn of the same pass with the purpose `resume`,
// every finding as `loop_instruction`, and no `plan_update`. It appends no
// ledger entry, makes no plan base, and resets the used time but not the pass
// budget.
#[test]
fn continue_from_plan_findings_without_a_proposal_resumes_on_every_finding() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let mut rested = rested_on_findings(&fx, &stream);
    let base = rested.base_commit.clone();
    rested.turn_time_used_ms = 7_000;
    crate::graduation::save_run(&fx.app, &mut rested).unwrap();
    let continued = continue_of(&fx, &rested).expect("continued");
    assert_eq!(continued.turn_time_used_ms, 0);
    assert!(continued.checkpoint.decisions.is_empty());
    assert_eq!(continued.base_commit, base);
    assert_eq!(continued.pass(), 1);
    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    fx.drive(&continued, dispatch.clone());
    let work = &dispatch.of_part("work")[0];
    assert_eq!(work.purpose, "resume");
    assert_eq!(work.pass, 1);
    let instruction = work.instruction_field("loop_instruction").unwrap_or_default();
    assert!(
        instruction.contains("The design is wrong.")
            && instruction.contains("The button is wrong.")
    );
    assert!(work.input.get("plan_update").is_none());
}

// OPU-FR-WOKJ, OPU-FR-SKTV, OPU-FR-ZNRR: a plan proposal admitted at a
// `plan_findings` rest is the plan of the run's Continue: the turn carries
// the `review` context with no reason, the `work` findings alone as
// `loop_instruction`, and the hunk decision in the ledger.
#[test]
fn continue_from_plan_findings_takes_the_admitted_proposal() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_findings(&fx, &stream);
    let rest = rested.rest.clone().unwrap();
    let admitted =
        record_into(&fx, &stream.id, rest.thread_id.as_deref().unwrap()).expect("admitted");
    assert_eq!(continue_of(&fx, &rested).unwrap_err(), "proposal_pending");
    accept(&fx, &admitted.id, &hunk_id(&fx, &admitted.id, "design.md"), None).expect("accepted");
    let continued = continue_of(&fx, &rested).expect("continued");
    assert_eq!(continued.checkpoint.decisions.len(), 1);
    assert_eq!(continued.checkpoint.decisions[0].question, "design.md edit 1");
    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    fx.drive(&continued, dispatch.clone());
    let work = &dispatch.of_part("work")[0];
    assert_eq!(work.purpose, "escalation_answer");
    assert_eq!(work.input["plan_update"]["origin"], "review");
    assert!(work.input["plan_update"].get("reason").is_none());
    let instruction = work.instruction_field("loop_instruction").unwrap_or_default();
    assert!(
        instruction.contains("The button is wrong.")
            && !instruction.contains("The design is wrong.")
    );
}

// OPU-FR-VKBO: Continue from `plan_update_invalid` dispatches a fresh work
// turn of the same pass with the purpose `resume`, and keeps the used time.
#[test]
fn continue_from_plan_update_invalid_resumes_fresh_and_keeps_the_time() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let empty = || plan_update(Vec::new());
    let mut rested = fx.drive(&run, ScriptedDispatch::new(vec![empty(), empty()]));
    assert_eq!(
        rested.rest.as_ref().map(|r| r.reason.as_str()),
        Some(plan::REST_PLAN_UPDATE_INVALID)
    );
    rested.turn_time_used_ms = 3_000;
    crate::graduation::save_run(&fx.app, &mut rested).unwrap();
    let continued = continue_of(&fx, &rested).expect("continued");
    assert_eq!(continued.turn_time_used_ms, 3_000);
    let dispatch = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    let done = fx.drive(&continued, dispatch.clone());
    let work = &dispatch.of_part("work")[0];
    assert_eq!((work.purpose.as_str(), work.pass), ("resume", 1));
    assert_eq!(work.resume_session, None, "a fresh turn");
    assert_eq!(done.state, GraduationRunState::Completed);
}

// OPU-FR-ZBXF, CMS-FR-ECWC, CMS-FR-EKUP: a discard of a run at rest on
// `plan_update` deletes its question set. The plan thread and the plan
// proposal stay, and the proposal stays decidable.
#[test]
fn a_discard_deletes_the_question_set_and_keeps_the_thread_and_the_proposal() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_update(&fx, &stream);
    let rest = rested.rest.clone().unwrap();
    let thread_id = rest.thread_id.clone().unwrap();
    crate::graduation::discard_graduation_run(fx.app.clone(), rested.id.clone())
        .expect("discarded");
    assert!(
        crate::comments::read_question_set(&store(&fx), &project_root(&fx), &thread_id).is_none()
    );
    assert_eq!(thread(&fx, &stream.id, &thread_id).comments[0].body, REASON);
    let proposal_id = rest.proposal_id.clone().unwrap();
    accept(&fx, &proposal_id, &hunk_id(&fx, &proposal_id, "design.md"), None).expect("decidable");

    // CMS-FR-ECWC: where the discussion holds no set, a deletion changes
    // nothing.
    assert_eq!(
        crate::comments::delete_question_set_record(
            &store(&fx),
            &project_root(&fx),
            &thread_id,
            "x"
        ),
        Ok(false)
    );
}

// OPU-FR-TPYT: a submission whose hand-over cannot be written is refused
// with `write_failed`, and nothing is appended to the plan thread.
#[cfg(unix)]
#[test]
fn a_submission_whose_hand_over_fails_appends_nothing() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_submission_whose_hand_over_fails_appends_nothing",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_update(&fx, &stream);
    let rest = rested.rest.clone().unwrap();
    let thread_id = rest.thread_id.clone().unwrap();
    let before = thread(&fx, &stream.id, &thread_id).comments.len();
    let home = crate::graduation::store_base(&fx.app).unwrap().home(&rested.id);
    let refused = {
        let _guard = super::checkout_paths::ReadOnly::new(&home);
        submit(&fx, &rest)
    };
    assert_eq!(refused.unwrap_err(), "write_failed");
    assert_eq!(thread(&fx, &stream.id, &thread_id).comments.len(), before);
    assert!(
        crate::comments::read_question_set(&store(&fx), &project_root(&fx), &thread_id).is_some()
    );
    assert_eq!(fx.reload(&rested.id).rest.unwrap().answers, None);
}

// OPU-FR-EUZU, GRD-FR-WKMD: a plan rest at the last pass of the budget holds
// no escalation and keeps its queue, and its Continue does not reset the
// pass budget: the turn after it is of the same pass.
#[test]
fn continue_from_a_plan_rest_at_the_last_pass_keeps_the_pass() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work(),
        Turn::revise(ReviewSeverity::Major, "The button is missing."),
        Turn::work().with_session("s1"),
        plan_review(),
        Turn::work(),
    ]);
    let rested = fx.drive(&run, dispatch);
    assert_eq!(rested.rest.as_ref().map(|r| r.reason.as_str()), Some(plan::REST_PLAN_FINDINGS));
    assert!(rested.escalation.is_none());
    assert!(rested.holds_queue());
    let pass = rested.pass();
    assert_eq!(pass, rested.checkpoint.pass_limit, "the run rests at the last pass");
    let continued = continue_of(&fx, &rested).expect("continued");
    assert_eq!(continued.pass(), pass);
    assert_eq!(continued.checkpoint.pass_floor, rested.checkpoint.pass_floor);
    let next = ScriptedDispatch::new(vec![finishing(), Turn::ready()]);
    fx.drive(&continued, next.clone());
    assert_eq!(next.of_part("work")[0].pass, pass);
}
