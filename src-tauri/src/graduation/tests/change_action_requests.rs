//! A confirmed change action request starts its run through the real queue
//! (`CAR-change-action-requests.md` CAR-FR-MMPV, CAR-FR-UPOV, CAR-FR-ARMK),
//! and the fix-ups of the starts that the request depends on
//! (`OCR-openspec-change-runs.md` OCR-FR-LNKQ, OCR-FR-SFDE, OCR-FR-TWDK).
//!
//! The OpenSpec CLI is faked and the loop is off, so each run waits in its
//! queue (OCR-FR-HNXC).

use super::change_runs::{
    assert_object, change_fixture, change_stream, set_resolved, start_apply, BLOCKED, CHANGE, READY,
};
use super::*;
use crate::change_action_requests::{self as car, ChangeAction, ChangeActionRequestState};

fn queue_len(fx: &Fixture) -> usize {
    crate::graduation::queue::project_queue(&fx.app).map(|q| q.runs.len()).unwrap_or_default()
}

fn store(fx: &Fixture) -> crate::fs::RootFs {
    fx.app.state::<crate::project::ProjectState>().require_store().unwrap()
}

fn agent() -> crate::comments::Participant {
    crate::comments::Participant::Agent {
        agent_id: "a1".into(),
        handle: "Ada".into(),
        model: None,
        title: Some(String::new()),
    }
}

/// A whole-change discussion of the stream's change, as the author opens it.
fn open_thread(fx: &Fixture, stream_id: &str, change: &str) -> String {
    crate::comments::open_discussion_unchecked(
        &store(fx),
        &crate::comments::DiscussionTarget::Change {
            stream_id: stream_id.to_string(),
            change: change.to_string(),
        },
        None,
        "@ada is it ready?".into(),
        Vec::new(),
        &crate::comments::Participant::local_human(),
        &crate::notes::now_rfc3339(),
    )
    .unwrap()
    .id
}

fn record(
    fx: &Fixture,
    stream_id: &str,
    thread_id: &str,
    action: ChangeAction,
) -> car::ChangeActionRequest {
    car::record_change_action_request(
        &fx.app,
        &store(fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        car::NewActionRequest {
            stream_id,
            change: CHANGE,
            action,
            instruction: Some("Drop the second task."),
            rationale: "Ready. <spoken>Ready.</spoken>",
            agent: &agent(),
            thread_id,
        },
    )
    .expect("recorded")
}

/// The refusal keeps the form the command rejects with (OFP-FR-QLQJ).
fn confirm(fx: &Fixture, id: &str) -> Result<car::ChangeActionOutcome, crate::openspec::Refusal> {
    car::confirm_action_request(
        &fx.app,
        &store(fx),
        &car::AppStarter { app: &fx.app },
        id,
        &crate::comments::Participant::local_human(),
    )
}

/// A pending change edit proposal of the change, in `thread_id`.
fn propose_edit(fx: &Fixture, stream_id: &str, thread_id: &str) -> String {
    crate::change_proposals::record_change_proposal(
        &fx.app,
        &store(fx),
        &crate::change_proposals::AppSites { app: &fx.app },
        crate::change_proposals::NewChangeProposal {
            stream_id,
            change: CHANGE,
            hunks: &[crate::change_proposals::PathedHunk {
                path: "tasks.md".into(),
                hunk: crate::text_hunks::hunks::ProposedHunk::from_fields(
                    Some("- [ ] Wire the call\n".into()),
                    Some("- [ ] Wire the logout call\n".into()),
                    None,
                    None,
                    None,
                    None,
                ),
            }],
            rationale: "Name the call.",
            agent: &agent(),
            thread_id,
        },
    )
    .expect("a pending proposal")
    .id
}

// CAR-FR-MMPV / CAR-FR-UPOV / OCR-FR-TWDK: a confirmed apply request enqueues
// an apply run, and the request names it.
#[test]
fn a_confirmed_apply_request_enqueues_an_apply_run() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let thread_id = open_thread(&fx, &stream.id, CHANGE);
    let request = record(&fx, &stream.id, &thread_id, ChangeAction::Apply);
    let outcome = confirm(&fx, &request.id).expect("started");
    let run = outcome.run.expect("the run");
    assert!(run.is_change_apply());
    assert_eq!(outcome.request.state, ChangeActionRequestState::Started);
    assert_eq!(outcome.request.run_id.as_deref(), Some(run.id.as_str()));
    assert_eq!(queue_len(&fx), 1);
}

// CAR-FR-MMPV / OCR-FR-TBGA / OCR-FR-TWDK: a confirmed rework request starts a
// rework run in the request's thread, with its agent as the requester, and
// opens no discussion.
#[test]
fn a_confirmed_rework_request_starts_in_its_thread_as_its_agent() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let thread_id = open_thread(&fx, &stream.id, CHANGE);
    let request = record(&fx, &stream.id, &thread_id, ChangeAction::Rework);
    let outcome = confirm(&fx, &request.id).expect("started");
    let data = outcome.run.as_ref().and_then(|run| run.openspec.clone()).expect("openspec data");
    assert_eq!(data.phase, crate::graduation::PHASE_REWORK);
    assert_eq!(data.thread_id.as_deref(), Some(thread_id.as_str()));
    assert_eq!(data.requester, Some(agent()));
    assert_eq!(data.instruction.as_deref(), Some("Drop the second task."));
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream.id.clone(),
        change: CHANGE.into(),
    };
    assert_eq!(crate::comments::list_discussions_in(&store(&fx), &target).len(), 1);
}

// CAR-FR-ARMK / OCR-FR-MGPU: a start the queue refuses keeps the request
// pending with the code, and enqueues nothing.
#[test]
fn a_refused_start_keeps_the_request_pending() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let running = start_apply(&fx, &stream).expect("a run of the change");
    let thread_id = open_thread(&fx, &stream.id, CHANGE);
    let request = record(&fx, &stream.id, &thread_id, ChangeAction::Apply);
    assert_eq!(
        confirm(&fx, &request.id).unwrap_err(),
        format!("openspec_run_active: {}", running.id)
    );
    let held = car::list_requests_impl(&store(&fx), &stream.id, CHANGE).remove(0);
    assert_eq!(held.state, ChangeActionRequestState::Pending);
    assert_eq!(held.last_refusal.as_deref(), Some("openspec_run_active"));
    assert_eq!(queue_len(&fx), 1);
}

// OFP-FR-QLQJ, OFP-FR-SJML, CAR-FR-ARMK: a confirmation whose apply start
// is refused with `change_not_ready` rejects with one `OpenSpecFailure`
// object, with the missing artifacts and the diagnostics, and keeps the
// request pending with the code.
#[test]
fn a_confirmation_rejects_an_openspec_failure_as_an_object() {
    let (fx, _, _) = change_fixture(BLOCKED);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let thread_id = open_thread(&fx, &stream.id, CHANGE);
    let request = record(&fx, &stream.id, &thread_id, ChangeAction::Apply);
    let refused = confirm(&fx, &request.id).unwrap_err();
    assert_object(&refused, "change_not_ready");
    let failure = refused.openspec().unwrap();
    assert_eq!(failure.detail.as_deref(), Some("design,tasks"));
    assert!(!failure.diagnostics.is_empty());
    let held = car::list_requests_impl(&store(&fx), &stream.id, CHANGE).remove(0);
    assert_eq!(held.state, ChangeActionRequestState::Pending);
    assert_eq!(held.last_refusal.as_deref(), Some("change_not_ready"));
    assert_eq!(queue_len(&fx), 0);
}

// OCR-FR-LNKQ / OCR-FR-GTOM: an apply start is refused with
// `proposal_pending` where the change holds a pending change edit proposal,
// or a closed one whose commit is still owed, and enqueues nothing.
#[test]
fn an_apply_start_is_refused_while_a_proposal_holds_files() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let thread_id = open_thread(&fx, &stream.id, CHANGE);
    let proposal_id = propose_edit(&fx, &stream.id, &thread_id);
    assert_eq!(start_apply(&fx, &stream).unwrap_err(), "proposal_pending");
    assert_eq!(queue_len(&fx), 0);

    // The same proposal, closed with its commit owed.
    let file = store(&fx)
        .path()
        .join(format!("changes/{}/{CHANGE}/proposals/{proposal_id}.toml", stream.id));
    let text = std::fs::read_to_string(&file).expect("the record");
    assert!(text.contains("state = \"pending\""), "{text}");
    let closed = text
        .replace("state = \"pending\"", "state = \"accepted\"")
        .replace("commitOwed = false", "commitOwed = true");
    std::fs::write(&file, closed).expect("write");
    let held = crate::change_proposals::list_proposals_impl(&store(&fx), &stream.id, CHANGE);
    assert!(
        held[0].commit_owed
            && held[0].state != crate::change_proposals::ChangeProposalState::Pending
    );
    assert_eq!(start_apply(&fx, &stream).unwrap_err(), "proposal_pending");
    assert_eq!(queue_len(&fx), 0);

    // Once the commit is made, the start is admitted.
    let done =
        std::fs::read_to_string(&file).unwrap().replace("commitOwed = true", "commitOwed = false");
    std::fs::write(&file, done).expect("write");
    start_apply(&fx, &stream).expect("admitted");
}

// OCR-FR-TWDK: a thread that is not a discussion of the change refuses a
// start in it with `discussion_not_found`, and enqueues nothing.
#[test]
fn a_rework_start_in_a_thread_of_another_change_is_refused() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    let elsewhere = open_thread(&fx, &stream.id, "other-change");
    for thread_id in [elsewhere.as_str(), "no-such-thread"] {
        let refused = crate::graduation::start_change_rework_in_thread(
            &fx.app,
            &stream.id,
            CHANGE,
            "Drop the second task.",
            thread_id,
            &agent(),
        );
        assert_eq!(refused.unwrap_err(), "discussion_not_found");
    }
    assert_eq!(queue_len(&fx), 0);
    // A resolved thread of the change is the other refusal of the thread.
    let thread_id = open_thread(&fx, &stream.id, CHANGE);
    set_resolved(&fx, &stream.id, &thread_id, true);
    let refused = crate::graduation::start_change_rework_in_thread(
        &fx.app,
        &stream.id,
        CHANGE,
        "Drop the second task.",
        &thread_id,
        &agent(),
    );
    assert_eq!(refused.unwrap_err(), "discussion_resolved");
}

// OCR-FR-SFDE: the suffix goes after the code and the detail of the failure,
// whatever its code.
#[test]
fn the_thread_suffix_keeps_the_code_and_detail_of_the_failure() {
    use crate::graduation::with_thread;
    assert_eq!(with_thread("write_failed", "t2"), "write_failed; thread: t2");
    assert_eq!(with_thread("no_project_open", "t2"), "no_project_open; thread: t2");
    assert_eq!(
        with_thread("openspec_run_active: run-7", "t2"),
        "openspec_run_active: run-7; thread: t2"
    );
}

// OCR-FR-SFDE: an enqueue that fails after the request thread opened leaves
// the thread, and the start returns the failure with its own code and detail,
// then `; thread: <thread-id>`.
#[test]
fn a_failed_enqueue_after_the_thread_opened_names_the_thread() {
    let (fx, _, _) = change_fixture(READY);
    fx.allow_execution();
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let stream = change_stream(&fx, "editor");
    // The run store cannot take a run: its folder of runs is a file.
    let runs = fx.store.path().join("g");
    let _ = std::fs::remove_dir_all(&runs);
    std::fs::write(&runs, "not a folder").expect("block the run store");

    let refused =
        super::change_runs::start_rework(&fx, &stream, "Drop the second task.").unwrap_err();
    let refused = refused.to_string();
    let (failure, thread_id) = refused.split_once("; thread: ").expect("the suffix");
    let (code, detail) = failure.split_once(": ").expect("a code and a detail");
    assert_eq!(code, "write_failed");
    assert!(!detail.trim().is_empty(), "the detail of the store: {failure}");
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream.id.clone(),
        change: CHANGE.into(),
    };
    let discussions = crate::comments::list_discussions_in(&store(&fx), &target);
    assert_eq!(discussions.len(), 1, "the discussion stays");
    assert_eq!(discussions[0].id, thread_id);
    assert_eq!(discussions[0].comments[0].body, "Drop the second task.");
}
