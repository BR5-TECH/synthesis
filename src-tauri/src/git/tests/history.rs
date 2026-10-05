//! The commit history and the files of one commit
//! (GTC-FR-BQNM, GTC-FR-RFLW, GTC-FR-YCEV, GTC-FR-PDSK).
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

/// A signature at a fixed instant, so the order of a history is the order the
/// test wrote and never the order of the wall clock.
fn signature_at(seconds: i64) -> git2::Signature<'static> {
    git2::Signature::new("Ada Lovelace", "ada@example.com", &git2::Time::new(seconds, 0))
        .unwrap()
}

/// One commit on `HEAD` at `seconds`. A file given as `Some` is written and
/// staged; one given as `None` is removed.
pub(super) fn commit_at(
    repo: &Repository,
    message: &str,
    seconds: i64,
    changes: &[(&str, Option<&[u8]>)],
) -> Oid {
    let workdir = repo.workdir().unwrap().to_path_buf();
    let mut index = repo.index().unwrap();
    for (path, content) in changes {
        let full = workdir.join(path);
        match content {
            Some(bytes) => {
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&full, bytes).unwrap();
                index.add_path(std::path::Path::new(path)).unwrap();
            }
            None => {
                std::fs::remove_file(&full).unwrap();
                index.remove_path(std::path::Path::new(path)).unwrap();
            }
        }
    }
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = signature_at(seconds);
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &refs)
        .unwrap()
}

fn file_named<'a>(files: &'a [CommitFile], path: &str) -> &'a CommitFile {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} is not in {files:?}"))
}

// ---------------------------------------------------------------------------
// History (GTC-FR-BQNM)
// ---------------------------------------------------------------------------

// GTC-FR-BQNM
#[test]
fn history_holds_the_hundred_latest_commits_newest_first() {
    let f = Fixture::new();
    let repo = f.repo();
    for n in 0..105 {
        commit_at(
            &repo,
            &format!("commit {n}"),
            1_700_000_000 + n * 60,
            &[("a.md", Some(format!("{n}\n").as_bytes()))],
        );
    }

    let history = commit_history_for(&f.root()).unwrap();

    assert_eq!(history.commits.len(), 100);
    assert_eq!(history.commits[0].subject, "commit 104");
    assert_eq!(history.commits[99].subject, "commit 5");
    let times: Vec<i64> = history.commits.iter().map(|c| c.authored_at).collect();
    assert!(times.windows(2).all(|pair| pair[0] > pair[1]), "newest first: {times:?}");
    assert_eq!(history.head_id.as_deref(), Some(history.commits[0].id.as_str()));
    assert_eq!(history.branch.as_deref(), Some(f.current_branch().as_str()));
    assert!(!history.is_detached);
}

// GTC-FR-BQNM
#[test]
fn history_of_a_branch_with_no_commit_is_empty_and_still_names_the_branch() {
    let f = Fixture::new();

    let history = commit_history_for(&f.root()).unwrap();

    assert!(history.commits.is_empty());
    assert!(history.head_id.is_none());
    assert!(!history.is_detached);
    assert!(history.branch.is_some(), "the unborn branch has a name");
}

// GTC-FR-BQNM
#[test]
fn history_of_a_detached_head_has_no_branch_name() {
    let f = Fixture::new();
    let repo = f.repo();
    let first = commit_at(&repo, "first", 1_700_000_000, &[("a.md", Some(b"1\n"))]);
    commit_at(&repo, "second", 1_700_000_060, &[("a.md", Some(b"2\n"))]);
    repo.set_head_detached(first).unwrap();

    let history = commit_history_for(&f.root()).unwrap();

    assert!(history.is_detached);
    assert!(history.branch.is_none());
    assert_eq!(history.head_id.as_deref(), Some(first.to_string().as_str()));
    assert_eq!(history.commits.len(), 1, "only what the detached head reaches");
    let json = serde_json::to_value(&history).unwrap();
    assert!(json.get("branch").is_none(), "an absent branch is omitted: {json}");
}

// GTC-FR-BQNM
#[test]
fn history_outside_a_repository_is_the_typed_error() {
    let dir = TempDir::new().unwrap();
    assert_eq!(commit_history_for(dir.path()).unwrap_err(), ERR_NOT_A_REPO);
    assert_eq!(
        commit_files_for(dir.path(), "0123456789012345678901234567890123456789").unwrap_err(),
        ERR_NOT_A_REPO
    );
}

// GTC-FR-BQNM, GTC-FR-YCEV, GTC-FR-PDSK
#[test]
fn reading_history_files_and_diffs_writes_nothing() {
    let f = Fixture::new();
    let repo = f.repo();
    let id = commit_at(&repo, "one", 1_700_000_000, &[("a.md", Some(b"1\n"))]);
    let before = crate::changes::tests_support::snapshot(f.root());

    commit_history_for(&f.root()).unwrap();
    commit_files_for(&f.root(), &id.to_string()).unwrap();
    commit_file_diff_for(&f.root(), &id.to_string(), "a.md").unwrap();

    assert_eq!(before, crate::changes::tests_support::snapshot(f.root()));
}

// ---------------------------------------------------------------------------
// The summary of a commit (GTC-FR-RFLW)
// ---------------------------------------------------------------------------

// GTC-FR-RFLW
#[test]
fn a_summary_carries_the_author_the_time_the_subject_the_message_and_the_refs() {
    let f = Fixture::new();
    let repo = f.repo();
    let older = commit_at(&repo, "older", 1_700_000_000, &[("a.md", Some(b"1\n"))]);
    let tip = commit_at(
        &repo,
        "Add the panel\n\nThe body explains why.\n",
        1_700_000_600,
        &[("a.md", Some(b"2\n"))],
    );
    let tip_commit = repo.find_commit(tip).unwrap();
    repo.branch("feature", &tip_commit, false).unwrap();
    repo.reference("refs/remotes/origin/main", tip, true, "t").unwrap();
    // The symbolic pointer of a remote is not a branch.
    repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/remotes/origin/main", true, "t")
        .unwrap();
    repo.reference("refs/remotes/origin/old", older, true, "t").unwrap();

    let history = commit_history_for(&f.root()).unwrap();

    let top = &history.commits[0];
    assert_eq!(top.id, tip.to_string());
    assert_eq!(top.short_id, tip.to_string()[..7]);
    assert_eq!(top.author_name, "Ada Lovelace");
    assert_eq!(top.author_email, "ada@example.com");
    assert_eq!(top.authored_at, 1_700_000_600);
    assert_eq!(top.subject, "Add the panel");
    assert_eq!(top.message, "Add the panel\n\nThe body explains why.\n");
    let mut refs = top.refs.clone();
    refs.sort();
    let current = f.current_branch();
    let mut expected = vec![current, "feature".to_string(), "origin/main".to_string()];
    expected.sort();
    assert_eq!(refs, expected, "local and remote-tracking tips, and no origin/HEAD");
    assert_eq!(history.commits[1].refs, vec!["origin/old".to_string()]);
}

// GTC-FR-RFLW
#[test]
fn a_commit_that_is_no_branch_tip_has_no_refs_and_the_wire_names_are_camel_case() {
    let f = Fixture::new();
    let repo = f.repo();
    commit_at(&repo, "one", 1_700_000_000, &[("a.md", Some(b"1\n"))]);
    commit_at(&repo, "two", 1_700_000_060, &[("a.md", Some(b"2\n"))]);

    let history = commit_history_for(&f.root()).unwrap();

    assert!(history.commits[1].refs.is_empty());
    let json = serde_json::to_value(&history).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["branch", "commits", "headId", "isDetached"]);
    let mut commit_keys: Vec<&str> = json["commits"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    commit_keys.sort();
    assert_eq!(
        commit_keys,
        vec![
            "authorEmail",
            "authorName",
            "authoredAt",
            "id",
            "message",
            "refs",
            "shortId",
            "subject"
        ]
    );
}

// ---------------------------------------------------------------------------
// The files of one commit (GTC-FR-YCEV)
// ---------------------------------------------------------------------------

const LONG_TEXT: &str = "line one of the document\nline two of the document\nline three of the document\nline four of the document\nline five of the document\nline six of the document\n";

// GTC-FR-YCEV
#[test]
fn a_commit_lists_what_it_added_changed_deleted_renamed_and_flags_a_binary_file() {
    let f = Fixture::new();
    let repo = f.repo();
    commit_at(
        &repo,
        "base",
        1_700_000_000,
        &[
            ("keep.txt", Some(b"one\ntwo\n")),
            ("gone.txt", Some(b"bye\n")),
            ("moved/old.txt", Some(LONG_TEXT.as_bytes())),
        ],
    );
    let id = commit_at(
        &repo,
        "change everything",
        1_700_000_100,
        &[
            ("keep.txt", Some(b"one\nTWO\n")),
            ("gone.txt", None),
            ("moved/old.txt", None),
            ("moved/new.txt", Some(LONG_TEXT.as_bytes())),
            ("fresh.txt", Some(b"hello\n")),
            ("image.bin", Some(&[0u8, 1, 2, 0, 255])),
        ],
    );

    let files = commit_files_for(&f.root(), &id.to_string()).unwrap();

    assert_eq!(files.len(), 5, "{files:?}");
    assert_eq!(file_named(&files, "keep.txt").status, CommitFileStatus::Modified);
    assert_eq!(file_named(&files, "gone.txt").status, CommitFileStatus::Deleted);
    assert_eq!(file_named(&files, "fresh.txt").status, CommitFileStatus::Added);
    let renamed = file_named(&files, "moved/new.txt");
    assert_eq!(renamed.status, CommitFileStatus::Renamed);
    assert_eq!(renamed.previous_path.as_deref(), Some("moved/old.txt"));
    assert!(file_named(&files, "fresh.txt").previous_path.is_none());
    let binary = file_named(&files, "image.bin");
    assert!(binary.is_binary);
    assert!(!file_named(&files, "keep.txt").is_binary);

    let json = serde_json::to_value(renamed).unwrap();
    assert_eq!(json["previousPath"], "moved/old.txt");
    assert_eq!(json["status"], "renamed");
    assert_eq!(json["isBinary"], false);
    assert!(serde_json::to_value(file_named(&files, "fresh.txt")).unwrap().get("previousPath").is_none());
}

// GTC-FR-YCEV
#[test]
fn a_root_commit_lists_every_file_as_added() {
    let f = Fixture::new();
    let repo = f.repo();
    let root = commit_at(
        &repo,
        "root",
        1_700_000_000,
        &[("a.md", Some(b"1\n")), ("dir/b.md", Some(b"2\n"))],
    );

    let files = commit_files_for(&f.root(), &root.to_string()).unwrap();

    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|file| file.status == CommitFileStatus::Added));
}

// GTC-FR-YCEV, GTC-FR-PDSK
#[test]
fn an_id_that_names_no_commit_is_unknown_commit_for_both_reads() {
    let f = Fixture::new();
    let repo = f.repo();
    commit_at(&repo, "one", 1_700_000_000, &[("a.md", Some(b"1\n"))]);

    for id in ["not-an-id", "", "0000000000000000000000000000000000000001"] {
        assert_eq!(commit_files_for(&f.root(), id).unwrap_err(), ERR_UNKNOWN_COMMIT, "{id:?}");
        assert_eq!(
            commit_file_diff_for(&f.root(), id, "a.md").unwrap_err(),
            ERR_UNKNOWN_COMMIT,
            "{id:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// One commit's change to one path (GTC-FR-PDSK)
// ---------------------------------------------------------------------------

// GTC-FR-PDSK
#[test]
fn a_commit_file_diff_has_the_hunks_of_that_path_alone() {
    let f = Fixture::new();
    let repo = f.repo();
    commit_at(
        &repo,
        "base",
        1_700_000_000,
        &[("a.md", Some(b"one\ntwo\nthree\n")), ("b.md", Some(b"x\n"))],
    );
    let id = commit_at(
        &repo,
        "edit",
        1_700_000_100,
        &[("a.md", Some(b"one\nTWO\nthree\n")), ("b.md", Some(b"y\n"))],
    );

    let payload = commit_file_diff_for(&f.root(), &id.to_string(), "a.md").unwrap();

    assert!(!payload.is_binary);
    assert_eq!(payload.hunks.len(), 1);
    let lines = all_lines(&payload);
    assert!(lines.contains(&("del".to_string(), "two".to_string())), "{lines:?}");
    assert!(lines.contains(&("add".to_string(), "TWO".to_string())), "{lines:?}");
    assert!(!lines.iter().any(|(_, text)| text == "y"), "b.md is not in view: {lines:?}");
}

// GTC-FR-PDSK
#[test]
fn a_commit_file_diff_of_a_binary_file_is_the_binary_marker() {
    let f = Fixture::new();
    let repo = f.repo();
    let id = commit_at(&repo, "bin", 1_700_000_000, &[("image.bin", Some(&[0u8, 1, 2, 0]))]);

    let payload = commit_file_diff_for(&f.root(), &id.to_string(), "image.bin").unwrap();

    assert!(payload.is_binary);
    assert!(payload.hunks.is_empty());
}

// GTC-FR-PDSK
#[test]
fn a_commit_file_diff_of_a_root_commit_is_all_additions() {
    let f = Fixture::new();
    let repo = f.repo();
    let id = commit_at(&repo, "root", 1_700_000_000, &[("a.md", Some(b"one\ntwo\n"))]);

    let payload = commit_file_diff_for(&f.root(), &id.to_string(), "a.md").unwrap();

    let lines = all_lines(&payload);
    assert_eq!(
        lines,
        vec![
            ("add".to_string(), "one".to_string()),
            ("add".to_string(), "two".to_string())
        ]
    );
}

// GTC-FR-PDSK
#[test]
fn a_commit_file_diff_of_a_renamed_path_shows_the_edit_not_a_whole_new_file() {
    let f = Fixture::new();
    let repo = f.repo();
    commit_at(&repo, "base", 1_700_000_000, &[("old.txt", Some(LONG_TEXT.as_bytes()))]);
    let edited = LONG_TEXT.replace("line three", "LINE THREE");
    let id = commit_at(
        &repo,
        "rename and edit",
        1_700_000_100,
        &[("old.txt", None), ("new.txt", Some(edited.as_bytes()))],
    );

    let payload = commit_file_diff_for(&f.root(), &id.to_string(), "new.txt").unwrap();

    let lines = all_lines(&payload);
    let added = lines.iter().filter(|(kind, _)| kind == "add").count();
    assert_eq!(added, 1, "only the edited line is added: {lines:?}");
}

// GTC-FR-PDSK
#[test]
fn a_path_the_commit_did_not_change_is_path_not_in_commit() {
    let f = Fixture::new();
    let repo = f.repo();
    commit_at(&repo, "base", 1_700_000_000, &[("a.md", Some(b"1\n")), ("b.md", Some(b"1\n"))]);
    let id = commit_at(&repo, "edit a", 1_700_000_100, &[("a.md", Some(b"2\n"))]);

    for path in ["b.md", "missing.md", "*.md", ""] {
        assert_eq!(
            commit_file_diff_for(&f.root(), &id.to_string(), path).unwrap_err(),
            ERR_PATH_NOT_IN_COMMIT,
            "{path:?}"
        );
    }
}

// GTC-FR-PDSK
#[test]
fn a_commit_file_diff_of_a_deleted_path_is_all_removals() {
    let f = Fixture::new();
    let repo = f.repo();
    commit_at(&repo, "base", 1_700_000_000, &[("a.md", Some(b"one\n")), ("b.md", Some(b"keep\n"))]);
    let id = commit_at(&repo, "delete a", 1_700_000_100, &[("a.md", None)]);

    let payload = commit_file_diff_for(&f.root(), &id.to_string(), "a.md").unwrap();

    assert_eq!(all_lines(&payload), vec![("del".to_string(), "one".to_string())]);
}
