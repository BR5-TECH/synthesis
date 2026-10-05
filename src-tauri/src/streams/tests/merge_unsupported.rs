//! The conflicts a text turn cannot reconcile are refused at the check
//! (`WKS-work-streams.md` WKS-FR-UCMR, `GRB-graduation-rebase.md` GRB-FR-DZQE).
//!
//! The streams fixture holds no vendor, so the image preflight would answer
//! `vendor_execution_unsupported` for any conflict that reached it. Each test
//! here therefore also proves the order of the checks: the refusal of an
//! unsupported conflict comes before the preflight.

use super::merge_support::*;
use super::*;

const RUN: &str = "gunsupported1";

fn tip_ids(fx: &Fixture, stream: &WorkStream) -> (git2::Oid, git2::Oid) {
    let repo = fx.repo();
    (
        tip_of(&repo, &stream.base_branch),
        tip_of(&repo, &stream.branch),
    )
}

/// The request is refused as an unsupported conflict, and nothing was written:
/// no ref (the private snapshot ref included), no run, no worktree, no file,
/// and both branch tips and both working copies are as they were.
fn assert_unsupported_and_untouched(
    fx: &Fixture,
    stream: &WorkStream,
    publication: StreamMergePublication,
) {
    let before = observe(fx, stream);
    let tips = tip_ids(fx, stream);

    let refusal = merge(fx, &stream.id, publication).expect_err("refused");

    assert_eq!(refusal, ERR_UNSUPPORTED_CONFLICT);
    assert_eq!(observe(fx, stream), before, "the refusal wrote something");
    assert_eq!(tip_ids(fx, stream), tips, "a branch tip moved");
    assert!(snapshot_refs(&fx.repo()).is_empty(), "a snapshot ref stands");
    assert_no_run_anywhere(fx);
}

fn both_publications() -> [StreamMergePublication; 2] {
    [
        StreamMergePublication::Uncommitted,
        StreamMergePublication::Commit {
            message: "never lands".into(),
        },
    ]
}

/// The error text of a snapshot capture that was refused, with the tips as they
/// stand.
fn capture_error(fx: &Fixture, stream: &WorkStream) -> String {
    let repo = fx.repo();
    match merge_snapshot::capture(
        &repo,
        tip_of(&repo, &stream.base_branch),
        tip_of(&repo, &stream.branch),
        RUN,
    ) {
        Ok(_) => panic!("the capture was accepted"),
        Err(reason) => reason,
    }
}

// WKS-FR-UCMR, WKS-FR-RAOM: both sides pointed the same symbolic link at
// different targets. There is no text to mark, so the request answers
// `unsupported_conflict` on either publication, before the image preflight, and
// writes nothing.
#[cfg(unix)]
#[test]
fn a_conflict_over_a_symbolic_link_is_refused_before_the_preflight() {
    for publication in both_publications() {
        let fx = Fixture::new();
        commit_symlink(&fx.repo(), "link", "seed-target", "seed link");
        let stream = fx.create("symlink", None).expect("created");
        commit_symlink(&stream_repo(&stream), "link", "stream-target", "stream link");
        commit_symlink(&fx.repo(), "link", "base-target", "base link");

        assert_unsupported_and_untouched(&fx, &stream, publication);
    }
}

// WKS-FR-UCMR: a path that is a plain file on one side and a symbolic link on
// the other is a conflict that involves a symbolic link, in both directions.
#[cfg(unix)]
#[test]
fn a_file_and_a_symbolic_link_in_conflict_are_refused() {
    // The stream turns the file into a link, and the base edits the file.
    let fx = Fixture::new();
    commit_file(&fx.repo(), "item", "seed\n", "seed item");
    let stream = fx.create("file to link", None).expect("created");
    commit_symlink(&stream_repo(&stream), "item", "elsewhere", "stream link");
    commit_file(&fx.repo(), "item", "base edit\n", "base edit");
    assert_unsupported_and_untouched(&fx, &stream, StreamMergePublication::Uncommitted);

    // The base turns the file into a link, and the stream edits the file.
    let fx = Fixture::new();
    commit_file(&fx.repo(), "item", "seed\n", "seed item");
    let stream = fx.create("link to file", None).expect("created");
    commit_file(&stream_repo(&stream), "item", "stream edit\n", "stream edit");
    commit_symlink(&fx.repo(), "item", "elsewhere", "base link");
    assert_unsupported_and_untouched(&fx, &stream, StreamMergePublication::Uncommitted);
}

// WKS-FR-UCMR: a symbolic link in conflict is refused even where an ordinary text
// conflict stands beside it, so the text conflict does not carry the merge into a
// run.
#[cfg(unix)]
#[test]
fn an_unsupported_conflict_beside_a_text_conflict_still_refuses_the_whole_request() {
    let fx = Fixture::new();
    commit_symlink(&fx.repo(), "link", "seed-target", "seed link");
    let stream = fx.create("mixed", None).expect("created");
    commit_symlink(&stream_repo(&stream), "link", "stream-target", "stream link");
    commit_file(&stream_repo(&stream), "README.md", "stream side\n", "stream readme");
    commit_symlink(&fx.repo(), "link", "base-target", "base link");
    commit_file(&fx.repo(), "README.md", "base side\n", "base readme");

    assert_unsupported_and_untouched(&fx, &stream, StreamMergePublication::Uncommitted);
}

// WKS-FR-UCMR: both sides moved a submodule to a different revision. A submodule
// holds no text either, so the request answers `unsupported_conflict`.
#[test]
fn a_conflict_over_a_submodule_is_refused_before_the_preflight() {
    for publication in both_publications() {
        let fx = Fixture::new();
        // Three revisions the repository holds, one for each side of the path.
        let seed = tip_of(&fx.repo(), &current_branch(&fx));
        commit_file(&fx.repo(), "one.txt", "1\n", "second revision");
        let second = tip_of(&fx.repo(), &current_branch(&fx));
        commit_file(&fx.repo(), "two.txt", "2\n", "third revision");
        let third = tip_of(&fx.repo(), &current_branch(&fx));
        commit_gitlink(&fx.repo(), "vendor/lib", seed, "seed submodule");
        let stream = fx.create("submodule", None).expect("created");
        commit_gitlink(&stream_repo(&stream), "vendor/lib", second, "stream moves it");
        commit_gitlink(&fx.repo(), "vendor/lib", third, "base moves it");

        assert_unsupported_and_untouched(&fx, &stream, publication);
    }
}

fn current_branch(fx: &Fixture) -> String {
    fx.repo()
        .head()
        .unwrap()
        .shorthand()
        .expect("a branch name")
        .to_string()
}

// WKS-FR-UCMR: the stream adds a file where the base holds a directory. One tree
// cannot hold both, so the request answers `unsupported_conflict` and drops no
// path of the directory.
#[test]
fn a_file_the_stream_adds_where_the_base_has_a_directory_is_refused() {
    for publication in both_publications() {
        let fx = Fixture::new();
        let stream = fx.create("file over directory", None).expect("created");
        commit_file(&stream_repo(&stream), "d", "a file\n", "stream adds the file");
        commit_file(&fx.repo(), "d/x", "inside\n", "base adds the directory");

        assert_unsupported_and_untouched(&fx, &stream, publication);
    }
}

// WKS-FR-UCMR: the reverse direction. The stream adds a directory where the base
// holds a file.
#[test]
fn a_directory_the_stream_adds_where_the_base_has_a_file_is_refused() {
    for publication in both_publications() {
        let fx = Fixture::new();
        let stream = fx.create("directory over file", None).expect("created");
        commit_file(&stream_repo(&stream), "d/x", "inside\n", "stream adds the directory");
        commit_file(&fx.repo(), "d", "a file\n", "base adds the file");

        assert_unsupported_and_untouched(&fx, &stream, publication);
    }
}

// WKS-FR-UCMR: a path both sides started as a file, which one side replaced with
// a directory and the other edited, is a file against a directory as well.
#[test]
fn a_file_replaced_by_a_directory_on_one_side_and_edited_on_the_other_is_refused() {
    let fx = Fixture::new();
    commit_file(&fx.repo(), "d", "seed\n", "seed file");
    let stream = fx.create("replaced", None).expect("created");
    delete_file(&stream_repo(&stream), "d", "stream removes the file");
    commit_file(&stream_repo(&stream), "d/x", "inside\n", "stream adds the directory");
    commit_file(&fx.repo(), "d", "base edit\n", "base edits the file");

    assert_unsupported_and_untouched(&fx, &stream, StreamMergePublication::Uncommitted);
}

// WKS-FR-UCMR, GRB-FR-RMQC: a capture that is refused leaves no private ref
// behind, so no run is ever made for the snapshot and nothing is left to
// reclaim.
#[test]
fn a_capture_that_is_refused_leaves_no_snapshot_ref() {
    let fx = Fixture::new();
    let stream = fx.create("refused capture", None).expect("created");
    commit_file(&stream_repo(&stream), "d", "a file\n", "stream adds the file");
    commit_file(&fx.repo(), "d/x", "inside\n", "base adds the directory");
    let before = observe(&fx, &stream);

    let reason = capture_error(&fx, &stream);

    assert_eq!(reason, ERR_UNSUPPORTED_CONFLICT);
    assert!(!merge_snapshot::has_ref(&fx.repo(), RUN), "a ref stands for a refused capture");
    assert_eq!(observe(&fx, &stream), before, "the capture wrote something");
}

// WKS-FR-UCMR: the private ref of a snapshot stands only once the capture has
// succeeded, never before it.
#[test]
fn the_snapshot_ref_stands_only_after_a_capture_that_succeeded() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    assert!(!merge_snapshot::has_ref(&fx.repo(), RUN), "no ref before the capture");

    capture_snapshot(&fx, &stream, RUN);

    assert!(merge_snapshot::has_ref(&fx.repo(), RUN), "the ref stands after it");
}

// GRB-FR-QIHE, WKS-FR-UCMR: a conflict merge with an `uncommitted` publication
// and no worktree on the base branch answers `base_not_checked_out`, which is
// asked before the image preflight and before any snapshot is built.
#[test]
fn a_missing_base_worktree_is_refused_before_the_preflight_and_before_a_snapshot() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let repo = fx.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("elsewhere", &head, false).unwrap();
    repo.set_head("refs/heads/elsewhere").unwrap();
    let before = observe(&fx, &stream);

    let refusal = merge(&fx, &stream.id, StreamMergePublication::Uncommitted).expect_err("refused");

    assert_eq!(refusal, ERR_BASE_NOT_CHECKED_OUT);
    assert_eq!(observe(&fx, &stream), before);
    assert!(snapshot_refs(&fx.repo()).is_empty());
    assert_no_run_anywhere(&fx);
}
