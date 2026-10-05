//! The push primitive (GTC-FR-04 / GTC-FR-11 / GTC-FR-22): the events it
//! streams, and the redaction its output goes through.
//!
//! One part of `mod.rs`, which holds the fixtures these run against.

use super::*;

// -----------------------------------------------------------------------
// Push (GTC-FR-04 / GTC-FR-11 / GTC-FR-22)
// -----------------------------------------------------------------------

#[test]
fn redaction_strips_userinfo_from_any_url_in_a_line() {
    // GTC-FR-11: a remote URL echoed by the transport reaches the output
    // area with its credential removed.
    assert_eq!(
        redact_credentials("remote: https://x-access-token:ghp_SECRET@github.com/a/b.git ok"),
        "remote: https://github.com/a/b.git ok"
    );
    assert_eq!(
        redact_credentials("pushing to https://github.com/a/b.git"),
        "pushing to https://github.com/a/b.git"
    );
    // A `@` after the authority belongs to the path and is left alone.
    assert_eq!(
        redact_credentials("https://github.com/a/b@v2"),
        "https://github.com/a/b@v2"
    );
    assert_eq!(redact_credentials("no url here"), "no url here");
}

#[test]
fn a_push_streams_its_output_and_ends_in_one_terminal_event() {
    // GTC-FR-22, GTC-FR-04: the events a push emits, against a local bare
    // remote so no network is involved. The channel and shape are the same
    // whichever surface invoked it — this path is the only one there is.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let remote_dir = TempDir::new().unwrap();
    Repository::init_bare(remote_dir.path()).unwrap();
    f.repo()
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();

    let sink = RecordingSink::default();
    let registry = ProgressRegistry::default();
    let emitted: RefCell<Vec<(String, serde_json::Value)>> = RefCell::new(Vec::new());
    let emit = |name: &str, payload: serde_json::Value| {
        emitted.borrow_mut().push((name.to_string(), payload));
    };
    let store = GlobalSettingsStore::default();
    let tokens = GithubTokens::default();

    push_branch_at(
        &sink,
        &SCRATCH_BUFFER,
        &emit,
        &registry,
        &f.root(),
        &store,
        &tokens,
        "key",
    )
    .unwrap();

    let events = emitted.borrow();
    let finished: Vec<&(String, serde_json::Value)> = events
        .iter()
        .filter(|(name, _)| name == GIT_OPERATION_FINISHED)
        .collect();
    assert_eq!(finished.len(), 1, "exactly one terminal event");
    assert_eq!(finished[0].1["ok"], serde_json::json!(true));
    assert_eq!(finished[0].1["operation"], serde_json::json!("push"));
    assert!(
        events.iter().any(|(name, _)| name == GIT_OUTPUT_LINE),
        "output streamed on the line channel"
    );

    // GTC-FR-22: reported in the status bar under `kind = "git"` for as
    // long as it ran.
    let seen = sink.seen.lock().unwrap();
    assert!(seen.iter().all(|(kind, _)| kind == PROGRESS_KIND_GIT));
    assert!(seen.iter().any(|(_, state)| state == "Finished"));

    // The branch now tracks the remote, so the next sync-state read reports
    // it as published rather than as never pushed (GTC-FR-21).
    let state = upstream_sync_state(&f.root()).unwrap();
    assert!(state.has_remote && state.has_upstream);
    assert_eq!((state.ahead, state.behind), (Some(0), Some(0)));
}

#[test]
fn a_push_operation_names_the_branch_it_publishes_as_its_destination() {
    // PRG-FR-KXQW, GTC-FR-22: every event of the push carries the same
    // `git_push` destination, naming the branch the push publishes.
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");
    let remote_dir = TempDir::new().unwrap();
    Repository::init_bare(remote_dir.path()).unwrap();
    f.repo()
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();
    let sink = CollectingSink::default();
    let registry = ProgressRegistry::default();
    let emit = |_name: &str, _payload: serde_json::Value| {};

    push_branch_at(
        &sink,
        &SCRATCH_BUFFER,
        &emit,
        &registry,
        &f.root(),
        &GlobalSettingsStore::default(),
        &GithubTokens::default(),
        "key",
    )
    .unwrap();

    let published = sink.published.lock().unwrap().clone();
    assert!(published.len() >= 2, "registration and terminal event: {published:?}");
    let expected = Some(crate::progress::Activation::GitPush {
        branch: f.current_branch(),
    });
    assert!(
        published.iter().all(|op| op.activation == expected),
        "every event carries the branch destination: {published:?}"
    );
}

#[test]
fn a_push_with_no_remote_is_the_typed_error_and_still_announces_its_end() {
    let f = Fixture::new();
    f.write("a.md", "one\n");
    f.commit("base");

    let sink = RecordingSink::default();
    let registry = ProgressRegistry::default();
    let emitted: RefCell<Vec<(String, serde_json::Value)>> = RefCell::new(Vec::new());
    let emit = |name: &str, payload: serde_json::Value| {
        emitted.borrow_mut().push((name.to_string(), payload));
    };

    let err = push_branch_at(
        &sink,
        &SCRATCH_BUFFER,
        &emit,
        &registry,
        &f.root(),
        &GlobalSettingsStore::default(),
        &GithubTokens::default(),
        "key",
    )
    .unwrap_err();
    assert_eq!(err, ERR_NO_REMOTE_CONFIGURED);

    let events = emitted.borrow();
    let finished: Vec<&(String, serde_json::Value)> = events
        .iter()
        .filter(|(name, _)| name == GIT_OPERATION_FINISHED)
        .collect();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].1["ok"], serde_json::json!(false));
    assert_eq!(
        finished[0].1["error"],
        serde_json::json!(ERR_NO_REMOTE_CONFIGURED)
    );
}

#[test]
fn new_command_functions_are_in_scope() {
    // Renaming or dropping one of these fails to compile here, before it
    // can silently regress at `invoke` time.
    let _a = commit_paths;
    let _b = get_upstream_sync_state;
    let _c = push_current_branch;
}
