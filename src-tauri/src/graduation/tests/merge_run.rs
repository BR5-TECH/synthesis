//! A conflict merge as a graduation run (`GRD-graduation.md` GRD-FR-MRNQ and the
//! requirements that follow it).

use super::*;
use crate::streams::{StreamMergePublication, StreamMergeResult};

/// Write one file and commit everything in a repository's working copy.
fn commit_file(repo: &git2::Repository, path: &str, content: &str, message: &str) -> git2::Oid {
    let workdir = repo.workdir().expect("a working copy").to_path_buf();
    std::fs::write(workdir.join(path), content).unwrap();
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("add");
    index.write().expect("write");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("tree");
    let signature = repo.signature().expect("signature");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&head])
        .expect("commit")
}

/// A stream whose branch and whose base branch both changed `README.md`.
fn conflicted_stream(fx: &Fixture) -> crate::streams::WorkStream {
    let stream = fx.stream("feature");
    let stream_repo = git2::Repository::open(stream.worktree()).expect("the stream's repo");
    commit_file(&stream_repo, "README.md", "stream side\n", "stream edit");
    commit_file(&stream_repo, "stream-only.txt", "kept\n", "stream file");
    commit_file(&fx.repo(), "README.md", "base side\n", "base edit");
    stream
}

fn hand_off(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    publication: StreamMergePublication,
) -> String {
    // The queue is not offered a start here: a test that drives the run drives it
    // with the agent scripted.
    fx.app.state::<GraduationState>().set_loop_enabled(false);
    let result = crate::streams::merge_work_stream_blocking(&fx.app, &stream.id, publication)
        .expect("the check settled");
    fx.app.state::<GraduationState>().set_loop_enabled(true);
    match result {
        StreamMergeResult::Conflicted { run_id, .. } => run_id,
        other => panic!("expected a handoff, got {other:?}"),
    }
}

// GRD-FR-MRNQ, GRD-FR-VCTH, GRD-FR-BHCS: a merge Git cannot settle becomes a
// merge run in state `queued` that carries merge data and no draft, and the
// handoff writes neither branch nor the base working copy.
#[test]
fn a_conflict_merge_becomes_a_queued_run_and_writes_nothing() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let base_before = fx.repo().head().unwrap().peel_to_commit().unwrap().id();

    let run_id = hand_off(&fx, &stream, StreamMergePublication::Uncommitted);
    let run = fx.reload(&run_id);

    assert_eq!(run.state, GraduationRunState::Queued);
    let data = run.merge.as_ref().expect("merge data");
    assert_eq!(data.name, "Merge feature");
    assert_eq!(data.unresolved_paths, vec!["README.md".to_string()]);
    assert!(data.changed_paths.contains(&"stream-only.txt".to_string()));
    assert_eq!(run.input.draft_id, "");
    let repo = fx.repo();
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base_before);
    assert_eq!(
        std::fs::read_to_string(fx.root().join("README.md")).unwrap(),
        "base side\n"
    );
}

// GRD-FR-AQNW, GXD-FR-XQJR, GXD-FR-KXXB: a `ready` merge review applies the merge
// as one two-parent commit, and the seam saw the two merge turn kinds.
#[test]
fn a_ready_merge_review_applies_the_merge_as_one_commit() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = conflicted_stream(&fx);
    let run_id = hand_off(
        &fx,
        &stream,
        StreamMergePublication::Commit {
            message: "Merge feature".to_string(),
        },
    );
    let run = fx.reload(&run_id);
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("README.md", "both sides\n"),
        Turn::ready(),
    ]);
    let done = fx.drive(&run, dispatch.clone());

    assert_eq!(done.state, GraduationRunState::Completed, "{done:?}");
    let seen = dispatch.seen();
    assert_eq!(seen[0].part, "merge_work");
    assert_eq!(seen[1].part, "merge_review");
    assert_eq!(seen[0].turn_kind, TurnKind::MergeWork);
    assert_eq!(seen[1].turn_kind, TurnKind::MergeReview);
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 2);
    assert_eq!(head.message().unwrap(), "Merge feature");
    assert_eq!(
        std::fs::read_to_string(fx.root().join("README.md")).unwrap(),
        "both sides\n"
    );
    assert_eq!(done.commits.len(), 1);
    assert!(done.merge.as_ref().unwrap().result.is_some());
}
