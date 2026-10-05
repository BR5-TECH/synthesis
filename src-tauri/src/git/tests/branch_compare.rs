//! What one branch changed against its base (GTC-FR-YQVD, GTC-FR-MBBH,
//! GTC-FR-PYVV, GTC-FR-QVDE).
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::history::commit_at;
use super::*;

/// Point `HEAD` at `branch` and make the working tree and index match it.
fn switch(repo: &Repository, branch: &str) {
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

/// Create `branch` at the tip of `from`.
fn cut(repo: &Repository, branch: &str, from: &str) {
    let tip = repo
        .find_branch(from, BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    repo.branch(branch, &tip, false).unwrap();
}

/// A repository whose default branch is `main`, with `feature` cut from it.
///
/// After the cut, `feature` adds `f.md` and renames `old.md` to `new.md`, and
/// `main` adds `m.md`. Returns the fixture and the commit `feature` was cut at.
fn forked() -> (Fixture, Oid) {
    let f = Fixture::new();
    let repo = f.repo();
    let fork = commit_at(
        &repo,
        "root",
        1_700_000_000,
        &[("a.md", Some(b"one\n")), ("old.md", Some(b"keep me as I am\n"))],
    );
    let root = repo.find_commit(fork).unwrap();
    repo.branch("main", &root, true).unwrap();
    switch(&repo, "main");
    for other in ["master"] {
        if let Ok(mut branch) = repo.find_branch(other, BranchType::Local) {
            branch.delete().unwrap();
        }
    }
    cut(&repo, "feature", "main");
    switch(&repo, "feature");
    commit_at(
        &repo,
        "feature work",
        1_700_000_060,
        &[
            ("f.md", Some(b"feature\n")),
            ("old.md", None),
            ("new.md", Some(b"keep me as I am\n")),
        ],
    );
    switch(&repo, "main");
    commit_at(&repo, "main work", 1_700_000_120, &[("m.md", Some(b"main\n"))]);
    (f, fork)
}

fn paths(comparison: &BranchComparison) -> Vec<&str> {
    comparison.files.iter().map(|f| f.path.as_str()).collect()
}

// GTC-FR-YQVD, GTC-FR-MBBH
#[test]
fn a_branch_is_compared_with_the_default_branch_from_their_merge_base() {
    let (f, fork) = forked();

    let comparison = branch_comparison_for(&f.root(), "feature", "local", None).unwrap();

    assert_eq!(comparison.branch, "feature");
    assert_eq!(comparison.kind, "local");
    assert_eq!(comparison.base, "main");
    assert_eq!(comparison.merge_base.as_deref(), Some(fork.to_string().as_str()));
    assert!(!comparison.same_as_base);
    // What main did after the cut is not the branch's change.
    assert_eq!(paths(&comparison), vec!["f.md", "new.md"]);
    let renamed = comparison.files.iter().find(|f| f.path == "new.md").unwrap();
    assert_eq!(renamed.status, CommitFileStatus::Renamed);
    assert_eq!(renamed.previous_path.as_deref(), Some("old.md"));
    let added = comparison.files.iter().find(|f| f.path == "f.md").unwrap();
    assert_eq!(added.status, CommitFileStatus::Added);
    assert!(!added.is_binary);
}

// GTC-FR-YQVD
#[test]
fn a_work_stream_branch_is_compared_with_the_stream_base() {
    let (f, _) = forked();
    let repo = f.repo();
    cut(&repo, "topic", "feature");
    switch(&repo, "topic");
    commit_at(&repo, "topic work", 1_700_000_200, &[("t.md", Some(b"topic\n"))]);

    let comparison = branch_comparison_for(&f.root(), "topic", "local", Some("feature")).unwrap();

    assert_eq!(comparison.base, "feature");
    assert_eq!(paths(&comparison), vec!["t.md"]);
}

// GTC-FR-MBBH
#[test]
fn the_default_branch_is_its_own_base_and_lists_no_path() {
    let (f, _) = forked();

    let comparison = branch_comparison_for(&f.root(), "main", "local", None).unwrap();

    assert_eq!(comparison.base, "main");
    assert!(comparison.same_as_base);
    assert!(comparison.merge_base.is_none());
    assert!(comparison.files.is_empty());
    let json = serde_json::to_value(&comparison).unwrap();
    assert_eq!(json["sameAsBase"], serde_json::json!(true));
    assert!(json.get("mergeBase").is_none());
}

// GTC-FR-YQVD, GTC-FR-MBBH
#[test]
fn a_remote_branch_is_compared_with_the_default_branch() {
    let (f, fork) = forked();
    let repo = f.repo();
    repo.remote("origin", "file:///nonexistent").unwrap();
    let tip = repo.find_branch("feature", BranchType::Local).unwrap().get().target().unwrap();
    repo.reference("refs/remotes/origin/feature", tip, true, "t").unwrap();

    let comparison = branch_comparison_for(&f.root(), "origin/feature", "remote", None).unwrap();

    assert_eq!(comparison.kind, "remote");
    assert_eq!(comparison.base, "main");
    assert_eq!(comparison.merge_base.as_deref(), Some(fork.to_string().as_str()));
    assert_eq!(paths(&comparison), vec!["f.md", "new.md"]);
}

// GTC-FR-YQVD: a base that only the primary remote holds still compares.
#[test]
fn a_base_held_only_by_the_primary_remote_is_found_there() {
    let (f, _) = forked();
    let repo = f.repo();
    repo.remote("origin", "file:///nonexistent").unwrap();
    let main_tip = repo.find_branch("main", BranchType::Local).unwrap().get().target().unwrap();
    repo.reference("refs/remotes/origin/trunk", main_tip, true, "t").unwrap();

    let comparison = branch_comparison_for(&f.root(), "feature", "local", Some("trunk")).unwrap();

    assert_eq!(comparison.base, "trunk");
    assert_eq!(paths(&comparison), vec!["f.md", "new.md"]);
}

// GTC-FR-QVDE
#[test]
fn a_name_that_is_not_a_branch_of_that_kind_is_unknown() {
    let (f, _) = forked();

    for (name, kind) in [
        ("nope", "local"),
        ("feature", "remote"),
        ("feature", "tag"),
        ("", "local"),
        ("origin/HEAD", "remote"),
    ] {
        assert_eq!(
            branch_comparison_for(&f.root(), name, kind, None).unwrap_err(),
            ERR_UNKNOWN_BRANCH,
            "{name:?} as {kind:?}"
        );
        assert_eq!(
            branch_compare_file_diff_for(&f.root(), name, kind, "f.md", None).unwrap_err(),
            ERR_UNKNOWN_BRANCH,
            "{name:?} as {kind:?}"
        );
    }
}

// GTC-FR-QVDE
#[test]
fn a_base_that_names_no_branch_is_refused() {
    let (f, _) = forked();

    assert_eq!(
        branch_comparison_for(&f.root(), "feature", "local", Some("missing")).unwrap_err(),
        ERR_NO_COMPARISON_BASE
    );
}

// GTC-FR-QVDE
#[test]
fn a_branch_that_shares_no_commit_with_its_base_is_refused() {
    let (f, _) = forked();
    let repo = f.repo();
    let sig = repo.signature().unwrap();
    let mut builder = repo.treebuilder(None).unwrap();
    let blob = repo.blob(b"alone\n").unwrap();
    builder.insert("alone.md", blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    repo.commit(Some("refs/heads/orphan"), &sig, &sig, "orphan", &tree, &[])
        .unwrap();

    assert_eq!(
        branch_comparison_for(&f.root(), "orphan", "local", None).unwrap_err(),
        ERR_NO_COMPARISON_MERGE_BASE
    );
}

// GTC-FR-PYVV
#[test]
fn the_diff_of_one_path_runs_from_the_merge_base_to_the_branch_tip() {
    let (f, _) = forked();

    let added = branch_compare_file_diff_for(&f.root(), "feature", "local", "f.md", None).unwrap();
    assert!(!added.is_binary);
    let lines: Vec<String> = added
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter().map(|l| l.content.clone()))
        .collect();
    assert!(lines.iter().any(|l| l.contains("feature")), "{lines:?}");

    // A pure rename is a change of the path, so it answers rather than refuses.
    branch_compare_file_diff_for(&f.root(), "feature", "local", "new.md", None).unwrap();
}

// GTC-FR-PYVV
#[test]
fn a_path_the_comparison_did_not_change_is_refused() {
    let (f, _) = forked();

    // `m.md` is the base's change, not the branch's.
    assert_eq!(
        branch_compare_file_diff_for(&f.root(), "feature", "local", "m.md", None).unwrap_err(),
        ERR_PATH_NOT_IN_COMPARISON
    );
    // The base compared with itself changed nothing.
    assert_eq!(
        branch_compare_file_diff_for(&f.root(), "main", "local", "a.md", None).unwrap_err(),
        ERR_PATH_NOT_IN_COMPARISON
    );
}

// GTC-FR-YQVD, GTC-FR-PYVV: both reads write nothing.
#[test]
fn comparing_a_branch_writes_nothing() {
    let (f, _) = forked();
    let before = crate::changes::tests_support::snapshot(f.root());

    branch_comparison_for(&f.root(), "feature", "local", None).unwrap();
    branch_compare_file_diff_for(&f.root(), "feature", "local", "f.md", None).unwrap();

    assert_eq!(crate::changes::tests_support::snapshot(f.root()), before);
}

// GTC-FR-MBBH: the wire shape the Git panel reads.
#[test]
fn the_comparison_serialises_in_camel_case() {
    let (f, _) = forked();

    let json =
        serde_json::to_value(branch_comparison_for(&f.root(), "feature", "local", None).unwrap())
            .unwrap();

    assert_eq!(json["sameAsBase"], serde_json::json!(false));
    assert!(json["mergeBase"].is_string());
    let renamed = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "new.md")
        .unwrap();
    assert_eq!(renamed["previousPath"], "old.md");
    assert_eq!(renamed["status"], "renamed");
}

// GTC-FR-PYVV: the change runs from the merge base, so what the base did to the
// same file after the fork is not in it.
#[test]
fn the_diff_holds_the_branch_change_and_not_the_base_change_to_the_same_file() {
    let (f, _) = forked();
    let repo = f.repo();
    commit_at(&repo, "main edits a", 1_700_000_300, &[("a.md", Some(b"one\nmain\n"))]);
    switch(&repo, "feature");
    commit_at(&repo, "feature edits a", 1_700_000_360, &[("a.md", Some(b"one\nfeature\n"))]);
    switch(&repo, "main");

    let diff = branch_compare_file_diff_for(&f.root(), "feature", "local", "a.md", None).unwrap();

    let lines: Vec<(String, String)> = diff
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter().map(|l| (l.kind.clone(), l.content.clone())))
        .collect();
    assert!(
        lines.iter().any(|(kind, text)| kind == "add" && text.contains("feature")),
        "{lines:?}"
    );
    assert!(!lines.iter().any(|(_, text)| text.contains("main")), "{lines:?}");
    assert!(!lines.iter().any(|(kind, _)| kind == "del"), "{lines:?}");
}

// GTC-FR-MBBH, GTC-FR-PYVV: a binary file is flagged in the list and in the diff.
#[test]
fn a_binary_file_is_flagged_in_the_list_and_has_no_hunks() {
    let (f, _) = forked();
    let repo = f.repo();
    switch(&repo, "feature");
    commit_at(&repo, "binary", 1_700_000_400, &[("logo.bin", Some(&[0u8, 159, 146, 150, 0, 1]))]);
    switch(&repo, "main");

    let comparison = branch_comparison_for(&f.root(), "feature", "local", None).unwrap();
    assert!(comparison.files.iter().find(|f| f.path == "logo.bin").unwrap().is_binary);
    let diff =
        branch_compare_file_diff_for(&f.root(), "feature", "local", "logo.bin", None).unwrap();
    assert!(diff.is_binary);
    assert!(diff.hunks.is_empty());
}

// GTC-FR-QVDE: both operations refuse a missing base and a base with no shared
// commit.
#[test]
fn both_operations_refuse_a_missing_base_and_an_unrelated_branch() {
    let (f, _) = forked();
    let repo = f.repo();
    let sig = repo.signature().unwrap();
    let mut builder = repo.treebuilder(None).unwrap();
    builder.insert("alone.md", repo.blob(b"alone\n").unwrap(), 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    repo.commit(Some("refs/heads/orphan"), &sig, &sig, "orphan", &tree, &[])
        .unwrap();

    assert_eq!(
        branch_compare_file_diff_for(&f.root(), "feature", "local", "f.md", Some("missing"))
            .unwrap_err(),
        ERR_NO_COMPARISON_BASE
    );
    assert_eq!(
        branch_compare_file_diff_for(&f.root(), "orphan", "local", "alone.md", None).unwrap_err(),
        ERR_NO_COMPARISON_MERGE_BASE
    );
}

// GTC-FR-MBBH: a branch its base already holds changed nothing against it.
#[test]
fn a_branch_its_base_already_holds_lists_no_path() {
    let (f, fork) = forked();
    let repo = f.repo();
    repo.branch("old", &repo.find_commit(fork).unwrap(), false).unwrap();

    let comparison = branch_comparison_for(&f.root(), "old", "local", None).unwrap();

    assert!(!comparison.same_as_base);
    assert!(comparison.files.is_empty());
}

// GTC-FR-YQVD, GTC-FR-QVDE: through the reported operation, a stream's branch is
// compared with the stream's base, and the read logs its start, its file
// count, and no file content.
#[test]
fn a_stream_branch_is_compared_with_its_base_and_the_reads_are_logged() {
    let fx = super::branch_deletion::DeletionFixture::new();
    let base = fx.f.current();
    let stream =
        crate::streams::create_work_stream(fx.app.clone(), "Compare".into(), Some(base.clone()))
            .unwrap();
    let worktree = git2::Repository::open(&stream.worktree_path).unwrap();
    commit_at(&worktree, "stream work", 1_700_000_500, &[("s.md", Some(b"secret body\n"))]);
    let log = own_buffer();

    let comparison =
        branch_comparison_reported(&fx.app, log, &fx.f.root(), &stream.branch, "local").unwrap();
    let diff =
        branch_compare_file_diff_reported(&fx.app, log, &fx.f.root(), &stream.branch, "local", "s.md")
            .unwrap();

    assert_eq!(comparison.base, base);
    assert_eq!(comparison.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["s.md"]);
    assert!(!diff.hunks.is_empty());

    let started = record_for(log, "branch comparison started");
    assert_eq!(started.fields.get("branch"), Some(&serde_json::json!(stream.branch)));
    assert_eq!(started.fields.get("isStream"), Some(&serde_json::json!(true)));
    let read = record_for(log, "branch comparison read");
    assert_eq!(read.fields.get("files"), Some(&serde_json::json!(1)));
    assert_eq!(read.fields.get("base"), Some(&serde_json::json!(base)));
    record_for(log, "branch comparison diff started");
    let computed = record_for(log, "branch comparison diff computed");
    assert_eq!(computed.fields.get("path"), Some(&serde_json::json!("s.md")));
    assert!(!buffer_text(log).contains("secret body"));
}

// GTC-FR-QVDE: a refusal is logged at the refusal level with its typed code.
#[test]
fn a_refused_comparison_is_logged_as_a_warning_with_its_code() {
    let fx = super::branch_deletion::DeletionFixture::new();
    let log = own_buffer();

    let error = branch_comparison_reported(&fx.app, log, &fx.f.root(), "nope", "local").unwrap_err();
    assert_eq!(error, ERR_UNKNOWN_BRANCH);
    let failed = record_for(log, "branch comparison failed");
    assert_eq!(failed.level, crate::logging::LogLevel::Warn);
    assert_eq!(failed.fields.get("error"), Some(&serde_json::json!(ERR_UNKNOWN_BRANCH)));

    let error =
        branch_compare_file_diff_reported(&fx.app, log, &fx.f.root(), "nope", "local", "a.md")
            .unwrap_err();
    assert_eq!(error, ERR_UNKNOWN_BRANCH);
    assert_eq!(
        record_for(log, "branch comparison diff failed").level,
        crate::logging::LogLevel::Warn
    );
}

