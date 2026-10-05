//! Two merge requests for one stream (`WKS-work-streams.md` WKS-FR-GKPX,
//! WKS-FR-HLGN, WKS-FR-VQDE).
//!
//! The second request must be refused `stream_busy` also where it passed its
//! first check of the stream's runs before the first request had made its run.
//! That is why the check is made again under the repository update guard.

use super::merge_support::*;
use super::*;

/// A merge run of the stream, made by hand in the state `queued`, as a first
/// request hands one off.
fn hand_off_with(
    app: &tauri::AppHandle<tauri::test::MockRuntime>,
    repo: &git2::Repository,
    stream: &WorkStream,
) -> crate::graduation::GraduationRun {
    let request = crate::graduation::MergeRunRequest {
        run_id: crate::graduation::new_run_id(),
        project_key: app.state::<crate::project::ProjectState>().slot_key(),
        stream_id: stream.id.clone(),
        stream_name: stream.name.clone(),
        stream_branch: stream.branch.clone(),
        base_branch: stream.base_branch.clone(),
        base_tip: tip_text(repo, &stream.base_branch),
        stream_tip: tip_text(repo, &stream.branch),
        merge_base: tip_text(repo, &stream.base_branch),
        snapshot_commit: tip_text(repo, &stream.branch),
        publication: StreamMergePublication::Uncommitted,
        changed_paths: vec!["added.txt".into()],
        unresolved_paths: Vec::new(),
        conflicts: Vec::new(),
    };
    crate::graduation::enqueue_merge_run(app, request).expect("a merge run")
}

fn runs_of_the_project(fx: &Fixture) -> usize {
    let fs = fx
        .app
        .state::<crate::fs::FsAccessState>()
        .get()
        .expect("instance");
    let base = crate::graduation::StoreBase::new(canonical(fx.store.path()));
    let key = fx.app.state::<crate::project::ProjectState>().slot_key();
    crate::graduation::read_queue_index(&fs, &base, &key).runs.len()
}

// WKS-FR-GKPX, WKS-FR-HLGN: a request that passed its first check of the
// stream's runs, and met a merge run another request handed off before it took
// the guard, is refused `stream_busy` by the check it makes under the guard. No
// second run is made, nothing is written to either branch or working copy, and
// the guard is free again.
#[test]
fn a_request_that_met_a_run_made_after_its_first_check_is_refused_under_the_guard() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "racing");
    let branches_before = observe(&fx, &stream);
    let base_tip_before = tip_text(&fx.repo(), &stream.base_branch);
    let stream_tip_before = tip_text(&fx.repo(), &stream.branch);
    let handed_off = std::rc::Rc::new(std::cell::RefCell::new(None));
    {
        let app = fx.app.clone();
        let stream = stream.clone();
        let repo_root = fx.root();
        let slot = handed_off.clone();
        merge_handoff::before_guard::set(move || {
            // The first request makes its run now, between the second request's
            // first check and its guard.
            let repo = git2::Repository::open(&repo_root).expect("repo");
            let run = hand_off_with(&app, &repo, &stream);
            *slot.borrow_mut() = Some(run.id);
        });
    }

    let refusal = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, ERR_STREAM_BUSY);
    let made = handed_off.borrow().clone();
    assert!(made.is_some(), "the seam ran, so the first request made its run");
    assert_eq!(runs_of_the_project(&fx), 1, "no second run was made");
    let after = observe(&fx, &stream);
    assert_eq!(after.base_files, branches_before.base_files);
    assert_eq!(after.stream_files, branches_before.stream_files);
    assert_eq!(after.base_status, branches_before.base_status);
    assert_eq!(tip_text(&fx.repo(), &stream.base_branch), base_tip_before);
    assert_eq!(tip_text(&fx.repo(), &stream.branch), stream_tip_before);
    assert!(
        !fx.app.state::<StreamState>().is_merging(&stream.id),
        "the refused request let go of the guard"
    );
    let streams = fx.app.state::<StreamState>();
    let hold = streams
        .begin_repository_update(&stream.id, Reconciliation::Merge)
        .expect("the guard is free");
    drop(hold);
}

// WKS-FR-GKPX, WKS-FR-HLGN: the same request, with no run made between its two
// checks, merges. The seam is not what refuses it.
#[test]
fn a_request_with_no_run_made_between_its_checks_is_not_refused() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "not racing");
    merge_handoff::before_guard::set(|| {});

    let result = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("merged");

    assert!(matches!(result, StreamMergeResult::Merged { .. }), "{result:?}");
}

// WKS-FR-GKPX, WKS-FR-VQDE: a second request that comes after the first request
// has made its run is refused `stream_busy` at its first check, and makes no run
// either. Only the sequential case is covered here; the race is the test above.
#[test]
fn a_second_request_after_the_run_exists_is_refused_and_makes_no_run() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "sequential");
    hand_off_with(&fx.app, &fx.repo(), &stream);
    let before = observe(&fx, &stream);

    for _ in 0..2 {
        let refusal =
            merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");
        assert_eq!(refusal, ERR_STREAM_BUSY);
    }

    assert_eq!(observe(&fx, &stream), before);
    assert_eq!(runs_of_the_project(&fx), 1);
}

// WKS-FR-GKPX, WKS-FR-VQDE, GRB-FR-UHFE: a merge run that has not ended refuses
// the next request to merge its stream with `stream_busy`, in every state a run
// can rest or work in. The refusal makes no second run and writes nothing, and
// the request is accepted again once the run has ended.
#[test]
fn a_merge_run_refuses_a_second_merge_in_every_state_that_has_not_ended() {
    use crate::graduation::GraduationRunState as State;
    for state in [
        State::Queued,
        State::Working,
        State::Reviewing,
        State::Blocked,
        State::AwaitingAuthor,
        State::Interrupted,
    ] {
        let fx = Fixture::new();
        let stream = stream_ahead(&fx, "every state");
        let mut run = hand_off_with(&fx.app, &fx.repo(), &stream);
        run.state = state;
        crate::graduation::save_run(&fx.app, &mut run).expect("saved");
        let before = observe(&fx, &stream);

        let refusal =
            merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");

        assert_eq!(refusal, ERR_STREAM_BUSY, "state {state:?}");
        assert_eq!(observe(&fx, &stream), before, "state {state:?}: the refusal wrote something");
        assert_eq!(runs_of_the_project(&fx), 1, "state {state:?}: a second run was made");

        run.state = State::Completed;
        crate::graduation::save_run(&fx.app, &mut run).expect("saved");
        let result = merge(&fx, &stream.id, StreamMergePublication::Uncommitted);
        assert!(
            matches!(result, Ok(StreamMergeResult::Merged { .. })),
            "a run that has ended does not hold the stream: {result:?}"
        );
    }
}
