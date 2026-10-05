//! The refusals of the pre-conflict check. Each one answers with its typed
//! reason and writes nothing (`WKS-work-streams.md` WKS-FR-GKPX,
//! `GRB-graduation-rebase.md` GRB-FR-QIHE).

use super::merge_support::*;
use super::*;

/// Assert a refusal that wrote nothing.
fn assert_refused_and_unchanged(
    fx: &Fixture,
    stream: &WorkStream,
    publication: StreamMergePublication,
    expected: &str,
) {
    let before = observe(fx, stream);
    let refusal = merge(fx, &stream.id, publication).expect_err("refused");
    assert!(refusal.starts_with(expected), "{refusal}");
    assert_eq!(observe(fx, stream), before, "the refusal wrote something");
}

// WKS-FR-GKPX, GRB-FR-UHFE: a stream that holds a non-terminal run is refused
// with `stream_busy`, whatever state the run rests in, a merge run included.
#[test]
fn a_stream_holding_a_run_that_has_not_ended_is_refused_as_busy() {
    use crate::graduation::GraduationRunState as State;
    for state in [
        State::Queued,
        State::Working,
        State::Reviewing,
        State::AwaitingAuthor,
        State::Blocked,
        State::Interrupted,
    ] {
        let fx = Fixture::new();
        let stream = stream_ahead(&fx, "busy");
        save_plain_run(&fx, &stream, state);
        assert_refused_and_unchanged(
            &fx,
            &stream,
            StreamMergePublication::Uncommitted,
            ERR_STREAM_BUSY,
        );
    }
}

// WKS-FR-GKPX, WKS-FR-VQDE: a merge run that has not ended refuses the next
// request to merge the stream, and one that has ended does not.
#[test]
fn a_live_merge_run_refuses_a_second_merge_and_an_ended_run_does_not() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "second");
    let request = crate::graduation::MergeRunRequest {
        run_id: crate::graduation::new_run_id(),
        project_key: fx.app.state::<crate::project::ProjectState>().slot_key(),
        stream_id: stream.id.clone(),
        stream_name: stream.name.clone(),
        stream_branch: stream.branch.clone(),
        base_branch: stream.base_branch.clone(),
        base_tip: tip_text(&fx.repo(), &stream.base_branch),
        stream_tip: tip_text(&fx.repo(), &stream.branch),
        merge_base: tip_text(&fx.repo(), &stream.base_branch),
        snapshot_commit: tip_text(&fx.repo(), &stream.branch),
        publication: StreamMergePublication::Uncommitted,
        changed_paths: vec!["added.txt".into()],
        unresolved_paths: Vec::new(),
        conflicts: Vec::new(),
    };
    let mut run = crate::graduation::enqueue_merge_run(&fx.app, request).expect("a merge run");

    assert_refused_and_unchanged(
        &fx,
        &stream,
        StreamMergePublication::Uncommitted,
        ERR_STREAM_BUSY,
    );

    run.state = crate::graduation::GraduationRunState::Discarded;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    let result = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("merged");
    assert!(matches!(result, StreamMergeResult::Merged { .. }), "{result:?}");
}

// WKS-FR-GKPX: a run that has ended does not hold the stream.
#[test]
fn a_run_that_has_ended_does_not_make_the_stream_busy() {
    use crate::graduation::GraduationRunState as State;
    for state in [State::Completed, State::Discarded, State::Failed] {
        let fx = Fixture::new();
        let stream = stream_ahead(&fx, "ended");
        save_plain_run(&fx, &stream, state);
        let result = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("merged");
        assert!(matches!(result, StreamMergeResult::Merged { .. }), "{result:?}");
    }
}

// WKS-FR-GKPX: a stream the busy mark names is refused, and writes nothing.
#[test]
fn a_stream_a_run_holds_is_refused_as_busy() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "held");
    claim_stream(&fx.app, &stream.id, "g1").expect("claimed");
    assert_refused_and_unchanged(
        &fx,
        &stream,
        StreamMergePublication::Uncommitted,
        ERR_STREAM_BUSY,
    );
}

// GRB-FR-QIHE, WKS-FR-UZHT: a stream with uncommitted work is refused with
// `stream_dirty`, carrying every path, and nothing is committed, stashed or
// reset on the author's behalf.
#[test]
fn a_dirty_stream_is_refused_with_every_dirty_path() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "dirty stream");
    let root = PathBuf::from(&stream.worktree_path);
    std::fs::write(root.join("one.txt"), "1\n").unwrap();
    std::fs::write(root.join("added.txt"), "edited\n").unwrap();
    std::fs::create_dir_all(root.join("nested")).unwrap();
    std::fs::write(root.join("nested/two.txt"), "2\n").unwrap();
    let before = observe(&fx, &stream);

    let refusal = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");

    assert!(refusal.starts_with(ERR_STREAM_DIRTY), "{refusal}");
    for path in ["one.txt", "added.txt", "nested/two.txt"] {
        assert!(refusal.contains(path), "{path} is named in {refusal}");
    }
    assert_eq!(observe(&fx, &stream), before);
    assert_eq!(std::fs::read_to_string(root.join("added.txt")).unwrap(), "edited\n");
}

// GRB-FR-QIHE, WKS-FR-UZHT: a base worktree with uncommitted work is refused
// with `base_dirty` carrying the complete path set, on both publications.
#[test]
fn a_dirty_base_worktree_is_refused_with_every_dirty_path() {
    for publication in [
        StreamMergePublication::Uncommitted,
        StreamMergePublication::Commit {
            message: "never lands".into(),
        },
    ] {
        let fx = Fixture::new();
        let stream = stream_ahead(&fx, "dirty base");
        std::fs::write(fx.root().join("mine.txt"), "mine\n").unwrap();
        std::fs::write(fx.root().join("README.md"), "edited seed\n").unwrap();
        std::fs::create_dir_all(fx.root().join("docs")).unwrap();
        std::fs::write(fx.root().join("docs/note.md"), "note\n").unwrap();
        let before = observe(&fx, &stream);

        let refusal = merge(&fx, &stream.id, publication).expect_err("refused");

        assert!(refusal.starts_with(ERR_BASE_DIRTY), "{refusal}");
        for path in ["mine.txt", "README.md", "docs/note.md"] {
            assert!(refusal.contains(path), "{path} is named in {refusal}");
        }
        assert_eq!(observe(&fx, &stream), before);
    }
}

// GRB-FR-QIHE, WKS-FR-ETKW: a stream whose working copy is gone is refused with
// `stream_missing`, and the base branch does not move.
#[test]
fn a_stream_whose_working_copy_is_gone_is_refused_as_missing() {
    let fx = Fixture::new();
    let stream = stream_ahead(&fx, "vanished");
    std::fs::remove_dir_all(&stream.worktree_path).unwrap();
    let base_before = tip_text(&fx.repo(), &stream.base_branch);
    let files_before = observe(&fx, &stream).base_files;

    let refusal = merge(
        &fx,
        &stream.id,
        StreamMergePublication::Commit {
            message: "never lands".into(),
        },
    )
    .expect_err("refused");

    assert_eq!(refusal, ERR_STREAM_MISSING);
    assert_eq!(tip_text(&fx.repo(), &stream.base_branch), base_before);
    assert_eq!(observe(&fx, &stream).base_files, files_before);
    assert_no_run_anywhere(&fx);
}

// WKS-FR-GKPX: a stream the project does not hold is refused as unknown, and
// nothing is written.
#[test]
fn an_unknown_stream_is_refused() {
    let fx = Fixture::new();
    let stream = fx.create("known", None).expect("created");
    let before = observe(&fx, &stream);

    let refusal =
        merge(&fx, "w-no-such-stream", StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, ERR_UNKNOWN_STREAM);
    assert_eq!(observe(&fx, &stream), before);
}

// WKS-FR-ZBHL, WKS-FR-GKPX: a project outside Git is told that before the
// stream is looked up, and nothing is written.
#[test]
fn a_project_outside_git_is_refused_before_the_stream_is_read() {
    let project = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let root = canonical(project.path());
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    handle.manage(crate::fs::FsAccessState::default());
    handle.manage(crate::project::ProjectState::default());
    handle.manage(StreamState::rooted_at(canonical(store.path())));
    handle
        .state::<crate::fs::FsAccessState>()
        .install_for_worktree_and(&root, &canonical(store.path()))
        .expect("two real directories");
    let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
    let state = handle.state::<crate::project::ProjectState>();
    state.set_root_with_access(root.clone(), Some(access));
    state.set_anchor(root.to_string_lossy().into_owned());

    let refusal = merge_work_stream_blocking(&handle, "w1", StreamMergePublication::Uncommitted)
        .expect_err("refused");

    assert_eq!(refusal, ERR_NOT_A_GIT_REPOSITORY);
    assert!(names_under(&canonical(store.path())).is_empty());
}

// WKS-FR-TSOA, GRB-FR-JIRD, WKS-FR-HLGN: while the guard is held a request is
// refused with the reason of whichever operation holds it, and changes nothing.
#[test]
fn a_request_made_while_the_guard_is_held_is_refused_and_changes_nothing() {
    for (holder, reason) in [
        (Reconciliation::Merge, ERR_MERGE_IN_PROGRESS),
        (Reconciliation::Update, ERR_UPDATE_IN_PROGRESS),
    ] {
        let fx = Fixture::new();
        let stream = stream_ahead(&fx, "guarded");
        let other = fx.create("another stream", None).expect("created");
        let before = observe(&fx, &stream);
        let state = fx.app.state::<StreamState>();
        let hold = state
            .begin_repository_update(&other.id, holder)
            .expect("the guard is free");

        let refusal =
            merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");
        assert_eq!(refusal, reason);
        assert_eq!(observe(&fx, &stream), before);
        drop(hold);

        let result = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect("merged");
        assert!(matches!(result, StreamMergeResult::Merged { .. }), "{result:?}");
    }
}

// GRB-FR-JIRD, WKS-FR-TSOA: an update request is refused with
// `merge_in_progress` while a merge check holds the guard.
#[test]
fn an_update_is_refused_while_a_merge_check_holds_the_guard() {
    let fx = Fixture::new();
    let stream = stream_behind_its_base(&fx);
    let state = fx.app.state::<StreamState>();
    let hold = state
        .begin_repository_update(&stream.id, Reconciliation::Merge)
        .expect("the guard is free");

    let refusal = state
        .begin_repository_update(&stream.id, Reconciliation::Update)
        .map(|_| ())
        .expect_err("refused");

    assert_eq!(refusal, ERR_MERGE_IN_PROGRESS);
    drop(hold);
}

// WKS-FR-BPGM, GRB-FR-MSNP, GRB-FR-UHFE: a conflict on a machine that cannot
// execute an agent is refused at the image preflight, before anything is built:
// no run, no snapshot ref, no worktree, and no branch or working copy moved.
#[test]
fn a_conflict_refused_at_the_preflight_builds_nothing() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let before = observe(&fx, &stream);

    let refusal = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, crate::graduation::ERR_VENDOR_EXECUTION_UNSUPPORTED);
    assert_eq!(observe(&fx, &stream), before);
    assert!(
        !before.refs.keys().any(|name| name.starts_with("refs/synthesis/merge/")),
        "no snapshot ref stands"
    );
    assert_no_run_anywhere(&fx);
}

// WKS-FR-UZHT, GRB-FR-QIHE, GRB-FR-MSNP: a conflict merge whose `uncommitted`
// publication has no worktree to land in is refused at the check, before the
// image preflight, and builds no snapshot and no run.
#[test]
fn a_conflict_merge_with_no_worktree_on_the_base_branch_is_refused_at_the_check() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();

    let refusal =
        merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, ERR_BASE_NOT_CHECKED_OUT);
    assert_no_run_anywhere(&fx);
}

const DRAFT_ID: &str = "1a2b3c4d5e6-0001-deadbeef";

/// The base worktree holds a committed draft that now differs from `HEAD`, and
/// the repository is in a state that refuses the save commit.
fn make_the_draft_save_fail(fx: &Fixture) {
    std::fs::write(
        fx.root().join(format!(".synthesis/drafts/UI/{DRAFT_ID}/files/P.md")),
        "edited, never saved\n",
    )
    .unwrap();
    std::fs::write(fx.repo().path().join("MERGE_HEAD"), "0".repeat(40)).expect("a merge");
}

fn commit_a_draft(fx: &Fixture) {
    let repo = fx.repo();
    commit_file(
        &repo,
        &format!(".synthesis/drafts/UI/{DRAFT_ID}/draft.toml"),
        &format!(
            "id = \"{DRAFT_ID}\"\nname = \"Push button\"\nstatus = \"active\"\n\
             promptPath = \"P.md\"\ncreatedAt = \"2026-01-01T00:00:00Z\"\n\
             updatedAt = \"2026-01-01T00:00:00Z\"\n"
        ),
        "seed draft",
    );
    commit_file(
        &repo,
        &format!(".synthesis/drafts/UI/{DRAFT_ID}/files/P.md"),
        "committed\n",
        "seed draft file",
    );
}

// PST-FR-RONA, GRB-FR-QIHE, WKS-FR-GKPX: a base worktree whose changed draft
// cannot be saved is refused with the typed `draft_save_failed`, on either
// publication and for a clean merge and a conflict merge alike, and nothing is
// written: no ref, no run, no file.
#[test]
fn a_draft_that_cannot_be_saved_refuses_the_request_and_writes_nothing() {
    for conflicting in [false, true] {
        for publication in [
            StreamMergePublication::Uncommitted,
            StreamMergePublication::Commit {
                message: "never lands".into(),
            },
        ] {
            let fx = Fixture::new();
            commit_a_draft(&fx);
            let stream = if conflicting {
                stream_with_three_conflicts(&fx)
            } else {
                stream_ahead(&fx, "draft save")
            };
            make_the_draft_save_fail(&fx);
            let before = observe(&fx, &stream);
            let base_tip = tip_text(&fx.repo(), &stream.base_branch);

            let refusal = merge(&fx, &stream.id, publication).expect_err("refused");

            assert_eq!(refusal, "draft_save_failed", "conflicting: {conflicting}");
            assert_eq!(observe(&fx, &stream), before, "the refusal wrote something");
            assert_eq!(tip_text(&fx.repo(), &stream.base_branch), base_tip);
            assert_no_run_anywhere(&fx);
        }
    }
}
