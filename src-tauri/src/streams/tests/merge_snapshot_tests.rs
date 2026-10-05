//! The merge snapshot a conflict is handed off with, and the pinned-tip check
//! (`GRB-graduation-rebase.md` GRB-FR-DZQE, GRB-FR-RMQC, GRB-FR-FPYA,
//! GRB-FR-BXEJ).

use super::merge_support::*;
use super::*;

const RUN: &str = "gsnapshot1";

// GRB-FR-DZQE, GRB-FR-SRVN, GRB-FR-FPYA, GRB-FR-RMQC, GRB-FR-ITCJ: the snapshot commit has
// the pinned base tip and the pinned stream tip as its two parents, holds
// diff3 markers in every path Git could not merge and the clean merge in the
// rest, and is kept under the private ref of the run. The test runs with no
// container runtime, no agentic CLI, no credential and no network.
#[test]
fn the_snapshot_is_a_commit_of_both_tips_with_markers_where_git_stopped() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let repo = fx.repo();
    let base_tip = tip_of(&repo, &stream.base_branch);
    let stream_tip = tip_of(&repo, &stream.branch);

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    let commit = repo.find_commit(snapshot.commit).unwrap();
    assert_eq!(commit.parent_ids().collect::<Vec<_>>(), vec![base_tip, stream_tip]);

    // A path both sides changed: diff3 markers naming the roles of the sides.
    let readme = blob_text(&repo, snapshot.commit, "README.md").expect("README.md");
    for expected in [
        "<<<<<<< the base branch\nbase side\n",
        "||||||| what both started from\nseed\n",
        "=======\nstream side\n>>>>>>> the stream\n",
    ] {
        assert!(readme.contains(expected), "{readme}");
    }
    // A path both sides added: marked too.
    let both = blob_text(&repo, snapshot.commit, "both-new.txt").expect("both-new.txt");
    assert!(both.contains("<<<<<<< ") && both.contains("base new") && both.contains("stream new"));
    // A path the base deleted and the stream changed: a marked file, because
    // Git writes no marker for a deletion.
    let gone = blob_text(&repo, snapshot.commit, "gone.txt").expect("gone.txt");
    assert!(gone.contains("<<<<<<< the base branch"), "{gone}");
    assert!(gone.contains("(this path was deleted on this side)"), "{gone}");
    assert!(gone.contains("changed by the stream"), "{gone}");
    // The paths Git merged cleanly hold their merged content and no marker.
    assert_eq!(
        blob_text(&repo, snapshot.commit, "stream-only.txt").as_deref(),
        Some("kept\n")
    );
    assert_eq!(
        blob_text(&repo, snapshot.commit, "base-only.txt").as_deref(),
        Some("from the base\n")
    );

    // The private ref keeps the commit reachable.
    let reference = repo
        .find_reference(&merge_snapshot::snapshot_ref(RUN))
        .expect("the private ref");
    assert_eq!(reference.name().ok(), Some("refs/synthesis/merge/gsnapshot1"));
    assert_eq!(reference.target(), Some(snapshot.commit));
}

// GRB-FR-FPYA, GRB-FR-DZQE, GRB-FR-YDTX: the snapshot names the merge base, every
// path the merge changes against the base tip (Git-clean paths included), the
// unresolved paths, and what each side did to each of them.
#[test]
fn the_snapshot_names_the_merge_base_the_changed_paths_and_the_conflicts() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let repo = fx.repo();
    let expected_base = repo
        .merge_base(
            tip_of(&repo, &stream.base_branch),
            tip_of(&repo, &stream.branch),
        )
        .unwrap();

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    assert_eq!(snapshot.merge_base, expected_base);
    assert_eq!(
        snapshot.unresolved_paths,
        vec!["README.md", "both-new.txt", "gone.txt"]
    );
    assert_eq!(
        snapshot.changed_paths,
        vec!["README.md", "both-new.txt", "gone.txt", "stream-only.txt"],
        "the Git-clean path is a changed path too, and a base-only change is not"
    );
    let sides: Vec<(&str, &str, &str)> = snapshot
        .conflicts
        .iter()
        .map(|c| (c.path.as_str(), c.base_change.as_str(), c.stream_change.as_str()))
        .collect();
    assert_eq!(
        sides,
        vec![
            ("README.md", "updated", "updated"),
            ("both-new.txt", "created", "created"),
            ("gone.txt", "deleted", "updated"),
        ]
    );
}

// GRB-FR-UHFE, GRB-FR-RMQC, WKS-FR-RAOM: capturing a snapshot moves no branch,
// writes neither working copy, registers no worktree, and adds the private ref
// and nothing else.
#[test]
fn capturing_a_snapshot_writes_no_branch_and_no_working_copy() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let before = observe(&fx, &stream);

    capture_snapshot(&fx, &stream, RUN);

    let after = observe(&fx, &stream);
    assert_eq!(after.base_files, before.base_files);
    assert_eq!(after.stream_files, before.stream_files);
    assert_eq!(after.base_status, before.base_status);
    assert_eq!(after.stream_status, before.stream_status);
    assert_eq!(after.worktrees, before.worktrees);
    assert_eq!(after.store_files, before.store_files);
    let mut expected = before.refs.clone();
    expected.insert(
        "refs/synthesis/merge/gsnapshot1".to_string(),
        after.refs["refs/synthesis/merge/gsnapshot1"].clone(),
    );
    assert_eq!(after.refs, expected, "the private ref is the only new ref");
}

// GRB-FR-RMQC: the private ref is removed when the run ends, and removing one
// that is not there is not an error.
#[test]
fn the_private_ref_can_be_released_and_released_again() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    capture_snapshot(&fx, &stream, RUN);
    let repo = fx.repo();
    assert!(merge_snapshot::has_ref(&repo, RUN));

    merge_snapshot::release_ref(&repo, RUN).expect("released");
    assert!(!merge_snapshot::has_ref(&repo, RUN));
    merge_snapshot::release_ref(&repo, RUN).expect("released again");
}

// GRB-FR-DZQE: a path either side holds as a binary file cannot be marked, so
// the snapshot keeps the base side's bytes and the path stays unresolved.
#[test]
fn a_binary_conflict_keeps_the_base_content_and_stays_unresolved() {
    let fx = Fixture::new();
    std::fs::write(fx.root().join("image.bin"), [0u8, 1, 2, 3]).unwrap();
    commit_all(&fx.repo(), "add the binary");
    let stream = fx.create("binary", None).expect("created");
    std::fs::write(Path::new(&stream.worktree_path).join("image.bin"), [0u8, 9, 9, 9]).unwrap();
    commit_all(&stream_repo(&stream), "stream binary");
    std::fs::write(fx.root().join("image.bin"), [0u8, 7, 7, 7]).unwrap();
    commit_all(&fx.repo(), "base binary");

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    assert_eq!(snapshot.unresolved_paths, vec!["image.bin"]);
    let repo = fx.repo();
    let tree = repo.find_commit(snapshot.commit).unwrap().tree().unwrap();
    let blob = repo
        .find_blob(tree.get_path(Path::new("image.bin")).unwrap().id())
        .unwrap();
    assert_eq!(blob.content(), &[0u8, 7, 7, 7]);
}

// GRB-FR-BXEJ: a merge run's pinned tips are checked against the branches: both
// standing is not moved, and either one moved, either branch gone or a pinned id
// that cannot be read is moved.
#[test]
fn the_pinned_tips_are_compared_with_both_branches() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let snapshot = capture_snapshot(&fx, &stream, RUN);
    let data = merge_data_for(&fx, &stream, &snapshot, StreamMergePublication::Uncommitted);
    let before = observe(&fx, &stream);

    assert!(!merge_tips_moved(&fx.app, &data), "both tips stand");
    assert_eq!(observe(&fx, &stream), before, "the check is read-only");

    let mut unreadable = data.clone();
    unreadable.stream_tip = "not an object id".to_string();
    assert!(merge_tips_moved(&fx.app, &unreadable));
    let mut other_branch = data.clone();
    other_branch.base_branch = "no-such-branch".to_string();
    assert!(merge_tips_moved(&fx.app, &other_branch));

    commit_file(&stream_repo(&stream), "later.txt", "later\n", "stream moves on");
    assert!(merge_tips_moved(&fx.app, &data), "the stream tip moved");
}

// GRB-FR-BXEJ: the base tip moving is a moved tip as well.
#[test]
fn a_base_branch_that_advanced_is_a_moved_tip() {
    let fx = Fixture::new();
    let stream = stream_with_three_conflicts(&fx);
    let snapshot = capture_snapshot(&fx, &stream, RUN);
    let data = merge_data_for(&fx, &stream, &snapshot, StreamMergePublication::Uncommitted);

    commit_file(&fx.repo(), "later.txt", "later\n", "base moves on");

    assert!(merge_tips_moved(&fx.app, &data));
}
