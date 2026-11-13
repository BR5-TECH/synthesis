//! The recovery paths of a plan update (`OPU-openspec-plan-updates.md`
//! OPU-FR-BKZX, OPU-FR-TPYT, OPU-FR-YTKN, OPU-FR-ZBXF): a recording step that
//! the store refuses, a step that an earlier attempt already took, a
//! submission whose append fails after its hand-over, a plan base that cannot
//! be made, and a discard while a recording is blocked. Driven with the agent
//! scripted and the OpenSpec CLI faked.

use super::change_plan_continue::*;
use super::change_plan_updates::*;
use super::change_runs::*;
use super::*;
use crate::comments::QuestionAnswer;
use crate::graduation::plan_updates::{self as plan};

/// The folder that holds the whole-change discussions of the change.
fn comments_dir(fx: &Fixture, stream_id: &str) -> PathBuf {
    store(fx).path().join(format!("changes/{stream_id}/{CHANGE}/comments"))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A side effect that opens a discussion of the change, so that its log and
/// its folder stand, then runs `then`.
fn open_other_then(
    fx: &Fixture,
    stream_id: &str,
    then: impl Fn() + Send + Sync + 'static,
) -> impl Fn() + Send + Sync + 'static {
    let (app, stream_id) = (fx.app.clone(), stream_id.to_string());
    move || {
        let store = app.state::<crate::project::ProjectState>().require_store().unwrap();
        let target = crate::comments::DiscussionTarget::Change {
            stream_id: stream_id.clone(),
            change: CHANGE.to_string(),
        };
        crate::comments::open_discussion_unchecked(
            &store,
            &target,
            None,
            "Another thread.".into(),
            Vec::new(),
            &crate::comments::Participant::local_human(),
            "2026-11-11T09:00:00Z",
        )
        .unwrap();
        then();
    }
}

/// The run's whole record, to show that a refusal changed nothing.
fn snapshot(fx: &Fixture, run_id: &str) -> serde_json::Value {
    serde_json::to_value(fx.reload(run_id)).unwrap()
}

// OPU-FR-BKZX, OPU-FR-FOPN: a plan thread that cannot be opened rests the
// run `blocked` with `write_failed` at the phase `record`, with no step done
// and the scratch folder kept. Continue opens the thread, reserves the set,
// records the proposal, rests the run on `plan_update` with no turn and no
// pass, and removes the scratch folder.
#[cfg(unix)]
#[test]
fn a_plan_thread_that_cannot_be_opened_is_opened_on_continue() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_plan_thread_that_cannot_be_opened_is_opened_on_continue",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let log = comments_dir(&fx, &stream.id).join("discussion.jsonl");
    let lock = open_other_then(&fx, &stream.id, {
        let log = log.clone();
        move || set_mode(&log, 0o444)
    });
    let dispatch = ScriptedDispatch::new(vec![updating().doing(lock)]);
    let blocked = fx.drive(&run, dispatch.clone());
    set_mode(&log, 0o644);
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    assert_eq!(blocked.blocker.as_ref().unwrap().code, "write_failed");
    assert_eq!(blocked.checkpoint.resume_part.as_deref(), Some("record"));
    let recording = blocked.checkpoint.plan.recording.clone().expect("the recording");
    assert_eq!((recording.thread_id, recording.set_id), (None, None));
    assert!(stream.worktree().join(".synthesis-plan").is_dir(), "the scratch folder stays");

    let continued = fx.continue_run(&blocked);
    let rested = fx.drive(&continued, dispatch.clone());
    assert_eq!(dispatch.seen().len(), 1, "no turn is dispatched");
    assert_eq!(rested.state, GraduationRunState::AwaitingAuthor);
    assert_eq!(rested.pass(), 1);
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.reason, plan::REST_PLAN_UPDATE);
    let thread_id = rest.thread_id.expect("the plan thread");
    assert_eq!(thread(&fx, &stream.id, &thread_id).comments[0].body, REASON);
    assert!(rest.set_id.is_some() && rest.proposal_id.is_some());
    assert!(!stream.worktree().join(".synthesis-plan").exists(), "the scratch folder is gone");
}

// OPU-FR-BKZX, OPU-FR-FOPN: a question set that cannot be reserved rests the
// run `blocked` at the phase `record` with the thread step kept. Where an
// earlier attempt of the step reserved the set in the plan thread before its
// id reached the checkpoint, Continue takes that set as the step's own and
// does not open a second thread.
#[cfg(unix)]
#[test]
fn a_question_set_that_cannot_be_reserved_is_reserved_on_continue() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_question_set_that_cannot_be_reserved_is_reserved_on_continue",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let sets = comments_dir(&fx, &stream.id).join("question-sets");
    let lock = open_other_then(&fx, &stream.id, {
        let sets = sets.clone();
        move || {
            std::fs::create_dir_all(&sets).unwrap();
            set_mode(&sets, 0o555);
        }
    });
    let dispatch = ScriptedDispatch::new(vec![updating().doing(lock)]);
    let blocked = fx.drive(&run, dispatch.clone());
    set_mode(&sets, 0o755);
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    assert_eq!(blocked.checkpoint.resume_part.as_deref(), Some("record"));
    let recording = blocked.checkpoint.plan.recording.clone().expect("the recording");
    let thread_id = recording.thread_id.clone().expect("the thread step is done");
    assert_eq!(recording.set_id, None);
    assert!(stream.worktree().join(".synthesis-plan").is_dir());

    // The earlier attempt reserved the set, and the run stopped before it
    // kept the id.
    let held = crate::comments::PendingQuestionSet {
        set_id: "held-set".into(),
        discussion_id: thread_id.clone(),
        asked_by: crate::comments::Participant::synthesis(),
        asked_at: "2026-11-11T09:00:00Z".into(),
        questions: vec![crate::comments::PendingQuestion {
            position: 1,
            text: "Keep the old cookie name?".into(),
            options: Vec::new(),
        }],
    };
    crate::comments::reserve_discussion_question_set(
        &fx.app,
        &store(&fx),
        &project_root(&fx),
        &thread_id,
        &held,
    )
    .expect("reserved");

    let continued = fx.continue_run(&blocked);
    let rested = fx.drive(&continued, dispatch.clone());
    assert_eq!(dispatch.seen().len(), 1, "no turn is dispatched");
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.thread_id.as_deref(), Some(thread_id.as_str()));
    assert_eq!(rest.set_id.as_deref(), Some("held-set"));
    assert!(rest.proposal_id.is_some());
    assert!(!stream.worktree().join(".synthesis-plan").exists());
}

// OPU-FR-BKZX: where an earlier attempt of the proposal step recorded the
// plan proposal in the plan thread before its id reached the checkpoint,
// Continue takes that proposal as the step's own and records no second one.
#[test]
fn a_plan_proposal_that_an_earlier_attempt_recorded_is_taken_on_continue() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let pend = pend_other_proposal(&fx, &stream, &run);
    let dispatch = ScriptedDispatch::new(vec![updating().doing(pend)]);
    let blocked = fx.drive(&run, dispatch.clone());
    assert_eq!(blocked.blocker.as_ref().unwrap().code, "proposal_pending");
    let thread_id = blocked.checkpoint.plan.recording.clone().unwrap().thread_id.unwrap();
    let earlier = proposals(&fx, &stream.id);
    crate::change_proposals::decline_change_edit_proposal(
        fx.app.clone(),
        earlier[0].id.clone(),
        None,
        fx.app.state(),
        fx.app.state(),
    )
    .expect("declined");

    let hunk = crate::change_proposals::PathedHunk {
        path: "design.md".into(),
        hunk: crate::graduation::driver::hunks_of(None, "# Design\n").remove(0),
    };
    let recorded = crate::change_proposals::record_plan_proposal(
        &fx.app,
        &store(&fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        crate::change_proposals::NewChangeProposal {
            stream_id: &stream.id,
            change: CHANGE,
            hunks: std::slice::from_ref(&hunk),
            rationale: REASON,
            agent: &crate::comments::Participant::synthesis(),
            thread_id: &thread_id,
        },
        &run.id,
    )
    .expect("the earlier attempt");

    let continued = fx.continue_run(&blocked);
    let rested = fx.drive(&continued, dispatch.clone());
    assert_eq!(dispatch.seen().len(), 1);
    let rest = rested.rest.expect("the rest");
    assert_eq!(rest.proposal_id.as_deref(), Some(recorded.id.as_str()));
    assert_eq!(proposals(&fx, &stream.id).len(), 2, "no second plan proposal");
    assert!(!stream.worktree().join(".synthesis-plan").exists());
}

// OPU-FR-ZBXF, CMS-FR-ECWC: a discard of a run whose recording reserved the
// question set and then rested `blocked` deletes that set. The plan thread
// stays.
#[test]
fn a_discard_while_the_recording_is_blocked_deletes_the_reserved_set() {
    let (fx, _, _) = change_fixture(READY);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let pend = pend_other_proposal(&fx, &stream, &run);
    let blocked = fx.drive(&run, ScriptedDispatch::new(vec![updating().doing(pend)]));
    assert_eq!(blocked.state, GraduationRunState::Blocked);
    let recording = blocked.checkpoint.plan.recording.clone().unwrap();
    let thread_id = recording.thread_id.clone().unwrap();
    let held = crate::comments::read_question_set(&store(&fx), &project_root(&fx), &thread_id)
        .expect("the reserved set");
    assert_eq!(Some(held.set_id), recording.set_id);

    crate::graduation::discard_graduation_run(fx.app.clone(), blocked.id.clone())
        .expect("discarded");
    assert!(
        crate::comments::read_question_set(&store(&fx), &project_root(&fx), &thread_id).is_none()
    );
    assert_eq!(thread(&fx, &stream.id, &thread_id).comments[0].body, REASON);
}

// OPU-FR-TPYT, OPU-FR-THOZ: a submission that is not acceptable writes no
// answers on the rest. A submission whose append fails after its hand-over
// takes the answers back, so the set stands and Continue stays refused with
// `plan_questions_pending`. A later submission then hands its answers over.
#[cfg(unix)]
#[test]
fn a_submission_whose_append_fails_takes_its_answers_back() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_submission_whose_append_fails_takes_its_answers_back",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_update(&fx, &stream);
    let rest = rested.rest.clone().unwrap();
    let thread_id = rest.thread_id.clone().unwrap();
    let set_id = rest.set_id.clone().unwrap();
    let submit_with = |answers: Vec<QuestionAnswer>| {
        crate::comments::submit_discussion_question_answers(
            fx.app.clone(),
            thread_id.clone(),
            set_id.clone(),
            answers,
            fx.app.state(),
            fx.app.state(),
        )
        .map(|_| ())
    };

    let incomplete = vec![answers().remove(0)];
    assert_eq!(submit_with(incomplete).unwrap_err(), "question_answers_incomplete");
    assert_eq!(fx.reload(&rested.id).rest.unwrap().answers, None, "no hand-over");

    let comments = thread(&fx, &stream.id, &thread_id).comments.len();
    let log = comments_dir(&fx, &stream.id).join("discussion.jsonl");
    set_mode(&log, 0o444);
    let failed = submit_with(answers());
    set_mode(&log, 0o644);
    assert!(failed.is_err());
    assert_eq!(thread(&fx, &stream.id, &thread_id).comments.len(), comments, "nothing appended");
    assert!(
        crate::comments::read_question_set(&store(&fx), &project_root(&fx), &thread_id).is_some()
    );
    assert_eq!(fx.reload(&rested.id).rest.unwrap().answers, None, "the answers are taken back");
    assert_eq!(continue_of(&fx, &rested).unwrap_err(), "plan_questions_pending");
    let taken_back = backend_records().into_iter().any(|r| {
        r.message == "graduation took back plan update answers whose submission failed"
            && r.fields.get("run_id") == Some(&serde_json::json!(rested.id))
    });
    assert!(taken_back, "the undo is logged");

    submit_with(answers()).expect("submitted");
    assert_eq!(fx.reload(&rested.id).rest.unwrap().answers, Some(answers()));
    assert_no_text_logged(&rested.id);
}

// OPU-FR-TPYT: a later hand-over of the same set replaces the answers that
// the rest holds, and reports the answers it replaced.
#[test]
fn a_later_hand_over_replaces_the_answers() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let rested = rested_on_update(&fx, &stream);
    let set_id = rested.rest.clone().unwrap().set_id.unwrap();
    let first = plan::hand_over_answers(&fx.app, &set_id, &answers()).unwrap().expect("handed");
    assert_eq!(first.previous, None);
    let mut other = answers();
    other[1].own_answer = Some("The users table.".into());
    let second = plan::hand_over_answers(&fx.app, &set_id, &other).unwrap().expect("handed");
    assert_eq!(second.previous, Some(answers()));
    assert_eq!(fx.reload(&rested.id).rest.unwrap().answers, Some(other));
    assert_eq!(plan::hand_over_answers(&fx.app, "another-set", &answers()), Ok(None));
}

// OPU-FR-YTKN, OPU-FR-THOZ: a plan base that cannot be made, because the
// close commit of the plan proposal is not in the repository, refuses
// Continue with `write_failed` and changes nothing: the rest, the ledger,
// the base commit, and the private ref stay as they were.
#[test]
fn a_plan_base_that_cannot_be_made_refuses_continue_and_changes_nothing() {
    let (fx, _, _) = change_fixture(ALL_DONE);
    let stream = change_stream(&fx, "editor");
    let run = apply_run(&fx, &stream);
    let update = plan_update(Vec::new()).writing(&plan_file("design.md"), "# Design\n");
    let rested = fx.drive(&run, ScriptedDispatch::new(vec![update]));
    let rest = rested.rest.clone().unwrap();
    let proposal_id = rest.proposal_id.clone().unwrap();
    accept(&fx, &proposal_id, &hunk_id(&fx, &proposal_id, "design.md"), None).expect("accepted");
    let close = plan_proposal(&fx, &rest).commit.expect("the close commit");

    let file = store(&fx)
        .path()
        .join(format!("changes/{}/{CHANGE}/proposals/{proposal_id}.toml", stream.id));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains(&close), "{text}");
    let missing = "0123456789012345678901234567890123456789";
    std::fs::write(&file, text.replace(&close, missing)).unwrap();

    let before = snapshot(&fx, &rested.id);
    assert_eq!(continue_of(&fx, &rested).unwrap_err(), "write_failed");
    assert_eq!(snapshot(&fx, &rested.id), before, "a refused Continue changes nothing");
    let repo = git2::Repository::open(stream.worktree()).unwrap();
    assert!(repo.find_reference(&plan::plan_base::ref_name(&rested.id)).is_err());
}
