//! Adding a token (GTS-FR-04, GTS-FR-05, GTS-FR-06).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- add_github_token (GTS-FR-04 / FR-05 / FR-06) --------------------

#[test]
fn gts_ts01_adding_stores_the_secret_in_the_keychain_and_the_description_in_the_registry() {
    // GTS-FR-01 / GTS-FR-02 / GTS-FR-03 / GTS-FR-04.
    let Harness { store, tokens, secrets, .. } = harness(FakeVerifier::accepting("ghp_secret_value_a3f9", "raver119", &["repo", "workflow"]));

    let record = add_token_impl(&store, &tokens, "work", "ghp_secret_value_a3f9").unwrap();

    assert_eq!(record.label, "work");
    assert_eq!(record.account_login.as_deref(), Some("raver119"));
    assert_eq!(record.scopes, vec!["repo", "workflow"]);
    assert_eq!(record.state, TokenState::Valid);
    assert_eq!(record.masked_hint, "a3f9");

    // The secret reached the keychain...
    assert!(secrets.contains(&record.id));
    // ...and nothing resembling it reached the registry.
    let persisted = store.load_github_token_registry().unwrap();
    assert_eq!(persisted.len(), 1);
    let as_text = serde_json::to_string(&persisted).unwrap();
    assert!(
        !as_text.contains("ghp_secret_value_a3f9"),
        "the registry must carry no secret material: {as_text}"
    );
    assert!(as_text.contains("a3f9"), "the masked hint is the exception");
}

#[test]
fn gts_ts02_a_rejected_token_stores_nothing() {
    // GTS-FR-04: verification precedes storage, so a refusal leaves no
    // keychain entry and no record.
    let Harness { store, tokens, secrets, .. } = harness(FakeVerifier::rejecting());
    let err = add_token_impl(&store, &tokens, "bad", "ghp_nope").unwrap_err();
    assert_eq!(err, ERR_INVALID_TOKEN);
    assert_eq!(secrets.count(), 0);
    assert!(store.load_github_token_registry().unwrap().is_empty());
}

#[test]
fn gts_ts03_an_unreachable_github_is_distinct_from_a_rejected_token() {
    // GTS-FR-04: the two call for different responses from the user, so
    // they must not collapse into one message.
    let Harness { store, tokens, secrets, .. } = harness(FakeVerifier::offline());
    let err = add_token_impl(&store, &tokens, "work", "ghp_anything").unwrap_err();
    assert_eq!(err, ERR_GITHUB_UNREACHABLE);
    assert_ne!(err, ERR_INVALID_TOKEN);
    assert_eq!(secrets.count(), 0);
    assert!(store.load_github_token_registry().unwrap().is_empty());
}

#[test]
fn gts_ts04_labels_are_unique_but_the_same_secret_may_be_stored_twice() {
    // GTS-FR-05 / GTS-FR-06.
    let verifier = FakeVerifier::accepting("secret_one_1234", "raver119", &["repo"]);
    verifier.accept("secret_two_5678", "raver119", &["repo"]);
    let Harness { store, tokens, .. } = harness(verifier);

    add_token_impl(&store, &tokens, "work", "secret_one_1234").unwrap();

    let err = add_token_impl(&store, &tokens, "work", "secret_two_5678").unwrap_err();
    assert_eq!(err, ERR_DUPLICATE_LABEL);
    assert_eq!(store.load_github_token_registry().unwrap().len(), 1);

    // Case alone does not make two labels distinguishable.
    let err = add_token_impl(&store, &tokens, "  WORK ", "secret_two_5678").unwrap_err();
    assert_eq!(err, ERR_DUPLICATE_LABEL);

    // A different label with the *same* secret is fine — identity is the
    // id, never the secret.
    add_token_impl(&store, &tokens, "work laptop", "secret_one_1234").unwrap();
    let records = store.load_github_token_registry().unwrap();
    assert_eq!(records.len(), 2);
    assert_ne!(records[0].id, records[1].id);
}

#[test]
fn an_omitted_label_is_derived_from_the_account_the_token_authenticates_as() {
    // GTS-FR-05: the account is the name the author would have typed, and
    // it is already known by the time the record is written — so a label is
    // a way to tell two tokens of the same account apart, not a toll gate.
    let Harness { store, tokens, .. } = harness(FakeVerifier::accepting(
        "ghp_secret_1234",
        "raver119",
        &["repo"],
    ));

    let first = add_token_impl(&store, &tokens, "", "ghp_secret_1234").unwrap();
    assert_eq!(first.label, "raver119");

    // A second token for the same account: derived labels dedupe silently,
    // because refusing a token over a name the author never chose would be
    // nonsense.
    let second = add_token_impl(&store, &tokens, "   ", "ghp_secret_1234").unwrap();
    assert_eq!(second.label, "raver119 (2)");
    let third = add_token_impl(&store, &tokens, "", "ghp_secret_1234").unwrap();
    assert_eq!(third.label, "raver119 (3)");
    assert_eq!(store.load_github_token_registry().unwrap().len(), 3);
}

#[test]
fn deriving_a_label_never_collides_with_one_the_author_chose() {
    let existing = vec![record("a", "raver119"), record("b", "raver119 (2)")];
    assert_eq!(derive_label_from_login(&existing, "raver119"), "raver119 (3)");
    // Case alone does not make it a distinct name.
    assert_eq!(derive_label_from_login(&existing, "RAVER119"), "RAVER119 (3)");
    assert_eq!(derive_label_from_login(&[], "raver119"), "raver119");
    // A verifier that resolved no login still yields something nameable.
    assert_eq!(derive_label_from_login(&[], "  "), "github token");
}

#[test]
fn an_explicit_duplicate_label_is_still_refused_before_the_network_call() {
    // The fast-fail path: a label the author chose is checked up front, so a
    // collision costs a round trip rather than a wait on GitHub.
    let Harness {
        store,
        tokens,
        verifier,
        ..
    } = harness(FakeVerifier::accepting(
        "ghp_secret_1234",
        "raver119",
        &["repo"],
    ));
    add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();

    // Make every verification fail: if the duplicate check ran first, the
    // error is still `duplicate_label` rather than the verifier's.
    verifier.revoke("ghp_secret_1234");
    assert_eq!(
        add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap_err(),
        ERR_DUPLICATE_LABEL
    );
}

#[test]
fn gts_ts14_a_locked_keychain_stores_nothing_but_listing_still_works() {
    // GTS-FR-02, GTS-FR-13 / GTS-FR-14: listing reads the registry alone, so it keeps
    // answering while the keychain will not.
    let store = GlobalSettingsStore::in_memory();
    let tokens = GithubTokens::new(
        Box::new(FakeSecrets::broken()),
        Box::new(FakeVerifier::accepting("ghp_good_1234", "raver119", &["repo"])),
    );

    let err = add_token_impl(&store, &tokens, "work", "ghp_good_1234").unwrap_err();
    assert_eq!(err, ERR_KEYCHAIN_UNAVAILABLE);
    assert!(store.load_github_token_registry().unwrap().is_empty());

    // Seed a record directly, as a previously-successful add would have.
    store
        .save_github_token_registry(vec![record("a", "work")])
        .unwrap();
    let listed = list_tokens_impl(&store, &tokens).unwrap();
    assert_eq!(listed.len(), 1, "listing must survive a locked keychain");
    assert_eq!(listed[0].state, TokenState::Unavailable);
}
