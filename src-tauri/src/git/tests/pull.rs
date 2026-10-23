//! The pull primitive (GTC-FR-QHMT .. GTC-FR-IHCE): what it fetches, what it
//! compares, what it lands, and what it refuses.
//!
//! Every scenario clones a real local repository over libgit2's `file://`
//! transport, so the fetch inside the pull is a genuine one and no network is
//! reached.

use super::*;
use crate::worktree::{refresh_outcome_for, RefreshOutcome, REMOTE_FAILED, REMOTE_REFRESHED};

/// An upstream repository the tests commit into, and a clone of it the pull
/// runs in. The clone's branch tracks the upstream's branch.
struct PullFixture {
    upstream: Fixture,
    _clone_dir: TempDir,
    clone: PathBuf,
}

impl PullFixture {
    fn new() -> PullFixture {
        let upstream = Fixture::new();
        upstream.write("a.md", "one\n");
        upstream.write("c.md", "three\n");
        upstream.commit("base");
        let clone_dir = TempDir::new().unwrap();
        let clone = clone_dir.path().join("clone");
        let url = format!("file://{}", upstream.root().to_string_lossy());
        let repo = Repository::clone(&url, &clone).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        PullFixture { upstream, _clone_dir: clone_dir, clone }
    }

    fn repo(&self) -> Repository {
        Repository::open(&self.clone).unwrap()
    }

    fn branch(&self) -> String {
        self.upstream.current_branch()
    }

    fn head(&self) -> Oid {
        self.repo().head().unwrap().peel_to_commit().unwrap().id()
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.clone.join(rel)).unwrap_or_default()
    }

    fn write(&self, rel: &str, contents: &str) {
        std::fs::write(self.clone.join(rel), contents).unwrap();
    }

    /// Commit every change in the clone onto its branch.
    fn commit_local(&self, message: &str) -> Oid {
        let repo = self.repo();
        let mut index = repo.index().unwrap();
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent]).unwrap()
    }

    /// The fetch the pull composes, against this clone.
    fn refresh(&self) -> Result<RefreshOutcome, String> {
        let root = self.clone.clone();
        refresh_outcome_for(&root, || {
            fetch_remote_branches(
                &CollectingSink::default(),
                &SCRATCH_BUFFER,
                &ProgressRegistry::default(),
                &root,
                &GlobalSettingsStore::in_memory(),
                &GithubTokens::default(),
                "",
            )
        })
    }
}

/// What one pull answered and emitted.
struct Pulled {
    outcome: PullOutcome,
    events: Vec<(String, serde_json::Value)>,
}

impl Pulled {
    fn finished(&self) -> Vec<&serde_json::Value> {
        self.events
            .iter()
            .filter(|(name, _)| name == GIT_OPERATION_FINISHED)
            .map(|(_, payload)| payload)
            .collect()
    }

    fn lines(&self) -> Vec<String> {
        self.events
            .iter()
            .filter(|(name, _)| name == GIT_OUTPUT_LINE)
            .map(|(_, payload)| payload["line"].as_str().unwrap_or_default().to_string())
            .collect()
    }
}

fn pull_with(
    f: &PullFixture,
    refresh: impl FnOnce() -> Result<RefreshOutcome, String>,
    guard: impl FnOnce(&str) -> Result<(), String>,
) -> Pulled {
    let events: RefCell<Vec<(String, serde_json::Value)>> = RefCell::new(Vec::new());
    let emit = |name: &str, payload: serde_json::Value| {
        events.borrow_mut().push((name.to_string(), payload));
    };
    let outcome = pull_branch_at(
        &CollectingSink::default(),
        &SCRATCH_BUFFER,
        &emit,
        &ProgressRegistry::default(),
        &f.clone,
        refresh,
        guard,
    )
    .unwrap();
    Pulled { outcome, events: events.into_inner() }
}

fn pull(f: &PullFixture) -> Pulled {
    pull_with(f, || f.refresh(), |_| Ok(()))
}

// GTC-FR-KDRW, GTC-FR-QHMT: a branch level with its upstream is up to date,
// and the outcome carries the refresh it ran first.
#[test]
fn a_branch_level_with_its_upstream_is_up_to_date() {
    let f = PullFixture::new();
    let before = f.head();
    let pulled = pull(&f);
    assert_eq!(pulled.outcome.state, PULL_UP_TO_DATE);
    assert_eq!((pulled.outcome.ahead, pulled.outcome.behind), (0, 0));
    assert_eq!(pulled.outcome.refresh.remote_state, REMOTE_REFRESHED);
    assert_eq!(pulled.outcome.branch.as_deref(), Some(f.branch().as_str()));
    assert_eq!(pulled.outcome.upstream, Some(format!("origin/{}", f.branch())));
    assert_eq!(f.head(), before);
    assert!(pulled.lines().iter().any(|l| l == "Already up to date"), "{:?}", pulled.lines());
}

// GTC-FR-KDRW, GTC-FR-OYDA, GTC-FR-04: a branch behind its upstream moves to the
// upstream tip with its files, and the pull streams its lines and ends once.
#[test]
fn a_branch_behind_its_upstream_is_fast_forwarded_with_its_files() {
    let f = PullFixture::new();
    let before = f.head();
    f.upstream.write("a.md", "one\ntwo\n");
    f.upstream.write("b.md", "new\n");
    let remote_tip = f.upstream.commit("upstream");

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.state, PULL_FAST_FORWARDED, "{:?}", pulled.outcome);
    assert_eq!(f.head(), remote_tip);
    assert_eq!(f.read("a.md"), "one\ntwo\n");
    assert_eq!(f.read("b.md"), "new\n");
    assert_eq!(pulled.outcome.updated_paths, vec!["a.md".to_string(), "b.md".to_string()]);
    assert_eq!(pulled.outcome.local_tip, Some(before.to_string()));
    assert_eq!(pulled.outcome.remote_tip, Some(remote_tip.to_string()));
    // The landing leaves the checkout clean: the index moved with the files.
    assert!(crate::streams::checked_out_uncommitted_paths_of(&f.clone).is_empty());

    let finished = pulled.finished();
    assert_eq!(finished.len(), 1, "exactly one terminal event");
    assert_eq!(finished[0]["operation"], serde_json::json!("pull"));
    assert_eq!(finished[0]["ok"], serde_json::json!(true));
    assert!(pulled
        .events
        .iter()
        .filter(|(name, _)| name == GIT_OUTPUT_LINE)
        .all(|(_, payload)| payload["operation"] == serde_json::json!("pull")));
    let lines = pulled.lines();
    assert!(lines.iter().any(|l| l.starts_with("Pulling ")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("Fast-forwarded ") && l.ends_with("(1 commit)")));
}

// GTC-FR-VOXB: an uncommitted path the pull does not change keeps its content
// and its index state, and an untracked file stays where it is.
#[test]
fn uncommitted_work_the_pull_does_not_change_survives_a_fast_forward() {
    let f = PullFixture::new();
    f.write("c.md", "three, edited\n");
    f.write("untracked.md", "mine\n");
    f.upstream.write("a.md", "one\ntwo\n");
    let remote_tip = f.upstream.commit("upstream");

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.state, PULL_FAST_FORWARDED, "{:?}", pulled.outcome);
    assert_eq!(f.head(), remote_tip);
    assert_eq!(f.read("a.md"), "one\ntwo\n");
    assert_eq!(f.read("c.md"), "three, edited\n");
    assert_eq!(f.read("untracked.md"), "mine\n");
    let dirty = crate::streams::checked_out_uncommitted_paths_of(&f.clone);
    assert_eq!(dirty, vec!["c.md".to_string(), "untracked.md".to_string()]);
}

// GTC-FR-VOXB: a modified path the pull would overwrite refuses the pull with
// that path, and nothing is written.
#[test]
fn a_modified_path_the_pull_changes_refuses_it_and_writes_nothing() {
    let f = PullFixture::new();
    let before = f.head();
    f.write("a.md", "one, mine\n");
    f.upstream.write("a.md", "one\ntwo\n");
    f.upstream.commit("upstream");

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.state, PULL_REFUSED);
    assert_eq!(pulled.outcome.error, Some(format!("{ERR_PULL_WOULD_OVERWRITE}: a.md")));
    assert_eq!(f.head(), before);
    assert_eq!(f.read("a.md"), "one, mine\n");
    let finished = pulled.finished();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0]["ok"], serde_json::json!(false));
}

// GTC-FR-VOXB: an untracked file at a path the pull adds refuses it too.
#[test]
fn an_untracked_file_where_the_pull_adds_one_refuses_it() {
    let f = PullFixture::new();
    let before = f.head();
    f.write("b.md", "mine\n");
    f.upstream.write("b.md", "theirs\n");
    f.upstream.commit("upstream");

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.error, Some(format!("{ERR_PULL_WOULD_OVERWRITE}: b.md")));
    assert_eq!(f.head(), before);
    assert_eq!(f.read("b.md"), "mine\n");
}

// GTC-FR-KDRW: a branch that holds commits of its own and lacks the upstream's
// is diverged: both tips and both counts are reported, and nothing is written.
#[test]
fn a_diverged_branch_is_reported_and_not_written() {
    let f = PullFixture::new();
    f.write("c.md", "local\n");
    let local_tip = f.commit_local("local");
    f.upstream.write("a.md", "remote\n");
    let remote_tip = f.upstream.commit("upstream one");
    f.upstream.write("b.md", "remote\n");
    let remote_tip_2 = f.upstream.commit("upstream two");

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.state, PULL_DIVERGED);
    assert_eq!((pulled.outcome.ahead, pulled.outcome.behind), (1, 2));
    assert_eq!(pulled.outcome.local_tip, Some(local_tip.to_string()));
    assert_eq!(pulled.outcome.remote_tip, Some(remote_tip_2.to_string()));
    assert_ne!(remote_tip, remote_tip_2);
    assert_eq!(f.head(), local_tip);
    assert_eq!(f.read("a.md"), "one\n");
    assert_eq!(pulled.finished()[0]["ok"], serde_json::json!(true));
}

// GTC-FR-ZWAE: a branch that tracks nothing is refused after the fetch.
#[test]
fn a_branch_with_no_upstream_is_refused_after_the_fetch() {
    let f = PullFixture::new();
    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("topic", &head, false).unwrap();
    repo.set_head("refs/heads/topic").unwrap();

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.state, PULL_REFUSED);
    assert_eq!(pulled.outcome.error.as_deref(), Some(ERR_NO_UPSTREAM));
    assert_eq!(pulled.outcome.refresh.remote_state, REMOTE_REFRESHED);
}

// GTC-FR-ZWAE: a detached `HEAD` names no branch.
#[test]
fn a_detached_head_is_refused() {
    let f = PullFixture::new();
    let repo = f.repo();
    repo.set_head_detached(f.head()).unwrap();

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.error.as_deref(), Some(ERR_NO_BRANCH_CHECKED_OUT));
}

// GTC-FR-ZWAE: an upstream whose remote-tracking ref is absent is gone.
#[test]
fn an_upstream_whose_ref_is_absent_is_gone() {
    let f = PullFixture::new();
    let repo = f.repo();
    let mut config = repo.config().unwrap();
    config.set_str(&format!("branch.{}.merge", f.branch()), "refs/heads/vanished").unwrap();

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.error.as_deref(), Some(ERR_UPSTREAM_GONE));
}

// GTC-FR-ZWAE: an upstream on a remote other than the primary one is refused,
// and so is one that tracks a local branch.
#[test]
fn an_upstream_off_the_primary_remote_is_refused() {
    let f = PullFixture::new();
    let repo = f.repo();
    let url = format!("file://{}", f.upstream.root().to_string_lossy());
    repo.remote("mirror", &url).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str(&format!("branch.{}.remote", f.branch()), "mirror").unwrap();
    let pulled = pull(&f);
    assert_eq!(pulled.outcome.error.as_deref(), Some(ERR_UPSTREAM_NOT_ON_PRIMARY_REMOTE));

    config.set_str(&format!("branch.{}.remote", f.branch()), ".").unwrap();
    let pulled = pull(&f);
    assert_eq!(pulled.outcome.error.as_deref(), Some(ERR_UPSTREAM_NOT_ON_PRIMARY_REMOTE));
}

// GTC-FR-QHMT: a skipped remote leg is "no remote configured", and a failed one
// is `fetch_failed` with that leg's cause. Neither integrates anything.
#[test]
fn a_remote_leg_that_did_not_refresh_integrates_nothing() {
    let f = PullFixture::new();
    let before = f.head();
    f.upstream.write("a.md", "two\n");
    f.upstream.commit("upstream");
    let root = f.clone.clone();

    let skipped = pull_with(
        &f,
        || refresh_outcome_for(&root, || Err(ERR_NO_REMOTE_CONFIGURED.into())),
        |_| Ok(()),
    );
    assert_eq!(skipped.outcome.state, PULL_REFUSED);
    assert_eq!(skipped.outcome.error.as_deref(), Some(ERR_NO_REMOTE_CONFIGURED));

    let unreachable = github_tokens::ERR_GITHUB_UNREACHABLE.to_string();
    let failed =
        pull_with(&f, || refresh_outcome_for(&root, || Err(unreachable.clone())), |_| Ok(()));
    assert_eq!(failed.outcome.state, PULL_FETCH_FAILED);
    assert_eq!(failed.outcome.refresh.remote_state, REMOTE_FAILED);
    assert_eq!(failed.outcome.error, Some(unreachable));
    assert_eq!(failed.finished()[0]["ok"], serde_json::json!(false));
    assert_eq!(f.head(), before);
}

// GTC-FR-TRWC: a guard that refuses stops the fast-forward before any write.
#[test]
fn a_held_guard_refuses_the_fast_forward_and_writes_nothing() {
    let f = PullFixture::new();
    let before = f.head();
    f.upstream.write("a.md", "two\n");
    f.upstream.commit("upstream");

    let pulled =
        pull_with(&f, || f.refresh(), |_| Err(crate::streams::ERR_PULL_IN_PROGRESS.into()));

    assert_eq!(pulled.outcome.state, PULL_REFUSED);
    assert_eq!(pulled.outcome.error.as_deref(), Some(crate::streams::ERR_PULL_IN_PROGRESS));
    assert_eq!(f.head(), before);
    assert_eq!(f.read("a.md"), "one\n");
}

// GTC-FR-VOXB: a landing that cannot write the index moves no branch and no
// file.
#[test]
fn a_landing_that_cannot_write_the_index_moves_nothing() {
    let f = PullFixture::new();
    let before = f.head();
    f.upstream.write("a.md", "two\n");
    let remote_tip = f.upstream.commit("upstream");
    let refreshed = f.refresh().unwrap();
    assert_eq!(refreshed.remote_state, REMOTE_REFRESHED);
    std::fs::write(f.clone.join(".git").join("index.lock"), "").unwrap();

    let repo = f.repo();
    let error = land_on_branch(&repo, &f.branch(), before, remote_tip).unwrap_err();

    assert!(!error.starts_with(ERR_PULL_WOULD_OVERWRITE), "{error}");
    assert_eq!(f.head(), before);
    assert_eq!(f.read("a.md"), "one\n");
}

// GTC-FR-IHCE: a pull reports under `kind = "git"` with the branch in its label
// and no activation.
#[test]
fn a_pull_reports_itself_as_a_git_operation_named_for_its_branch() {
    let f = PullFixture::new();
    let sink = CollectingSink::default();
    let emit = |_name: &str, _payload: serde_json::Value| {};
    pull_branch_at(
        &sink,
        &SCRATCH_BUFFER,
        &emit,
        &ProgressRegistry::default(),
        &f.clone,
        || f.refresh(),
        |_| Ok(()),
    )
    .unwrap();
    let published = sink.published.lock().unwrap().clone();
    let pulls: Vec<_> = published.iter().filter(|op| op.label.starts_with("Pulling")).collect();
    assert!(!pulls.is_empty(), "{published:?}");
    assert!(pulls.iter().all(|op| op.kind == PROGRESS_KIND_GIT));
    assert!(pulls.iter().all(|op| op.label == format!("Pulling {}", f.branch())));
    assert!(pulls.iter().all(|op| op.activation.is_none()));
}

// GTC-FR-QHMT: a root outside a repository is the one error the call returns,
// and the terminal event still ends the transfer.
#[test]
fn a_root_outside_a_repository_is_the_typed_error() {
    let dir = TempDir::new().unwrap();
    let events: RefCell<Vec<(String, serde_json::Value)>> = RefCell::new(Vec::new());
    let emit = |name: &str, payload: serde_json::Value| {
        events.borrow_mut().push((name.to_string(), payload));
    };
    let error = pull_branch_at(
        &CollectingSink::default(),
        &SCRATCH_BUFFER,
        &emit,
        &ProgressRegistry::default(),
        dir.path(),
        || refresh_outcome_for(dir.path(), || Ok(())),
        |_| Ok(()),
    )
    .unwrap_err();
    assert_eq!(error, ERR_NOT_A_REPO);
    let finished = events.borrow().iter().filter(|(n, _)| n == GIT_OPERATION_FINISHED).count();
    assert_eq!(finished, 1);
}

// GTC-FR-YLTM, GTC-FR-VOXB: a landing whose branch moved since its tip was read
// is refused before any file is written, so the new commit's files stand.
#[test]
fn a_landing_on_a_branch_that_moved_writes_nothing() {
    let f = PullFixture::new();
    let read_tip = f.head();
    f.upstream.write("a.md", "two\n");
    let remote_tip = f.upstream.commit("upstream");
    f.refresh().unwrap();
    f.write("c.md", "committed meanwhile\n");
    let moved = f.commit_local("a commit made after the tip was read");

    let repo = f.repo();
    let error = land_on_branch(&repo, &f.branch(), read_tip, remote_tip).unwrap_err();

    assert_eq!(error, ERR_PULL_TIPS_MOVED);
    assert_eq!(f.head(), moved);
    assert_eq!(f.read("c.md"), "committed meanwhile\n");
    assert_eq!(f.read("a.md"), "one\n");
    assert!(crate::streams::checked_out_uncommitted_paths_of(&f.clone).is_empty());
}

// GTC-FR-VOXB: a staged-only change and a deletion the pull would overwrite
// refuse it too.
#[test]
fn a_staged_or_deleted_path_the_pull_changes_refuses_it() {
    let f = PullFixture::new();
    let before = f.head();
    f.upstream.write("a.md", "one\ntwo\n");
    f.upstream.write("c.md", "three\nfour\n");
    f.upstream.commit("upstream");
    // a.md: staged, then the working file put back, so only the index differs.
    f.write("a.md", "staged\n");
    let repo = f.repo();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.md")).unwrap();
    index.write().unwrap();
    f.write("a.md", "one\n");
    // c.md: deleted from the working tree.
    std::fs::remove_file(f.clone.join("c.md")).unwrap();

    let pulled = pull(&f);

    assert_eq!(pulled.outcome.error, Some(format!("{ERR_PULL_WOULD_OVERWRITE}: a.md, c.md")));
    assert_eq!(f.head(), before);
    assert!(!f.clone.join("c.md").exists());
}
