//! The message rewrite a completing run gives its own abandoned commit
//! (`GRD-graduation.md` GRD-FR-WQTN), held to the two conditions it rewrites
//! under.

use super::*;

/// A commit on the stream's branch, holding one file and the message named.
fn commit_on(worktree: &Path, file: &str, message: &str) -> String {
    let fs = crate::fs::FsAccess::builder()
        .allow_root(&worktree.to_path_buf())
        .build()
        .expect("the working copy");
    fs.write_text_atomic(worktree.join(file), message)
        .expect("the file is writable");
    let repo = git2::Repository::open(worktree).expect("the stream's repository");
    let mut index = repo.index().expect("the index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("staged");
    index.write().expect("written");
    let tree = repo
        .find_tree(index.write_tree().expect("a tree"))
        .expect("a tree");
    let signature = repo.signature().expect("a signature");
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("the head");
    let revision = repo
        .commit(Some("HEAD"), &signature, &signature, message, &tree, &[&head])
        .expect("committed")
        .to_string();
    revision
}

fn head_message(worktree: &Path) -> String {
    let repo = git2::Repository::open(worktree).expect("the stream's repository");
    let message = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("the head")
        .message()
        .unwrap_or_default()
        .to_string();
    message
}

// GRD-FR-WQTN: the rewrite happens only where the branch head is the revision
// the run recorded and its message opens with the abandoned title. A head that
// moved past that revision, and a head that is not an abandoned turn, each
// keep their message.
#[test]
fn only_the_named_abandoned_head_is_rewritten() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let worktree = stream.worktree();
    let title = "Abandoned graduation turn";

    let abandoned = commit_on(&worktree, "a.txt", "Abandoned graduation turn\n\nstopped");
    let other = commit_on(&worktree, "b.txt", "Someone else's commit");

    assert_eq!(
        crate::graduation::retitle_head_commit(&worktree, &abandoned, title, "Graduated").unwrap(),
        None,
        "a head that moved past the run's commit is left alone"
    );
    assert_eq!(
        crate::graduation::retitle_head_commit(&worktree, &other, title, "Graduated").unwrap(),
        None,
        "a head that is not an abandoned turn is left alone"
    );
    assert_eq!(head_message(&worktree), "Someone else's commit");

    let own = commit_on(&worktree, "c.txt", "Abandoned graduation turn\n\nstopped again");
    let rewritten = crate::graduation::retitle_head_commit(&worktree, &own, title, "Graduated")
        .unwrap()
        .expect("the run's own abandoned head is rewritten");
    assert_ne!(rewritten, own, "the rewrite makes a revision of its own");
    assert_eq!(head_message(&worktree), "Graduated");
}
