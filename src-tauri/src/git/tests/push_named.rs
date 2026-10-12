//! The push of a named branch (GTC-FR-YHDU / GTC-FR-RZNQ / GTC-FR-KUVE): the
//! branch it publishes, the refusals it makes before any transport, and the
//! checkout it runs from.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;
use tauri::Manager;

/// A bare repository that `origin` of `f` points at.
fn bare_origin(f: &Fixture) -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let bare = Repository::init_bare(dir.path()).unwrap();
    f.repo().remote("origin", dir.path().to_str().unwrap()).unwrap();
    (dir, bare)
}

/// Pushes `branch` from `root` with no token store, and returns the outcome
/// and every event the push emitted.
fn push_named(
    root: &std::path::Path,
    store: &GlobalSettingsStore,
    branch: &str,
) -> (Result<(), String>, Vec<(String, serde_json::Value)>) {
    let emitted: RefCell<Vec<(String, serde_json::Value)>> = RefCell::new(Vec::new());
    let emit = |name: &str, payload: serde_json::Value| {
        emitted.borrow_mut().push((name.to_string(), payload));
    };
    let outcome = push_named_branch_at(
        &CollectingSink::default(),
        &SCRATCH_BUFFER,
        &emit,
        &ProgressRegistry::default(),
        GithubAccess { root, store, tokens: &GithubTokens::default(), project_key: "key" },
        branch,
    );
    (outcome, emitted.into_inner())
}

fn finished(events: &[(String, serde_json::Value)]) -> Vec<&serde_json::Value> {
    events.iter().filter(|(name, _)| name == GIT_OPERATION_FINISHED).map(|(_, p)| p).collect()
}

// GTC-FR-YHDU, GTC-FR-RZNQ, GTC-FR-04
#[test]
fn a_named_branch_that_is_not_checked_out_is_published_with_its_upstream() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let current = f.current_branch();
    {
        let repo = f.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    let (_dir, bare) = bare_origin(&f);

    let (outcome, events) = push_named(&f.root(), &GlobalSettingsStore::default(), "feature");
    outcome.unwrap();

    assert!(bare.find_reference("refs/heads/feature").is_ok(), "the remote holds the branch");
    assert!(
        bare.find_reference(&format!("refs/heads/{current}")).is_err(),
        "the branch the checkout has out is not pushed"
    );
    let repo = f.repo();
    assert!(
        repo.find_reference("refs/remotes/origin/feature").is_ok(),
        "the remote-tracking ref exists, so the head state sees the branch (GTC-FR-YWCP)"
    );
    let local = repo.find_branch("feature", BranchType::Local).unwrap();
    assert_eq!(local.upstream().unwrap().name().unwrap(), Some("origin/feature"));

    let done = finished(&events);
    assert_eq!(done.len(), 1, "exactly one terminal event");
    assert_eq!(done[0]["ok"], serde_json::json!(true));
    assert!(events.iter().any(|(name, _)| name == GIT_OUTPUT_LINE));
}

// GTC-FR-22, PRG-FR-KXQW
#[test]
fn a_named_push_names_that_branch_as_its_destination() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    {
        let repo = f.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    let (_dir, _bare) = bare_origin(&f);
    let sink = CollectingSink::default();

    push_named_branch_at(
        &sink,
        &SCRATCH_BUFFER,
        &|_: &str, _: serde_json::Value| {},
        &ProgressRegistry::default(),
        GithubAccess {
            root: &f.root(),
            store: &GlobalSettingsStore::default(),
            tokens: &GithubTokens::default(),
            project_key: "key",
        },
        "feature",
    )
    .unwrap();

    let published = sink.published.lock().unwrap().clone();
    let expected = Some(crate::progress::Activation::GitPush { branch: "feature".to_string() });
    assert!(!published.is_empty());
    assert!(published.iter().all(|op| op.activation == expected), "{published:?}");
}

// GTC-FR-KUVE, GTC-FR-04
#[test]
fn an_unknown_or_empty_branch_is_refused_before_the_remote_is_reached() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let (_dir, bare) = bare_origin(&f);

    let refs_before: Vec<(String, Option<git2::Oid>)> = f
        .repo()
        .references()
        .unwrap()
        .flatten()
        .map(|r| (r.name().unwrap_or_default().to_string(), r.target()))
        .collect();
    for name in ["", "nowhere", "origin/nowhere"] {
        let (outcome, events) = push_named(&f.root(), &GlobalSettingsStore::default(), name);
        assert_eq!(outcome.unwrap_err(), ERR_UNKNOWN_BRANCH, "branch {name:?}");
        let done = finished(&events);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0]["ok"], serde_json::json!(false));
        assert_eq!(done[0]["error"], serde_json::json!(ERR_UNKNOWN_BRANCH));
        assert!(
            !events.iter().any(|(name, _)| name == GIT_OUTPUT_LINE),
            "no transfer started for {name:?}"
        );
    }
    assert_eq!(bare.references().unwrap().count(), 0, "the remote received nothing");
    let refs_after: Vec<(String, Option<git2::Oid>)> = f
        .repo()
        .references()
        .unwrap()
        .flatten()
        .map(|r| (r.name().unwrap_or_default().to_string(), r.target()))
        .collect();
    assert_eq!(refs_before, refs_after, "no local ref changed");
}

// GTC-FR-KUVE
#[test]
fn a_named_push_with_no_remote_is_the_typed_error() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let current = f.current_branch();

    let (outcome, events) = push_named(&f.root(), &GlobalSettingsStore::default(), &current);
    assert_eq!(outcome.unwrap_err(), ERR_NO_REMOTE_CONFIGURED);
    assert_eq!(finished(&events).len(), 1);
}

// GTC-FR-09, GTC-FR-10
#[test]
fn a_named_push_to_github_with_no_token_refuses_before_any_network_request() {
    // The URL names a repository that does not exist: a push that reached
    // the network would fail with another error, or hang, instead.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let current = f.current_branch();
    f.repo().remote("origin", "https://github.com/acme/does-not-exist.git").unwrap();

    let (outcome, events) = push_named(&f.root(), &GlobalSettingsStore::in_memory(), &current);
    assert_eq!(outcome.unwrap_err(), github_tokens::ERR_TOKEN_MISSING);
    assert!(!events.iter().any(|(name, _)| name == GIT_OUTPUT_LINE));
    assert!(f.repo().find_reference(&format!("refs/remotes/origin/{current}")).is_err());
}

/// A Tauri app with an empty stream store of its own, so the lookup never
/// reads the machine's real store.
fn app_with_empty_stream_store(store: &TempDir) -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    handle.manage(crate::fs::FsAccessState::default());
    handle.manage(crate::streams::StreamState::rooted_at(store.path().to_path_buf()));
    app
}

// GTC-FR-RZNQ
#[test]
fn the_push_runs_from_the_checkout_that_holds_the_branch_else_from_the_primary_worktree() {
    let f = WorktreeFixture::new();
    let primary = f.root();
    let current = f.current();
    f.branch("feature");
    let linked = crate::worktree::create_worktree_at(
        &primary,
        "feature",
        &f.sibling("feature-wt").to_string_lossy(),
    )
    .unwrap();
    f.branch("loose");
    let streams = TempDir::new().unwrap();
    let app = app_with_empty_stream_store(&streams);
    let handle = app.handle();
    let canon = |p: PathBuf| crate::changes::canonicalize_lenient(&p);

    // A linked worktree has the branch out.
    assert_eq!(canon(push_checkout(handle, &primary, "feature")), canon(linked.clone()));
    // The primary worktree has the branch out.
    assert_eq!(canon(push_checkout(handle, &linked, &current)), canon(primary.clone()));
    // No checkout holds it: the primary worktree, also from a linked root.
    assert_eq!(canon(push_checkout(handle, &linked, "loose")), canon(primary.clone()));
    assert_eq!(canon(push_checkout(handle, &primary, "loose")), canon(primary.clone()));
}

#[test]
fn the_named_push_command_is_in_scope() {
    // Renaming or dropping it fails to compile here, before it can silently
    // regress at `invoke` time.
    let _a = push_branch;
}

// GTC-FR-KUVE
#[test]
fn a_name_held_only_as_a_remote_tracking_ref_is_an_unknown_branch() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let (_dir, bare) = bare_origin(&f);
    {
        let repo = f.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.reference("refs/remotes/origin/feature", head.id(), true, "t").unwrap();
    }

    let (outcome, _) = push_named(&f.root(), &GlobalSettingsStore::default(), "origin/feature");
    assert_eq!(outcome.unwrap_err(), ERR_UNKNOWN_BRANCH);
    assert!(f.repo().find_branch("origin/feature", BranchType::Local).is_err());
    assert_eq!(bare.references().unwrap().count(), 0);
}

// GTC-FR-KUVE
#[test]
fn with_no_remote_an_unknown_branch_is_refused_as_no_remote() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    let (outcome, _) = push_named(&f.root(), &GlobalSettingsStore::default(), "nowhere");
    assert_eq!(outcome.unwrap_err(), ERR_NO_REMOTE_CONFIGURED);
}

// GTC-FR-RZNQ, GTC-FR-04
#[test]
fn a_refused_push_returns_the_remote_reason_and_leaves_the_remote_branch_alone() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let current = f.current_branch();
    let (_dir, bare) = bare_origin(&f);
    push_named(&f.root(), &GlobalSettingsStore::default(), &current).0.unwrap();
    let published = bare.refname_to_id(&format!("refs/heads/{current}")).unwrap();

    // The local branch moves to a commit that does not descend from the one
    // the remote holds, so the push is not a fast-forward.
    {
        let repo = f.repo();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        let sig = repo.signature().unwrap();
        let orphan = repo.commit(None, &sig, &sig, "rewritten", &tree, &[]).unwrap();
        repo.reference(&format!("refs/heads/{current}"), orphan, true, "t").unwrap();
    }

    let (outcome, events) = push_named(&f.root(), &GlobalSettingsStore::default(), &current);
    let reason = outcome.unwrap_err();
    assert!(!reason.is_empty());
    assert_ne!(reason, ERR_UNKNOWN_BRANCH);
    assert_eq!(bare.refname_to_id(&format!("refs/heads/{current}")).unwrap(), published);
    let done = finished(&events);
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["ok"], serde_json::json!(false));
}

// GTC-FR-04
#[test]
fn a_branch_the_remote_already_holds_in_full_pushes_cleanly() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let current = f.current_branch();
    let (_dir, _bare) = bare_origin(&f);
    push_named(&f.root(), &GlobalSettingsStore::default(), &current).0.unwrap();

    let (outcome, events) = push_named(&f.root(), &GlobalSettingsStore::default(), &current);
    outcome.unwrap();
    assert_eq!(finished(&events).len(), 1);
}
