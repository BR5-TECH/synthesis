//! The records every Git operation reports (`../core/LGC-logging.md`): that
//! an operation reports itself at all, and that what it reports carries no
//! credential, no remote URL, and none of the author's content.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -- Logging (`../core/LGC-logging.md`) ----------------------------------
//
// Two properties are worth pinning, and neither is visible in a passing
// suite otherwise: that an operation reports itself at all, and that what it
// reports carries no credential, no remote URL, and none of the author's
// content. Every test takes a buffer of its own (`own_buffer`), so a count
// is exact and an absence is real.

#[test]
fn a_checkout_logs_its_request_and_the_branch_it_landed_on() {
    let log = own_buffer();
    let f = WorktreeFixture::new();
    f.branch("develop");

    checkout_branch_at(&NullSink, log, &f.root(), "develop").unwrap();

    let requested = record_for(log, "checking out branch");
    assert_eq!(requested.level, LogLevel::Info);
    assert_eq!(requested.domains, vec![Domain::Backend]);
    assert_eq!(
        requested.fields.get("branch"),
        Some(&serde_json::json!("develop"))
    );

    let done = record_for(log, "branch checked out");
    assert_eq!(done.level, LogLevel::Info);
    assert_eq!(
        done.fields.get("localBranch"),
        Some(&serde_json::json!("develop"))
    );
    assert_eq!(
        done.fields.get("createdLocalBranch"),
        Some(&serde_json::json!(false)),
        "a local branch that already existed was not created"
    );
    assert!(done.fields.contains_key("durationMs"));
    assert!(
        records_for(log, MSG_CHECKOUT_FAILED).is_empty(),
        "a checkout that took reports no failure"
    );
}

#[test]
fn a_checkout_of_a_remote_branch_records_the_local_branch_it_created() {
    // WTC-FR-11: the branch that ends up checked out is not the name the
    // caller asked for, which is precisely why the record carries both.
    let log = own_buffer();
    let f = WorktreeFixture::new();
    f.remote_ref("origin/experiment");

    checkout_branch_at(&NullSink, log, &f.root(), "origin/experiment").unwrap();

    let done = record_for(log, "branch checked out");
    assert_eq!(
        done.fields.get("branch"),
        Some(&serde_json::json!("origin/experiment"))
    );
    assert_eq!(
        done.fields.get("localBranch"),
        Some(&serde_json::json!("experiment"))
    );
    assert_eq!(
        done.fields.get("createdLocalBranch"),
        Some(&serde_json::json!(true))
    );
}

#[test]
fn a_refused_checkout_is_a_warning_carrying_the_typed_cause() {
    // Nothing broke — the branch does not exist and the panel says so — so
    // the record is a `WARN` naming the refusal in a field rather than an
    // `ERROR` a reader would go hunting through.
    let log = own_buffer();
    let f = WorktreeFixture::new();

    let err = checkout_branch_at(&NullSink, log, &f.root(), "no-such-branch").unwrap_err();
    assert_eq!(err, ERR_UNKNOWN_BRANCH);

    let failed = record_for(log, MSG_CHECKOUT_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(ERR_UNKNOWN_BRANCH))
    );
    assert!(
        records_for(log, "branch checked out").is_empty(),
        "a refusal reports no success"
    );
}

#[test]
fn a_checkout_refused_in_a_linked_worktree_is_a_warning_too() {
    // GTC-FR-08 / WTC-FR-21: the refusal a reader is most likely to be
    // puzzled by, since nothing about the panel says why.
    let log = own_buffer();
    let f = WorktreeFixture::new();
    f.branch("alpha");
    f.branch("develop");
    let alpha = crate::worktree::create_worktree_at(
        &f.root(),
        "alpha",
        &f.sibling("wt-alpha").to_string_lossy(),
    )
    .unwrap();

    let err = checkout_branch_at(&NullSink, log, &alpha, "develop").unwrap_err();
    assert_eq!(err, crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE);
    let failed = record_for(log, MSG_CHECKOUT_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(
            crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE
        ))
    );
}

#[test]
fn a_fetch_reports_the_remote_by_name_and_never_its_url() {
    // GTC-FR-11 at the logging seam: the remote's URL is the one value in
    // reach that can carry an embedded credential, so no record carries one
    // — the remote is named, and whether a token was presented is a boolean.
    let log = own_buffer();
    let f = RemoteFixture::new();

    fetch_remote_branches(
        &CollectingSink::default(),
        log,
        &ProgressRegistry::default(),
        &f.root(),
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "",
    )
    .unwrap();

    let started = record_for(log, "fetching remote branches");
    assert_eq!(started.level, LogLevel::Info);
    assert_eq!(
        started.domains,
        vec![Domain::Backend, Domain::Remote],
        "a transfer is about the remote as well as the backend"
    );
    assert_eq!(
        started.fields.get("remote"),
        Some(&serde_json::json!("origin"))
    );
    assert_eq!(
        started.fields.get("authenticated"),
        Some(&serde_json::json!(false)),
        "the credential is a boolean, never the secret"
    );

    let done = record_for(log, "remote branches fetched");
    assert_eq!(done.level, LogLevel::Info);
    assert_eq!(done.fields.get("remote"), Some(&serde_json::json!("origin")));
    assert!(done.fields.contains_key("durationMs"));

    let text = buffer_text(log);
    assert!(
        !text.contains(&f.url()),
        "no record carries the remote's URL: {text}"
    );
    assert!(
        !text.contains("file://"),
        "not even its scheme: {text}"
    );
}

#[test]
fn a_push_with_no_remote_is_reported_as_a_refusal() {
    // The commonest "I pressed push and nothing happened", and the Changes
    // panel offers no other explanation.
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    let err = push_for_logging(log, &f.root()).unwrap_err();
    assert_eq!(err, ERR_NO_REMOTE_CONFIGURED);

    let failed = record_for(log, MSG_PUSH_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(ERR_NO_REMOTE_CONFIGURED))
    );
    assert!(records_for(log, "pushing branch").is_empty());
}

#[test]
fn a_push_that_resolves_no_token_reports_the_refusal_and_reaches_nothing() {
    // The other commonest one, and the record names the branch and remote a
    // fetch's cannot — a push is always about a specific branch. The URL is
    // never contacted: the credential is resolved first, and there is none.
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.repo()
        .remote("origin", "https://github.com/acme/does-not-exist.git")
        .unwrap();
    let branch = f.current_branch();

    let err = push_for_logging(log, &f.root()).unwrap_err();
    assert_eq!(err, github_tokens::ERR_TOKEN_MISSING);

    let failed = record_for(log, MSG_PUSH_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(github_tokens::ERR_TOKEN_MISSING))
    );
    assert_eq!(
        failed.fields.get("branch"),
        Some(&serde_json::json!(branch))
    );
    assert_eq!(
        failed.fields.get("remote"),
        Some(&serde_json::json!("origin"))
    );
    assert!(
        records_for(log, "pushing branch").is_empty(),
        "the refusal comes before the transfer, so nothing announced one"
    );
    assert!(
        !buffer_text(log).contains("github.com/acme"),
        "and no record carries the remote's URL"
    );
}

#[test]
fn a_fetch_that_fails_at_the_transport_is_an_error_naming_no_url() {
    // The record behind a branch listing that quietly went stale: a
    // background refresh reaches no remote and no surface says so.
    let log = own_buffer();
    let f = WorktreeFixture::new();
    let nowhere = f.sibling("nowhere");
    f.repo()
        .remote("origin", &format!("file://{}", nowhere.display()))
        .unwrap();

    let err = fetch_remote_branches(
        &CollectingSink::default(),
        log,
        &ProgressRegistry::default(),
        &f.root(),
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "",
    )
    .unwrap_err();
    assert_eq!(err, github_tokens::ERR_GITHUB_UNREACHABLE);

    // The transfer was announced before it was attempted, which is what
    // makes the pair readable as "started, then failed".
    assert!(records_for(log, "fetching remote branches").len() == 1);
    let failed = record_for(log, MSG_FETCH_FAILED);
    assert_eq!(failed.level, LogLevel::Error);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(github_tokens::ERR_GITHUB_UNREACHABLE))
    );
    assert!(failed.fields.contains_key("durationMs"));
    let text = buffer_text(log);
    assert!(
        !text.contains("nowhere") && !text.contains("file://"),
        "the remote's URL reaches no record, failure or not: {text}"
    );
}

#[test]
fn a_fetch_that_resolves_no_token_reports_the_refusal_and_reaches_nothing() {
    // GTC-FR-10 / GTC-FR-14: the two token failures the UI answers
    // differently, each on the record as a `WARN` — the author picks a token
    // and tries again. The URL is unreachable, so a test that opened a
    // transport would hang rather than pass.
    let log = own_buffer();
    let f = WorktreeFixture::new();
    f.repo()
        .remote("origin", "https://github.com/acme/does-not-exist.git")
        .unwrap();

    let err = fetch_remote_branches(
        &CollectingSink::default(),
        log,
        &ProgressRegistry::default(),
        &f.root(),
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "/dev/acme",
    )
    .unwrap_err();
    assert_eq!(err, github_tokens::ERR_TOKEN_MISSING);

    let failed = record_for(log, MSG_FETCH_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(github_tokens::ERR_TOKEN_MISSING))
    );
    assert_eq!(
        failed.fields.get("remote"),
        Some(&serde_json::json!("origin"))
    );
    assert!(
        records_for(log, "fetching remote branches").is_empty(),
        "the refusal comes before the transfer, so nothing announced one"
    );
    let text = buffer_text(log);
    assert!(
        !text.contains("github.com/acme"),
        "and no record carries the remote's URL: {text}"
    );

    // The other variant, which the UI answers by opening the token picker
    // rather than by offering to add one: two tokens stored, no binding.
    let picker = own_buffer();
    let two = GlobalSettingsStore::in_memory();
    two.save_github_token_registry(vec![
        crate::github_tokens::GithubTokenRecord {
            id: "a".into(),
            label: "one".into(),
            ..Default::default()
        },
        crate::github_tokens::GithubTokenRecord {
            id: "b".into(),
            label: "two".into(),
            ..Default::default()
        },
    ])
    .unwrap();
    let err = fetch_remote_branches(
        &CollectingSink::default(),
        picker,
        &ProgressRegistry::default(),
        &f.root(),
        &two,
        &GithubTokens::default(),
        "/dev/acme",
    )
    .unwrap_err();
    assert_eq!(err, github_tokens::ERR_SELECTION_REQUIRED);
    let failed = record_for(picker, MSG_FETCH_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(github_tokens::ERR_SELECTION_REQUIRED))
    );
    assert!(
        !buffer_text(picker).contains("github.com/acme"),
        "neither refusal names the remote's URL"
    );
}

#[test]
fn a_fetch_with_no_remote_is_reported_as_a_refusal() {
    let log = own_buffer();
    let f = WorktreeFixture::new();

    let err = fetch_remote_branches(
        &CollectingSink::default(),
        log,
        &ProgressRegistry::default(),
        &f.root(),
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "",
    )
    .unwrap_err();
    assert_eq!(err, ERR_NO_REMOTE_CONFIGURED);

    let failed = record_for(log, MSG_FETCH_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(ERR_NO_REMOTE_CONFIGURED))
    );
}

/// A push with its events discarded — the event stream has its own tests;
/// these are about the records.
///
/// The settings store is `in_memory`, never `default`: `default` reads the
/// machine's real global settings, and against a `github.com` remote that
/// would resolve the *author's own* token out of their keychain and push
/// with it. A unit test must reach neither. With an empty store the
/// credential resolves to nothing and the refusal happens before any
/// transport is opened.
fn push_for_logging(log: &'static LogBuffer, root: &Path) -> Result<(), String> {
    let emitted: RefCell<Vec<(String, serde_json::Value)>> = RefCell::new(Vec::new());
    let emit = |name: &str, payload: serde_json::Value| {
        emitted.borrow_mut().push((name.to_string(), payload));
    };
    push_branch_at(
        &RecordingSink::default(),
        log,
        &emit,
        &ProgressRegistry::default(),
        root,
        &GlobalSettingsStore::in_memory(),
        &GithubTokens::default(),
        "key",
    )
}

#[test]
fn a_push_records_the_branch_it_published_and_whether_it_set_an_upstream() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let remote_dir = TempDir::new().unwrap();
    Repository::init_bare(remote_dir.path()).unwrap();
    f.repo()
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();
    let branch = f.current_branch();

    push_for_logging(log, &f.root()).unwrap();

    let started = record_for(log, "pushing branch");
    assert_eq!(started.level, LogLevel::Info);
    assert_eq!(started.domains, vec![Domain::Backend, Domain::Remote]);
    assert_eq!(
        started.fields.get("branch"),
        Some(&serde_json::json!(branch))
    );
    assert_eq!(
        started.fields.get("authenticated"),
        Some(&serde_json::json!(false))
    );

    let done = record_for(log, "branch pushed");
    assert_eq!(done.level, LogLevel::Info);
    assert_eq!(done.fields.get("remote"), Some(&serde_json::json!("origin")));
    assert_eq!(
        done.fields.get("upstreamSet"),
        Some(&serde_json::json!(true)),
        "the branch had never been published, so this push published it"
    );
    assert!(done.fields.contains_key("durationMs"));

    // A second push has nothing left to wire up.
    let again = own_buffer();
    f.write("a.md", "one\ntwo\n");
    f.commit("second");
    push_for_logging(again, &f.root()).unwrap();
    assert_eq!(
        record_for(again, "branch pushed").fields.get("upstreamSet"),
        Some(&serde_json::json!(false))
    );
}

#[test]
fn a_failed_push_is_an_error_carrying_the_typed_cause_and_no_url() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    // A path no repository lives at: the transfer fails at the transport
    // rather than being refused a credential, and reaches no network.
    let unreachable = f.root().join("log-probe-nonexistent-remote");
    f.repo()
        .remote("origin", &format!("file://{}", unreachable.to_string_lossy()))
        .unwrap();

    let err = push_for_logging(log, &f.root()).unwrap_err();
    assert_eq!(err, github_tokens::ERR_GITHUB_UNREACHABLE);

    let failed = record_for(log, MSG_PUSH_FAILED);
    assert_eq!(
        failed.level,
        LogLevel::Error,
        "a remote that could not be reached leaves the author nothing to act on"
    );
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(github_tokens::ERR_GITHUB_UNREACHABLE)),
        "the typed cause is `classify_transfer_error`'s fixed vocabulary, \
         not libgit2's message"
    );
    assert!(failed.fields.contains_key("durationMs"));
    let text = buffer_text(log);
    assert!(
        !text.contains("log-probe-nonexistent-remote") && !text.contains("file://"),
        "the remote's URL reaches no record: {text}"
    );
}

#[test]
fn a_push_from_a_detached_head_is_reported_as_a_refusal_before_any_transport() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let remote_dir = TempDir::new().unwrap();
    Repository::init_bare(remote_dir.path()).unwrap();
    f.repo()
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();
    let repo = f.repo();
    let id = repo.head().unwrap().peel_to_commit().unwrap().id();
    repo.set_head_detached(id).unwrap();
    drop(repo);

    let err = push_for_logging(log, &f.root()).unwrap_err();
    assert_eq!(err, ERR_NO_BRANCH_CHECKED_OUT);

    let failed = record_for(log, MSG_PUSH_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(ERR_NO_BRANCH_CHECKED_OUT))
    );
    assert!(
        records_for(log, "pushing branch").is_empty(),
        "nothing announced a push that never had a branch to publish"
    );
}

#[test]
fn a_commit_logs_what_it_created_and_never_the_message() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.write("a.md", "one\ntwo\n");

    let outcome = commit_paths_reported(
        &NullSink,
        log,
        &crate::fs::RootFs::for_root(f.root()),
        "log-probe-commit-message",
        &["a.md".to_string()],
        None,
    )
    .unwrap();

    let created = record_for(log, "commit created");
    assert_eq!(created.level, LogLevel::Info);
    assert_eq!(
        created.fields.get("commit"),
        Some(&serde_json::json!(outcome.commit_id))
    );
    assert_eq!(created.fields.get("pathCount"), Some(&serde_json::json!(1)));
    assert_eq!(
        created.fields.get("recordedCount"),
        Some(&serde_json::json!(1))
    );
    assert!(created.fields.contains_key("durationMs"));
    let text = buffer_text(log);
    assert!(
        !text.contains("log-probe-commit-message"),
        "the commit message is the author's own prose and is logged nowhere: {text}"
    );
    assert!(
        !text.contains("a.md"),
        "the paths are counted rather than itemised: {text}"
    );
}

#[test]
fn a_refused_commit_is_a_warning_carrying_the_typed_cause() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.write("a.md", "one\ntwo\n");

    let err = commit_paths_reported(
        &NullSink,
        log,
        &crate::fs::RootFs::for_root(f.root()),
        "   ",
        &["a.md".to_string()],
        None,
    )
    .unwrap_err();
    assert_eq!(err, ERR_EMPTY_COMMIT_MESSAGE);

    let failed = record_for(log, MSG_COMMIT_FAILED);
    assert_eq!(
        failed.level,
        LogLevel::Warn,
        "a validation refusal is something the author can fix"
    );
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(ERR_EMPTY_COMMIT_MESSAGE))
    );
    assert!(records_for(log, "commit created").is_empty());
}

#[test]
fn a_diff_logs_its_shape_and_never_its_content() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    f.write("a.md", "one\nlog-probe-diff-content\n");

    let payload = diff_reported(
        &NullSink,
        log,
        &f.root(),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();
    assert!(!payload.hunks.is_empty());

    let computed = record_for(log, "diff computed");
    assert_eq!(
        computed.level,
        LogLevel::Debug,
        "a read the panel repeats on every render must not evict the records \
         a reader is actually after"
    );
    assert_eq!(
        computed.fields.get("scope"),
        Some(&serde_json::json!("path"))
    );
    assert_eq!(computed.fields.get("path"), Some(&serde_json::json!("a.md")));
    assert_eq!(
        computed.fields.get("isBinary"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(
        computed.fields.get("hunks"),
        Some(&serde_json::json!(payload.hunks.len()))
    );
    assert!(
        !buffer_text(log).contains("log-probe-diff-content"),
        "the diff text is the author's content and is logged nowhere"
    );
}

#[test]
fn a_diff_that_cannot_be_computed_is_reported_with_the_scope_it_was_asked_about() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    let err = diff_reported(
        &NullSink,
        log,
        &f.root(),
        &DiffScope::Branch {
            path: "a.md".into(),
            target_branch: "no-such-branch".into(),
            previous_path: None,
        },
    )
    .unwrap_err();
    assert_eq!(err, ERR_UNKNOWN_BRANCH);

    let failed = record_for(log, MSG_DIFF_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("scope"),
        Some(&serde_json::json!("branch"))
    );
    assert_eq!(
        failed.fields.get("targetBranch"),
        Some(&serde_json::json!("no-such-branch"))
    );
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(ERR_UNKNOWN_BRANCH))
    );
}

#[test]
fn a_file_revisions_read_logs_which_sides_exist_and_neither_side_itself() {
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "log-probe-old-side\n");
    f.commit("base");
    f.write("a.md", "log-probe-new-side\n");

    file_revisions_reported(
        &NullSink,
        log,
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Path {
            path: "a.md".into(),
            previous_path: None,
        },
    )
    .unwrap();

    let read = record_for(log, "file revisions read");
    assert_eq!(read.level, LogLevel::Debug);
    assert_eq!(read.fields.get("hasOld"), Some(&serde_json::json!(true)));
    assert_eq!(read.fields.get("hasNew"), Some(&serde_json::json!(true)));
    assert_eq!(
        read.fields.get("isBinary"),
        Some(&serde_json::json!(false))
    );
    let text = buffer_text(log);
    assert!(
        !text.contains("log-probe-old-side") && !text.contains("log-probe-new-side"),
        "neither side's text appears in a record — only whether it exists \
         and how long it is: {text}"
    );
}

#[test]
fn the_staged_scope_is_reported_as_the_refusal_it_is() {
    // GTC-FR-16: the staged set names no single file, so this read cannot
    // serve it — and the record says which scope was asked for.
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    let err = file_revisions_reported(
        &NullSink,
        log,
        &crate::fs::RootFs::for_root(f.root()),
        &DiffScope::Staged,
    )
    .unwrap_err();
    assert_eq!(err, ERR_SCOPE_NOT_A_FILE);

    let failed = record_for(log, MSG_REVISIONS_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("scope"),
        Some(&serde_json::json!("staged"))
    );
    assert!(
        !failed.fields.contains_key("path"),
        "the staged set names no path, so the record claims none"
    );
}

#[test]
fn a_branch_listing_logs_its_counts_and_the_current_branch() {
    let log = own_buffer();
    let f = WorktreeFixture::new();
    f.branch("develop");
    f.remote_ref("origin/experiment");

    let listed = branches_reported(&NullSink, log, &f.root()).unwrap();

    let record = record_for(log, "branches listed");
    assert_eq!(record.level, LogLevel::Debug);
    assert_eq!(
        record.fields.get("local"),
        Some(&serde_json::json!(
            listed.iter().filter(|b| b.kind == "local").count()
        ))
    );
    assert_eq!(
        record.fields.get("remote"),
        Some(&serde_json::json!(1)),
        "the remote-tracking branch is counted"
    );
    assert_eq!(
        record.fields.get("current"),
        Some(&serde_json::json!(f.current()))
    );
    assert!(
        !buffer_text(log).contains("develop"),
        "branches are counted rather than itemised, so a repository with a \
         thousand of them costs one record"
    );
}

#[test]
fn a_read_against_a_non_repository_is_reported_as_a_refusal() {
    let log = own_buffer();
    let dir = TempDir::new().unwrap();

    assert_eq!(
        branches_reported(&NullSink, log, dir.path()).unwrap_err(),
        ERR_NOT_A_REPO
    );
    let failed = record_for(log, MSG_LISTING_FAILED);
    assert_eq!(failed.level, LogLevel::Warn);
    assert_eq!(
        failed.fields.get("error"),
        Some(&serde_json::json!(ERR_NOT_A_REPO))
    );

    let sync = own_buffer();
    assert_eq!(
        upstream_sync_state_reported(&NullSink, sync, dir.path()).unwrap_err(),
        ERR_NOT_A_REPO
    );
    assert_eq!(
        record_for(sync, "failed to read the upstream sync state").level,
        LogLevel::Warn
    );
}

#[test]
fn the_upstream_sync_state_read_logs_the_standing_it_reported() {
    // The read behind the Changes panel's decision to offer a push
    // (CHG-FR-37): when it says nothing is publishable, this record is the
    // only place the reason survives.
    let log = own_buffer();
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    let state = upstream_sync_state_reported(&NullSink, log, &f.root()).unwrap();
    assert!(!state.has_remote);

    let read = record_for(log, "upstream sync state read");
    assert_eq!(read.level, LogLevel::Debug);
    assert_eq!(
        read.fields.get("hasRemote"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(
        read.fields.get("hasUpstream"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(read.fields.get("ahead"), Some(&serde_json::json!(null)));
    assert_eq!(read.fields.get("behind"), Some(&serde_json::json!(null)));
}

#[test]
fn a_failure_is_a_warning_when_the_author_can_act_on_it_and_an_error_otherwise() {
    // The level is what a reader is meant to *do* about a record. Pinned
    // over the whole vocabulary rather than only the constants the emit-site
    // tests above happen to reach.
    for refusal in [
        ERR_NOT_A_REPO,
        ERR_UNKNOWN_BRANCH,
        ERR_NO_MERGE_BASE,
        ERR_SCOPE_NOT_A_FILE,
        ERR_NO_REMOTE_CONFIGURED,
        ERR_EMPTY_COMMIT_MESSAGE,
        ERR_NO_PATHS_SELECTED,
        ERR_NOTHING_TO_COMMIT,
        ERR_CHECKOUT_BLOCKED,
        ERR_BRANCH_ALREADY_CHECKED_OUT,
        ERR_NO_BRANCH_CHECKED_OUT,
        crate::worktree::ERR_CHECKOUT_NOT_IN_LINKED_WORKTREE,
        github_tokens::ERR_SELECTION_REQUIRED,
        github_tokens::ERR_TOKEN_MISSING,
    ] {
        assert!(is_refusal(refusal), "{refusal} is the author's to resolve");
    }
    // GTC-FR-31: the identity refusal carries its two paths, so it is matched
    // by prefix rather than by equality — asserted on the value that is
    // actually emitted rather than on the bare code, which would pass even
    // if the prefix arm were removed.
    assert!(is_refusal(&worktree_identity_error(
        "/dev/acme",
        "/dev/acme-feature"
    )));
    for failure in [
        github_tokens::ERR_INVALID_TOKEN,
        github_tokens::ERR_GITHUB_UNREACHABLE,
        github_tokens::ERR_KEYCHAIN_UNAVAILABLE,
        "failed to write the index: whatever libgit2 said",
    ] {
        assert!(!is_refusal(failure), "{failure} leaves nothing to act on");
    }
}
