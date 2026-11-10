//! The commit of a closed proposal (CEP-FR-FOJW, CEP-FR-XVXQ, CEP-FR-IYHP,
//! CEP-FR-OAIW).

use super::*;

// CEP-FR-FOJW / CEP-FR-XVXQ: the decision that closes a proposal with an
// accepted hunk makes one commit that holds exactly the files those hunks
// wrote, under the fixed message, and pushes nothing.
#[test]
fn a_close_with_accepted_hunks_makes_one_commit_of_their_files() {
    let f = Fixture::new();
    let record = f.pending();
    let before = f.commits();
    f.accept(&record.id, &row_id(&record, "tasks.md")).expect("accepted");
    assert_eq!(f.commits(), before, "no commit while a hunk is undecided");
    let outcome = f.accept(&record.id, &row_id(&record, "specs/auth/spec.md")).expect("accepted");
    assert!(outcome.resolved);
    assert!(outcome.committed);
    assert_eq!(f.commits(), before + 1);
    let (subject, paths) = f.head();
    assert_eq!(subject, "Accept edits to OpenSpec change add-auth");
    assert_eq!(
        paths,
        vec![
            format!("openspec/changes/{CHANGE}/specs/auth/spec.md"),
            format!("openspec/changes/{CHANGE}/tasks.md"),
        ]
    );
    let comments = f.thread().comments;
    assert!(comments.last().expect("decision").body.starts_with("Accepted the proposed edits"));
    assert!(!f.reread(&record.id).commit_owed);
}

// CEP-FR-FOJW: a close by rejection still commits the files of the hunks that
// were accepted earlier.
#[test]
fn a_close_by_rejection_commits_the_earlier_accepted_files() {
    let f = Fixture::new();
    let record = f.pending();
    let before = f.commits();
    f.accept(&record.id, &row_id(&record, "tasks.md")).expect("accepted");
    let outcome = f.reject(&record.id, &row_id(&record, "specs/auth/spec.md")).expect("rejected");
    assert!(outcome.committed);
    assert_eq!(f.commits(), before + 1);
    assert_eq!(f.head().1, vec![format!("openspec/changes/{CHANGE}/tasks.md")]);
}

// CEP-FR-OAIW: a close with no accepted hunk makes no commit.
#[test]
fn a_close_with_no_accepted_hunk_makes_no_commit() {
    let f = Fixture::new();
    let record = f.pending();
    let before = f.commits();
    let outcome = f.decide(&record.id, Decision::Decline, None).expect("declined");
    assert!(!outcome.committed);
    assert_eq!(f.commits(), before);
}

// CEP-FR-XVXQ: a file that another writer changed after the hunk wrote it and
// before the proposal closed is committed as it stands on disk.
#[test]
fn a_file_changed_by_another_writer_before_the_close_is_committed_as_it_stands() {
    let f = Fixture::new();
    let record = f.pending();
    f.accept(&record.id, &row_id(&record, "tasks.md")).expect("accepted");
    let path = f.wc.path().join(format!("openspec/changes/{CHANGE}/tasks.md"));
    let written = f.file("tasks.md").expect("tasks");
    f.wc.write_text_atomic(&path, &format!("{written}- [ ] 1.9 Theirs\n")).expect("write");
    f.reject(&record.id, &row_id(&record, "specs/auth/spec.md")).expect("rejected");
    let repo = git2::Repository::open(f.wc.path()).expect("repo");
    let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
    let entry = tree
        .get_path(std::path::Path::new(&format!("openspec/changes/{CHANGE}/tasks.md")))
        .unwrap();
    let text = String::from_utf8(repo.find_blob(entry.id()).unwrap().content().to_vec()).unwrap();
    assert!(text.contains("1.2 Add logout") && text.contains("1.9 Theirs"));
}

// CEP-FR-IYHP: a commit that fails marks the proposal `commit_owed` and keeps
// the files written; `complete_change_edit_decision` commits every file still
// owed and clears the mark.
#[test]
fn a_failed_commit_is_owed_and_completed_later() {
    let f = Fixture::new();
    let record = f
        .record(&[replace("tasks.md", "- [ ] 1.1 Add the form\n", "- [ ] 1.1 Add it\n")])
        .expect("recorded");
    let tasks = row_id(&record, "tasks.md");
    // Hold the index lock so that the commit fails inside this repository.
    let lock = f.wc.path().join(".git/index.lock");
    std::fs::write(&lock, "").expect("lock");
    let outcome = f.accept(&record.id, &tasks).expect("accepted");
    assert!(outcome.resolved);
    assert!(!outcome.committed);
    assert!(outcome.proposal.commit_owed);
    assert!(f.file("tasks.md").expect("tasks").contains("1.1 Add it"));

    std::fs::remove_file(&lock).expect("unlock");
    let before = f.commits();
    let seen = f.watch();
    let done = complete_in_process(&f.handle(), &f.store, &f.sites, &record.id).expect("completed");
    assert!(done.committed);
    assert!(!done.proposal.commit_owed);
    assert_eq!(f.commits(), before + 1);
    assert_eq!(f.head().1, vec![format!("openspec/changes/{CHANGE}/tasks.md")]);
    // CEP-FR-SQCG: a commit and a cleared `commit_owed` are announced.
    assert!(seen.lock().unwrap().iter().any(|e| e["cause"] == "committed"));
}

// CEP-FR-IYHP: the next close of a proposal of the same change commits the
// files an earlier proposal still owes.
#[test]
fn the_next_close_commits_what_an_earlier_proposal_owes() {
    let f = Fixture::new();
    let first = f
        .record(&[replace("tasks.md", "- [ ] 1.1 Add the form\n", "- [ ] 1.1 Add it\n")])
        .expect("recorded");
    let lock = f.wc.path().join(".git/index.lock");
    std::fs::write(&lock, "").expect("lock");
    let owed = f.accept(&first.id, &row_id(&first, "tasks.md")).expect("accepted");
    assert!(owed.proposal.commit_owed);
    std::fs::remove_file(&lock).expect("unlock");

    let second = f
        .record(&[replace("proposal.md", "Users need to log in.", "Users log in.")])
        .expect("recorded");
    let before = f.commits();
    let outcome = f.accept(&second.id, &row_id(&second, "proposal.md")).expect("accepted");
    assert!(outcome.committed);
    assert_eq!(f.commits(), before + 1);
    assert_eq!(
        f.head().1,
        vec![
            format!("openspec/changes/{CHANGE}/proposal.md"),
            format!("openspec/changes/{CHANGE}/tasks.md"),
        ]
    );
    assert!(!f.reread(&first.id).commit_owed);
}
