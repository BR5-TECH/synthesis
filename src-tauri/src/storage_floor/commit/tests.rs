//! The tests of the draft-event commit: what one event writes, what it leaves,
//! and the order events are committed in.

use super::*;
use std::path::Path;
use tempfile::TempDir;

/// The generated-shaped id every fixture draft is filed under.
const ID: &str = "1a2b3c4d5e6-0001-deadbeef";

/// A project whose worktree is a repository with one commit and a
/// configured authoring identity — a checkout as an author leaves one.
struct Fixture {
    _dir: TempDir,
    root: RootFs,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let repo = git2::Repository::init(dir.path()).expect("repo");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "An Author").expect("name");
        config.set_str("user.email", "author@example.com").expect("email");
        std::fs::write(dir.path().join("README.md"), "the project\n").expect("a file");
        commit_all(&repo, "the first commit");
        let root = RootFs::for_root(dir.path());
        Fixture { _dir: dir, root }
    }

    fn repo(&self) -> git2::Repository {
        git2::Repository::open(self.root.path()).expect("repo")
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.root.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the folder");
        std::fs::write(path, contents).expect("the file");
    }

    /// Every path Git reports as changed, so a test can say what is left.
    fn dirty(&self) -> Vec<String> {
        let repo = self.repo();
        let mut options = git2::StatusOptions::new();
        options.include_untracked(true).recurse_untracked_dirs(true);
        let statuses = repo.statuses(Some(&mut options)).expect("status");
        let paths: Vec<String> = statuses
            .iter()
            .filter(|entry| !entry.status().is_empty())
            .filter_map(|entry| entry.path().map(str::to_string).ok())
            .collect();
        paths
    }

    fn head_message(&self) -> String {
        self.repo()
            .head()
            .and_then(|head| head.peel_to_commit())
            .expect("a head")
            .message()
            .unwrap_or_default()
            .to_string()
    }

    fn head_holds(&self, rel: &str) -> bool {
        self.repo()
            .head()
            .and_then(|head| head.peel_to_commit())
            .and_then(|commit| commit.tree())
            .expect("a tree")
            .get_path(Path::new(rel))
            .is_ok()
    }
}

fn commit_all(repo: &git2::Repository, message: &str) {
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .expect("add");
    index.write().expect("write");
    let tree = repo
        .find_tree(index.write_tree().expect("tree"))
        .expect("tree");
    let signature = repo.signature().expect("signature");
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )
    .expect("commit");
}

// PST-FR-DQZT / PST-FR-TYNC / PST-FR-YWXF / DRS-FR-VECL: one event is one
// commit holding the committed storage of its draft, under the event's own
// message; the author's staged file is untouched and stays staged.
#[test]
fn one_event_commits_its_draft_and_leaves_the_author_staged() {
    let f = Fixture::new();
    f.write(&format!(".synthesis/drafts/{ID}/draft.toml"), "id = \"one\"\n");
    f.write(&format!(".synthesis/drafts/{ID}/files/Prompt.md"), "the prompt\n");
    f.write(".synthesis/drafts/.gitattributes", "*.jsonl merge=union\n");
    f.write(".synthesis/drafts/.gitignore", "# the drafts root's own\n");

    // The author has staged work of their own, mid-compose.
    f.write("src/main.rs", "fn main() {}\n");
    let repo = f.repo();
    let mut index = repo.index().expect("index");
    index.add_path(Path::new("src/main.rs")).expect("stage");
    index.write().expect("write");

    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), None, "the commit is taken");
    assert_eq!(f.head_message(), "draft: create \"A draft\"", "the message names the event");
    for owned in [
        format!(".synthesis/drafts/{ID}/draft.toml"),
        format!(".synthesis/drafts/{ID}/files/Prompt.md"),
        ".synthesis/drafts/.gitattributes".to_string(),
        ".synthesis/drafts/.gitignore".to_string(),
    ] {
        assert!(f.head_holds(&owned), "{owned} is in the commit");
    }
    assert!(
        !f.head_holds("src/main.rs"),
        "the author's staged file is in no commit this module wrote"
    );
    assert_eq!(
        f.dirty(),
        vec!["src/main.rs".to_string()],
        "the draft is clean and the author's work is still uncommitted"
    );
    let staged = f
        .repo()
        .index()
        .expect("index")
        .get_path(Path::new("src/main.rs"), 0)
        .is_some();
    assert!(staged, "and it is still staged");
}

// DRS-FR-21 / DRS-FR-VECL / DRS-FR-PIWL: the deletion event names every path
// Git tracks under the deleted draft's folder, a private one an earlier build
// committed included, and records each as a deletion.
#[test]
fn a_deletion_event_records_every_tracked_path_of_its_draft_as_deleted() {
    let f = Fixture::new();
    f.write(&format!(".synthesis/drafts/UI/{ID}/draft.toml"), "id = \"one\"\n");
    f.write(&format!(".synthesis/drafts/UI/{ID}/proposals/p.toml"), "state = \"pending\"\n");
    f.write(".synthesis/drafts/UI/1a2b3c4d5e6-0002-cafebabe/draft.toml", "id = \"two\"\n");
    commit_all(&f.repo(), "an earlier build committed both drafts whole");

    std::fs::remove_dir_all(f.root.path().join(format!(".synthesis/drafts/UI/{ID}")))
        .expect("the draft goes");
    assert_eq!(commit_once(&f.root, ID, DraftEvent::Deleted), None, "the deletion is committed");
    assert_eq!(f.head_message(), "draft: delete \"A draft\"");
    assert!(!f.head_holds(&format!(".synthesis/drafts/UI/{ID}/draft.toml")));
    assert!(!f.head_holds(&format!(".synthesis/drafts/UI/{ID}/proposals/p.toml")));
    assert!(
        f.head_holds(".synthesis/drafts/UI/1a2b3c4d5e6-0002-cafebabe/draft.toml"),
        "another draft is untouched"
    );
    assert!(f.dirty().is_empty(), "and nothing of the deletion is left behind");
}

/// PST-FR-KGRW (PST-FR-KGRW): every condition under which committing would
/// be wrong rather than merely unnecessary is a silent no-op.
#[test]
fn pst_ts_zmbx_a_repository_that_is_not_ready_is_a_silent_no_op() {
    // A content root in no Git repository.
    let plain = TempDir::new().expect("tempdir");
    let root = RootFs::for_root(plain.path());
    std::fs::create_dir_all(plain.path().join(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef")).expect("the folder");
    std::fs::write(plain.path().join(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml"), "id = \"a\"\n").expect("a draft");
    assert_eq!(commit_once(&root, ID, DraftEvent::Created), Some(Skip::NotARepository.reason()));

    // A repository with no commit yet: this must not become the project's
    // first commit, nor create its branch.
    let unborn = TempDir::new().expect("tempdir");
    let repo = git2::Repository::init(unborn.path()).expect("repo");
    let mut config = repo.config().expect("config");
    config.set_str("user.name", "An Author").expect("name");
    config.set_str("user.email", "author@example.com").expect("email");
    let root = RootFs::for_root(unborn.path());
    std::fs::create_dir_all(unborn.path().join(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef")).expect("the folder");
    std::fs::write(unborn.path().join(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml"), "id = \"a\"\n").expect("a draft");
    assert_eq!(commit_once(&root, ID, DraftEvent::Created), Some(Skip::NoCommitYet.reason()));
    assert!(repo.head().is_err(), "and the repository is still unborn");

    // A repository holding an operation the author is in the middle of.
    let f = Fixture::new();
    f.write(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml", "id = \"one\"\n");
    std::fs::write(f.repo().path().join("MERGE_HEAD"), "0".repeat(40)).expect("a merge");
    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), Some(Skip::RepositoryBusy.reason()));

    // The index is held by another writer.
    std::fs::remove_file(f.repo().path().join("MERGE_HEAD")).expect("the merge ends");
    let repo = f.repo();
    let held = crate::git::index_lock::IndexLock::try_acquire(&repo)
        .expect("the instance builds")
        .expect("the lock is free");
    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), Some(Skip::IndexHeld.reason()));
    drop(held);

    // PST-FR-KGRW / DRS-FR-JPVB: an index holding a conflict in a
    // repository Git calls clean — what a conflicted `git stash pop` leaves
    // — commits nothing, so no conflicted file is staged with its markers
    // in it and neither side is chosen.
    let repo = f.repo();
    let conflicted = std::path::Path::new("contested.md");
    std::fs::write(f.root.path().join(conflicted), "ours\n").expect("a file");
    {
        let mut index = repo.index().expect("the index");
        let blob = repo.blob(b"theirs\n").expect("a blob");
        for stage in [2u16, 3u16] {
            index
                .add(&git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    file_size: 0,
                    id: blob,
                    flags: stage << 12,
                    flags_extended: 0,
                    path: conflicted.to_string_lossy().as_bytes().to_vec(),
                })
                .expect("a conflicted entry");
        }
        index.write().expect("a written index");
    }
    assert!(repo.index().expect("the index").has_conflicts());
    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), Some(Skip::ConflictInIndex.reason()));
    {
        let mut index = repo.index().expect("the index");
        index.remove_all(["contested.md"], None).expect("the conflict is cleared");
        index.write().expect("a written index");
    }
    std::fs::remove_file(f.root.path().join(conflicted)).expect("the file goes");

    // Nothing that differs from `HEAD` is the ordinary quiet case.
    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), None, "the pending draft commits");
    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), Some(Skip::NothingToCommit.reason()));
}

/// PST-FR-KGRW: a detached `HEAD` is committed to rather than skipped, and
/// the commit moves that `HEAD` alone.
#[test]
fn a_detached_head_is_committed_to_and_no_branch_moves() {
    let f = Fixture::new();
    let repo = f.repo();
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .expect("a head")
        .id();
    let branch_before = repo
        .find_branch("master", git2::BranchType::Local)
        .or_else(|_| repo.find_branch("main", git2::BranchType::Local))
        .expect("a branch")
        .get()
        .peel_to_commit()
        .expect("a commit")
        .id();
    repo.set_head_detached(head).expect("detach");

    f.write(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml", "id = \"one\"\n");
    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), None, "a detached head is committed to");

    let repo = f.repo();
    assert!(repo.head_detached().expect("detached"), "and stays detached");
    let branch_after = repo
        .find_branch("master", git2::BranchType::Local)
        .or_else(|_| repo.find_branch("main", git2::BranchType::Local))
        .expect("a branch")
        .get()
        .peel_to_commit()
        .expect("a commit")
        .id();
    assert_eq!(branch_before, branch_after, "no branch moved");
}

// PST-FR-TYNC / DRS-FR-VECL / DRS-FR-WYIN: the commit names the committed
// storage of its one draft and nothing else, however dirty the rest of the
// worktree is — not that draft's private storage, and not another draft.
#[test]
fn the_commit_names_its_draft_and_nothing_else() {
    let f = Fixture::new();
    f.write(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml", "id = \"one\"\n");
    f.write(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/proposals/p.toml", "state = \"pending\"\n");
    f.write(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/history/h.snapshot", "a version\n");
    f.write(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/publication.toml", "[attempt]\n");
    f.write(".synthesis/drafts/1a2b3c4d5e6-0002-cafebabe/draft.toml", "id = \"two\"\n");
    f.write(".synthesis/notes/half-written.md", "an unfinished thought\n");
    f.write(".synthesis/drafts-old/1a0/draft.toml", "id = \"old\"\n");
    // The folders an earlier build committed its logs into are no longer the
    // floor's: they are the author's to remove (RMS-FR-XRPT).
    f.write(".synthesis/statistics/1a0.jsonl", "{\"v\":1}\n");
    f.write(".synthesis/comments/abc.jsonl", "{\"v\":1}\n");
    f.write("src/main.rs", "fn main() {}\n");

    assert_eq!(commit_once(&f.root, ID, DraftEvent::Created), None);
    assert!(f.head_holds(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml"));
    for authored in [
        ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/proposals/p.toml",
        ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/history/h.snapshot",
        ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/publication.toml",
        ".synthesis/drafts/1a2b3c4d5e6-0002-cafebabe/draft.toml",
        ".synthesis/notes/half-written.md",
        ".synthesis/drafts-old/1a0/draft.toml",
        ".synthesis/statistics/1a0.jsonl",
        ".synthesis/comments/abc.jsonl",
        "src/main.rs",
    ] {
        assert!(!f.head_holds(authored), "{authored} is in no commit of this event");
    }
}

// PST-FR-KGRW: the same reason is recorded once for a root rather than once
// per event, and is reported again once a commit has intervened.
#[test]
fn a_reason_is_recorded_once_rather_than_once_per_event() {
    let f = Fixture::new();
    f.write(".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml", "id = \"one\"\n");
    std::fs::write(f.repo().path().join("MERGE_HEAD"), "0".repeat(40)).expect("a merge");

    let (first, records) = commit_once_recording(&f.root, ID, DraftEvent::Created, None);
    assert_eq!(first, Some(Skip::RepositoryBusy.reason()));
    assert_eq!(records.len(), 1, "the first time it is recorded");

    let (second, records) = commit_once_recording(&f.root, ID, DraftEvent::Created, first);
    assert_eq!(second, first, "the reason has not changed");
    assert!(records.is_empty(), "and it is not recorded again");

    // A different reason for the same root is a new thing to say.
    std::fs::remove_file(f.repo().path().join("MERGE_HEAD")).expect("the merge ends");
    let repo = f.repo();
    let held = crate::git::index_lock::IndexLock::try_acquire(&repo)
        .expect("the instance builds")
        .expect("the lock is free");
    let (third, records) = commit_once_recording(&f.root, ID, DraftEvent::Created, second);
    assert_eq!(third, Some(Skip::IndexHeld.reason()));
    assert_eq!(records.len(), 1, "a different reason is recorded");
    drop(held);

    // A commit forgets the reason, so the same one is reported again.
    let (taken, records) = commit_once_recording(&f.root, ID, DraftEvent::Created, third);
    assert_eq!(taken, None, "the commit is taken");
    assert_eq!(records.len(), 1, "and the commit itself is recorded");
}

/// PST-FR-KGRW: a record carries a count and a reason, and no path, no
/// draft identity, and no file content.
///
/// The failure prose of `commit_named_paths` names the pathspec it stopped
/// on, and every path this module commits is named for a draft — so a
/// forwarded message would put a draft's identity into a record that
/// travels to the Logs panel, the clipboard, and any exported file.
#[test]
fn a_failure_reason_carries_no_path_and_no_draft_identity() {
    let leaky = [
        "failed to stage .synthesis/statistics/1a03545f600-0001-af6cbf93.jsonl: permission denied",
        "failed to record the deletion of .synthesis/comments/1a0-abc.jsonl: no such file",
    ];
    for message in leaky {
        let reason = failure_reason(message);
        assert!(
            !reason.contains(".jsonl")
                && !reason.contains('/')
                && !reason.contains("1a0"),
            "{reason} still carries the path from {message}"
        );
    }
    assert_eq!(
        failure_reason(leaky[0]), "stage_failed",
        "and it still says which stage failed"
    );
    assert_eq!(failure_reason(leaky[1]), "deletion_failed");
    assert_eq!(
        failure_reason(crate::git::ERR_NOTHING_TO_COMMIT),
        "nothing_to_commit"
    );
    assert_eq!(
        failure_reason("something nobody anticipated: /Users/someone/secret"),
        "commit_failed",
        "an unrecognised message is a token rather than a passthrough"
    );
}

/// Every skip reason is a fixed token too, for the same reason.
#[test]
fn a_skip_reason_is_a_fixed_token() {
    for skip in [
        Skip::RootUnavailable,
        Skip::NotARepository,
        Skip::BareRepository,
        Skip::ConflictInIndex,
        Skip::RepositoryBusy,
        Skip::NoCommitYet,
        Skip::NoAuthoringIdentity,
        Skip::IndexHeld,
        Skip::NothingToCommit,
        Skip::StatusUnreadable,
    ] {
        let reason = skip.reason();
        assert!(
            !reason.contains('/') && !reason.contains('.'),
            "{reason} is a token rather than a path"
        );
    }
}

// PST-FR-BIPA / PST-FR-YWXF: events are committed one at a time, in the order
// they occurred, and two events never share one commit.
#[test]
fn events_are_committed_one_at_a_time_in_order() {
    let f = Fixture::new();
    let before = f
        .repo()
        .head()
        .and_then(|h| h.peel_to_commit())
        .expect("a head")
        .id();
    let second = "1a2b3c4d5e6-0002-cafebabe";
    f.write(&format!(".synthesis/drafts/{ID}/draft.toml"), "id = \"one\"\n");
    draft_event(&f.root, ID, "First", DraftEvent::Created);
    f.write(&format!(".synthesis/drafts/{second}/draft.toml"), "id = \"two\"\n");
    draft_event(&f.root, second, "Second", DraftEvent::GraduationStarted);
    assert_eq!(pending_events(&f.root).len(), 2, "both events wait in the queue");

    wait_for_committer(&f.root);

    let repo = f.repo();
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .expect("a head");
    assert_eq!(head.message().unwrap_or_default(), "draft: graduate \"Second\"");
    let parent = head.parent(0).expect("a parent");
    assert_eq!(parent.message().unwrap_or_default(), "draft: create \"First\"");
    assert_eq!(parent.parent_id(0).expect("a grandparent"), before, "two events, two commits");
    assert!(f.dirty().is_empty(), "and the worktree is clean");
    assert!(pending_events(&f.root).is_empty(), "and the queue is drained");
}

// PST-FR-YWXF: the message is the event and the draft's current name, and
// nothing else — a line break in a name does not open a commit body.
#[test]
fn the_message_names_the_event_and_the_draft() {
    assert_eq!(message(DraftEvent::Created, "Push button"), "draft: create \"Push button\"");
    assert_eq!(message(DraftEvent::Deleted, "Push button"), "draft: delete \"Push button\"");
    assert_eq!(
        message(DraftEvent::GraduationStarted, "Push button"),
        "draft: graduate \"Push button\""
    );
    assert_eq!(message(DraftEvent::Created, "two\nlines"), "draft: create \"two lines\"");
}

// DRS-FR-VECL / DRS-FR-32: a draft moved between folders since its last commit
// is recorded by its next event as gone from the old folder and present in the
// new one, so the history never holds one draft at two locations.
#[test]
fn a_moved_draft_is_committed_at_its_new_location_and_removed_from_the_old() {
    let f = Fixture::new();
    f.write(&format!(".synthesis/drafts/A/{ID}/draft.toml"), "id = \"one\"\n");
    f.write(&format!(".synthesis/drafts/A/{ID}/files/Prompt.md"), "the prompt\n");
    commit_all(&f.repo(), "the draft as it was filed");
    std::fs::create_dir_all(f.root.path().join(".synthesis/drafts/B")).expect("the folder");
    std::fs::rename(
        f.root.path().join(format!(".synthesis/drafts/A/{ID}")),
        f.root.path().join(format!(".synthesis/drafts/B/{ID}")),
    )
    .expect("the move");

    assert_eq!(commit_once(&f.root, ID, DraftEvent::GraduationStarted), None, "the commit is taken");
    for gone in [
        format!(".synthesis/drafts/A/{ID}/draft.toml"),
        format!(".synthesis/drafts/A/{ID}/files/Prompt.md"),
    ] {
        assert!(!f.head_holds(&gone), "{gone} is removed");
    }
    for present in [
        format!(".synthesis/drafts/B/{ID}/draft.toml"),
        format!(".synthesis/drafts/B/{ID}/files/Prompt.md"),
    ] {
        assert!(f.head_holds(&present), "{present} is committed");
    }
    assert!(f.dirty().is_empty(), "and nothing of the move is left behind: {:?}", f.dirty());
}

// PST-FR-TYNC / DRS-FR-VECL: Git would pair a deleted draft with another
// draft of similar content as a rename. The deletion of one draft records its
// own deletion and carries nothing of the other.
#[test]
fn a_deletion_is_not_paired_with_another_draft() {
    let f = Fixture::new();
    let other = "1a2b3c4d5e6-0002-cafebabe";
    f.write(&format!(".synthesis/drafts/{ID}/draft.toml"), "id = \"x\"\nname = \"Untitled\"\n");
    f.write(&format!(".synthesis/drafts/{ID}/files/Untitled.md"), "## Intent\n\n## Requirements\n");
    commit_all(&f.repo(), "draft A");
    std::fs::remove_dir_all(f.root.path().join(format!(".synthesis/drafts/{ID}"))).unwrap();
    f.write(&format!(".synthesis/drafts/{other}/draft.toml"), "id = \"x\"\nname = \"Untitled\"\n");
    f.write(&format!(".synthesis/drafts/{other}/files/Untitled.md"), "## Intent\n\n## Requirements\n");

    assert_eq!(commit_once(&f.root, ID, DraftEvent::Deleted), None, "the deletion is committed");
    assert!(!f.head_holds(&format!(".synthesis/drafts/{ID}/draft.toml")));
    assert!(
        !f.head_holds(&format!(".synthesis/drafts/{other}/draft.toml")),
        "the other draft is in no commit of this event",
    );
}

// PST-FR-TYNC / DRS-FR-VECL / DRS-FR-21: a creation commits its own draft and
// records no deletion it would be paired with — another draft's, or a file of
// the author's with the same text.
#[test]
fn a_creation_records_no_deletion_it_is_paired_with() {
    let f = Fixture::new();
    let other = "1a2b3c4d5e6-0002-cafebabe";
    let text = "## Intent\n\nA long enough body for Git to pair the two files.\n";
    f.write(&format!(".synthesis/drafts/{ID}/files/Untitled.md"), text);
    f.write("docs/plan.md", text);
    commit_all(&f.repo(), "a draft and the author's plan");
    std::fs::remove_dir_all(f.root.path().join(format!(".synthesis/drafts/{ID}"))).unwrap();
    std::fs::remove_file(f.root.path().join("docs/plan.md")).unwrap();
    f.write(&format!(".synthesis/drafts/{other}/files/Untitled.md"), text);

    assert_eq!(commit_once(&f.root, other, DraftEvent::Created), None, "the creation is committed");
    assert!(f.head_holds(&format!(".synthesis/drafts/{other}/files/Untitled.md")));
    assert!(
        f.head_holds(&format!(".synthesis/drafts/{ID}/files/Untitled.md")),
        "another draft's deletion is not recorded by this event",
    );
    assert!(f.head_holds("docs/plan.md"), "the author's deletion is theirs to commit");
    assert!(f.dirty().contains(&"docs/plan.md".to_string()), "and it still stands uncommitted");
}

