//! What the settings file never holds, derived state, and the write lock
//! (GSS-FR-22, GTS-FR-08).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- The secret never reaches the store (GSS-FR-22) ------------------

#[test]
fn gss_ts18_a_real_add_leaves_no_secret_anywhere_in_synthesis_toml() {
    // The negative half of GSS-FR-22, against a genuinely disk-backed store:
    // the file must be safe to read, copy, or attach to a bug report.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    let secret = "ghp_real_secret_value_a3f9";
    let secrets = Arc::new(FakeSecrets::default());
    let tokens = GithubTokens::new(
        Box::new(ArcSecrets(secrets.clone())),
        Box::new(FakeVerifier::accepting(secret, "raver119", &["repo"])),
    );

    let store = GlobalSettingsStore::with_path(path.clone());
    let record = add_token_impl(&store, &tokens, "work", secret).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains(secret),
        "the whole secret reached synthesis.toml: {text}"
    );
    assert!(
        !text.contains("ghp_real_secret_value"),
        "a prefix of the secret reached synthesis.toml: {text}"
    );
    // Only the last four characters, and only as the masked hint.
    assert!(text.contains("a3f9"), "{text}");
    assert!(text.contains("work"), "{text}");
    // And the secret really is in the credential store instead.
    assert_eq!(
        secrets.entries.lock().unwrap().get(&record.id).map(String::as_str),
        Some(secret)
    );
}

// -- Derived vs stored state (GTS-FR-08) -----------------------------

#[test]
fn an_unavailable_state_is_derived_rather_than_written_into_the_record() {
    // Persisting it would leave the row reading `unavailable` even after the
    // keychain entry came back, until someone pressed Verify again.
    let Harness {
        store,
        tokens,
        secrets,
        ..
    } = harness(FakeVerifier::accepting(
        "ghp_secret_1234",
        "raver119",
        &["repo"],
    ));
    let added = add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();

    secrets.forget(&added.id);
    let reported = validate_token_impl(&store, &tokens, &added.id).unwrap();
    assert_eq!(reported.state, TokenState::Unavailable);
    assert_eq!(
        store.load_github_token_registry().unwrap()[0].state,
        TokenState::Valid,
        "the derived state must not be persisted"
    );

    // The entry comes back (keychain unlocked, machine restored): the row
    // recovers on the next read with no further action from the author.
    secrets.set(&added.id, "ghp_secret_1234").unwrap();
    assert_eq!(
        list_tokens_impl(&store, &tokens).unwrap()[0].state,
        TokenState::Valid
    );
}

#[test]
fn removal_reports_a_broken_keychain_rather_than_orphaning_the_secret() {
    // Pinning a deliberate trade-off: with the credential store refusing,
    // dropping the registry record would strand a secret nothing names and
    // nothing can ever delete. The typed error asks the author to retry once
    // the keychain is available, which is recoverable; an orphan is not.
    let store = GlobalSettingsStore::in_memory();
    store
        .save_github_token_registry(vec![record("a", "work")])
        .unwrap();
    let tokens = GithubTokens::new(
        Box::new(FakeSecrets::broken()),
        Box::new(FakeVerifier::rejecting()),
    );

    assert_eq!(
        remove_token_impl(&store, &tokens, "a").unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
    assert_eq!(store.load_github_token_registry().unwrap().len(), 1);
}

// -- Concurrency (the `write_lock` guard) ----------------------------

#[test]
fn concurrent_adds_do_not_lose_records_or_admit_a_duplicate_label() {
    // Each command reads the registry, edits it, and writes it back. Without
    // the write lock two `invoke`s interleaving would silently discard one
    // another's record — the classic lost update.
    let secret = "ghp_shared_secret_1234";
    let store = Arc::new(GlobalSettingsStore::in_memory());
    let verifier = Arc::new(FakeVerifier::accepting(secret, "raver119", &["repo"]));
    let tokens = Arc::new(GithubTokens::new(
        Box::new(FakeSecrets::default()),
        Box::new(ArcVerifier(verifier)),
    ));

    // Distinct labels: every one must survive.
    let mut handles = Vec::new();
    for i in 0..8 {
        let (store, tokens) = (store.clone(), tokens.clone());
        handles.push(std::thread::spawn(move || {
            add_token_impl(&store, &tokens, &format!("token-{i}"), secret)
        }));
    }
    for h in handles {
        h.join().unwrap().unwrap();
    }
    assert_eq!(
        store.load_github_token_registry().unwrap().len(),
        8,
        "a lost update dropped a record"
    );

    // One shared label: exactly one may win.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let (store, tokens) = (store.clone(), tokens.clone());
        handles.push(std::thread::spawn(move || {
            add_token_impl(&store, &tokens, "contested", secret)
        }));
    }
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        outcomes.iter().filter(|r| r.is_ok()).count(),
        1,
        "a label must be unique even under concurrent adds"
    );
    for err in outcomes.iter().filter_map(|r| r.as_ref().err()) {
        assert_eq!(err, ERR_DUPLICATE_LABEL);
    }
    assert_eq!(store.load_github_token_registry().unwrap().len(), 9);
}

// -- Ids (they are keychain account names) ---------------------------

#[test]
fn token_ids_are_never_repeated() {
    // The id is the keychain account name, so a reused one would hand a new
    // record the previous one's secret.
    let ids: std::collections::HashSet<String> =
        (0..10_000).map(|_| new_token_id()).collect();
    assert_eq!(ids.len(), 10_000);
}

#[test]
fn a_mask_never_exposes_more_than_four_characters() {
    for secret in ["", "a", "abcd", "abcde", "ghp_0123456789abcdef", "é☃🔑xyz"] {
        assert!(
            mask_hint(secret).chars().count() <= 4,
            "mask_hint({secret:?}) exposed too much"
        );
    }
}

#[test]
fn github_token_command_functions_are_in_scope() {
    // Compile-time references: renaming or removing any GTS command without
    // updating `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    let _ = list_github_tokens;
    let _ = add_github_token;
    let _ = validate_github_token;
    let _ = rename_github_token;
    let _ = remove_github_token;
    let _ = open_github_token_creation_page;
    let _ = get_project_github_token_binding;
    let _ = set_project_github_token_binding;
}
