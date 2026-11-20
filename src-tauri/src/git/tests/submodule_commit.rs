//! The commit across the repository set (GTC-FR-CHEC, GTC-FR-GDNR,
//! GTC-FR-TJPJ, GTC-FR-19; GSM-FR-KWWZ, GSM-FR-MPWK, GSM-FR-ZKEJ) and the
//! file reads of a submodule file (GTC-FR-HYHB).
//!
//! One part of `mod.rs`. These run on a real superproject whose submodules
//! libgit2 cloned (`changes::repo_set::tests::fixture`).

use super::*;
use crate::changes::repo_set::tests::fixture::{commit_all, write, Superproject};

fn rootfs(p: &Superproject) -> crate::fs::RootFs {
    crate::fs::RootFs::for_root(p.root())
}

fn head_of(repo: &Repository) -> git2::Oid {
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

/// The entry `tree` of `repo`'s `HEAD` holds at `spec`, as (mode, id).
fn head_entry(repo: &Repository, spec: &str) -> (i32, git2::Oid) {
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    let entry = tree.get_path(Path::new(spec)).unwrap();
    (entry.filemode(), entry.id())
}

fn names(paths: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = paths.iter().map(String::as_str).collect();
    out.sort();
    out
}

// GTC-FR-CHEC, GTC-FR-GDNR, GSM-FR-KWWZ, GSM-FR-MPWK: a file of a submodule
// is committed in the submodule first, and the main commit then records the
// new submodule HEAD as a submodule entry, with the same message.
#[test]
fn a_submodule_file_commit_records_the_new_pointer_in_the_parent() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");

    let outcome = commit_named_paths(&rootfs(&p), "work", &["libs/sub/a.md".into()], None).unwrap();
    let order: Vec<&str> = outcome.repositories.iter().map(|c| c.repository.as_str()).collect();
    assert_eq!(order, ["libs/sub", ""]);
    let sub_head = head_of(&sub);
    assert_eq!(outcome.repositories[0].commit_id, sub_head.to_string());
    assert_eq!(outcome.commit_id, head_of(&p.main()).to_string(), "the main commit");
    assert_eq!(head_entry(&p.main(), "libs/sub"), (0o160000, sub_head));
    for repo in [&sub, &p.main()] {
        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.message().ok(), Some("work"));
    }
    assert_eq!(names(&outcome.committed_paths), ["libs/sub", "libs/sub/a.md"]);
    let after = crate::changes::uncommitted_change_set(&rootfs(&p)).unwrap();
    assert!(after.entries.is_empty(), "{:?}", after.entries);
}

// GSM-FR-KWWZ, GSM-FR-MPWK: files of three levels are committed deepest
// first, and each parent records the new pointer of its child.
#[test]
fn nested_commits_run_deepest_first_and_chain_the_pointers() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let inner = p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    let before = [head_of(&inner), head_of(&sub), head_of(&p.main())];
    write(&inner, "i.md", "i\n2\n");
    write(&sub, "a.md", "a\n2\n");
    write(&p.main(), "top.md", "top\n2\n");

    let paths = ["top.md".into(), "libs/sub/a.md".into(), "libs/sub/inner/i.md".into()];
    let outcome = commit_named_paths(&rootfs(&p), "three", &paths, None).unwrap();
    let order: Vec<&str> = outcome.repositories.iter().map(|c| c.repository.as_str()).collect();
    assert_eq!(order, ["libs/sub/inner", "libs/sub", ""]);
    assert_eq!(head_entry(&sub, "inner"), (0o160000, head_of(&inner)));
    assert_eq!(head_entry(&p.main(), "libs/sub"), (0o160000, head_of(&sub)));
    for (repo, old) in [&inner, &sub, &p.main()].iter().zip(before) {
        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.parent_ids().collect::<Vec<_>>(), [old], "one parent: the old HEAD");
    }
}

// GTC-FR-CHEC, GSM-FR-FCOF: naming only a pointer that moved makes one commit
// in the parent, which records the checked-out submodule commit.
#[test]
fn a_pointer_alone_is_committed_in_the_parent() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    let moved = commit_all(&sub, "outside the app");

    let outcome = commit_named_paths(&rootfs(&p), "bump", &["libs/sub".into()], None).unwrap();
    assert_eq!(outcome.repositories.len(), 1);
    assert_eq!(outcome.repositories[0].repository, "");
    assert_eq!(outcome.committed_paths, ["libs/sub"]);
    assert_eq!(head_entry(&p.main(), "libs/sub"), (0o160000, moved));
    assert_eq!(head_of(&sub), moved, "the submodule makes no commit");
}

// GTC-FR-20, GTC-FR-TJPJ: nothing to commit in any repository is refused
// before anything is written.
#[test]
fn nothing_to_commit_across_repositories_writes_nothing() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let heads = (head_of(&p.main()), head_of(&sub));
    let paths = ["top.md".into(), "libs/sub/a.md".into(), "libs/sub".into()];
    let err = commit_named_paths(&rootfs(&p), "none", &paths, None).unwrap_err();
    assert_eq!(err, ERR_NOTHING_TO_COMMIT);
    assert_eq!((head_of(&p.main()), head_of(&sub)), heads);
}

// GTC-FR-TJPJ: a path outside the content root is refused before the first
// commit, so no repository moves.
#[test]
fn validation_runs_before_the_first_commit() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    let heads = (head_of(&p.main()), head_of(&sub));
    let paths = ["libs/sub/a.md".into(), "../escape.md".into()];
    assert!(commit_named_paths(&rootfs(&p), "x", &paths, None).is_err());
    assert_eq!((head_of(&p.main()), head_of(&sub)), heads);
}

// GTC-FR-TJPJ, GSM-FR-ZKEJ: when the parent commit fails after the submodule
// commit was made, the error names the commit made and the repository that
// failed, and the submodule commit stays.
#[test]
fn a_parent_failure_after_a_submodule_commit_is_a_partial_commit() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    let main_head = head_of(&p.main());
    let lock = p.root().join(".git/index.lock");
    std::fs::write(&lock, "").unwrap();

    let err = commit_named_paths(&rootfs(&p), "work", &["libs/sub/a.md".into()], None).unwrap_err();
    std::fs::remove_file(&lock).unwrap();
    let json = err.strip_prefix("partial_commit: ").unwrap_or_else(|| panic!("{err}"));
    let data: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(data["failed"], "");
    assert!(!data["cause"].as_str().unwrap().is_empty());
    assert_eq!(data["committed"][0]["repository"], "libs/sub");
    assert_eq!(data["committed"][0]["commitId"], head_of(&sub).to_string());
    assert_eq!(head_of(&p.main()), main_head, "the main repository did not move");
}

// GTC-FR-TJPJ: a failure in the first repository is the plain error.
#[test]
fn a_failure_of_the_first_commit_is_not_partial() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    let heads = (head_of(&p.main()), head_of(&sub));
    let lock = p.root().join(".git/modules/libs/sub/index.lock");
    std::fs::write(&lock, "").unwrap();
    let err = commit_named_paths(&rootfs(&p), "work", &["libs/sub/a.md".into()], None).unwrap_err();
    std::fs::remove_file(&lock).unwrap();
    assert!(!err.starts_with(ERR_PARTIAL_COMMIT), "{err}");
    assert_eq!((head_of(&p.main()), head_of(&sub)), heads);
}

// GTC-FR-GDNR: a gitlink index mode is a submodule tree entry.
#[test]
fn a_gitlink_mode_is_a_submodule_tree_entry() {
    assert_eq!(tree_mode(0o160000), git2::FileMode::Commit);
    assert_eq!(tree_mode(0o100644), git2::FileMode::Blob);
}

// GSM-FR-WPKJ: a commit in more than one repository logs one record for each.
#[test]
fn each_repository_commit_is_logged() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    let buffer = own_buffer();
    commit_paths_reported(&NullSink, buffer, &rootfs(&p), "w", &["libs/sub/a.md".into()], None)
        .unwrap();
    let records = records_for(buffer, "commit created in a repository");
    let repos: Vec<String> =
        records.iter().map(|r| r.fields["repository"].as_str().unwrap().to_string()).collect();
    assert_eq!(repos, ["libs/sub", ""]);
}

// GTC-FR-HYHB: the diff and the file revisions of a submodule file come from
// the submodule HEAD, and in Branch scope from the commit the main merge-base
// records.
#[test]
fn a_submodule_file_is_read_in_the_submodule() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "one\n")]);
    let main = p.main();
    let head = main.head().unwrap().peel_to_commit().unwrap();
    main.branch("base", &head, false).unwrap();
    write(&sub, "a.md", "two\n");
    commit_all(&sub, "two");
    commit_all(&main, "record two");
    write(&sub, "a.md", "three\n");

    let path = DiffScope::Path { path: "libs/sub/a.md".into(), previous_path: None };
    let revisions = file_revisions_for_scope(&rootfs(&p), &path).unwrap();
    assert_eq!(revisions.old.as_deref(), Some("two\n"));
    assert_eq!(revisions.new.as_deref(), Some("three\n"));
    let lines = all_lines(&diff_for_scope(&p.root(), &path).unwrap());
    assert!(lines.iter().any(|(o, l)| o == "del" && l.contains("two")), "{lines:?}");

    let branch = DiffScope::Branch {
        path: "libs/sub/a.md".into(),
        target_branch: "base".into(),
        previous_path: None,
    };
    let revisions = file_revisions_for_scope(&rootfs(&p), &branch).unwrap();
    assert_eq!(revisions.old.as_deref(), Some("one\n"));
}

// GTC-FR-TJPJ, GSM-FR-MPWK: after a partial commit, the same request again
// records the submodule pointer that the failed run did not record.
#[test]
fn a_retry_after_a_partial_commit_records_the_pointer() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    let lock = p.root().join(".git/index.lock");
    std::fs::write(&lock, "").unwrap();
    let paths = ["libs/sub/a.md".to_string()];
    assert!(commit_named_paths(&rootfs(&p), "w", &paths, None).is_err());
    std::fs::remove_file(&lock).unwrap();

    let outcome = commit_named_paths(&rootfs(&p), "w", &paths, None).unwrap();
    assert_eq!(outcome.repositories.len(), 1);
    assert_eq!(outcome.repositories[0].repository, "");
    assert_eq!(outcome.committed_paths, ["libs/sub"]);
    assert_eq!(head_entry(&p.main(), "libs/sub"), (0o160000, head_of(&sub)));
}

// GSM-FR-ZKEJ: when a middle repository fails, no later commit is made, and
// the error names the commit made before it and the repository that failed.
#[test]
fn a_middle_failure_stops_the_run() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let inner = p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    write(&inner, "i.md", "i\n2\n");
    let heads = (head_of(&sub), head_of(&p.main()));
    let lock = p.root().join(".git/modules/libs/sub/index.lock");
    std::fs::write(&lock, "").unwrap();
    let err =
        commit_named_paths(&rootfs(&p), "w", &["libs/sub/inner/i.md".into()], None).unwrap_err();
    std::fs::remove_file(&lock).unwrap();
    let json = err.strip_prefix("partial_commit: ").unwrap_or_else(|| panic!("{err}"));
    let data: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(data["failed"], "libs/sub");
    assert_eq!(data["committed"].as_array().unwrap().len(), 1);
    assert_eq!(data["committed"][0]["repository"], "libs/sub/inner");
    assert_eq!((head_of(&sub), head_of(&p.main())), heads, "no later commit");
}

// GTC-FR-19, GSM-FR-MPWK: a moved pointer that the request does not name or
// imply stays out of the parent commit.
#[test]
fn an_unnamed_moved_pointer_stays_out_of_the_commit() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let recorded = head_of(&sub);
    write(&sub, "a.md", "a\nmore\n");
    commit_all(&sub, "moved outside the app");
    write(&p.main(), "top.md", "top\n2\n");
    let outcome = commit_named_paths(&rootfs(&p), "top", &["top.md".into()], None).unwrap();
    assert_eq!(outcome.committed_paths, ["top.md"]);
    assert_eq!(head_entry(&p.main(), "libs/sub"), (0o160000, recorded));
}

// GSM-FR-GHFY: the exact commit of an application store commits in the main
// repository alone, so it never writes a submodule branch with no check.
#[test]
fn an_exact_commit_never_writes_a_submodule() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    write(&sub, "a.md", "a\nmore\n");
    write(&p.main(), "top.md", "top\n2\n");
    let sub_head = head_of(&sub);
    let paths = ["libs/sub/a.md".into(), "top.md".into()];
    let outcome = commit_exact_paths(&rootfs(&p), "store", &paths).unwrap();
    assert_eq!(outcome.repositories.len(), 1);
    assert_eq!(outcome.repositories[0].repository, "");
    assert_eq!(outcome.committed_paths, ["top.md"]);
    assert_eq!(head_of(&sub), sub_head, "the submodule is not written");
}
