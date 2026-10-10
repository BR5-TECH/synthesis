//! Listing, validating, renaming and removing a token
//! (GTS-FR-05, GTS-FR-07, GTS-FR-08, GTS-FR-09).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- list / validate (GTS-FR-07 / FR-08) -----------------------------

#[test]
fn gts_ts06_a_record_whose_secret_vanished_lists_as_unavailable() {
    // GTS-FR-07 / GTS-FR-08: retained rather than pruned — a row the author
    // can remove deliberately beats a token that silently disappeared.
    let Harness { store, tokens, secrets, .. } = harness(FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    let added = add_token_impl(&store, &tokens, "work", "ghp_secret_1234", "").unwrap();
    assert_eq!(list_tokens_impl(&store, &tokens).unwrap()[0].state, TokenState::Valid);

    secrets.forget(&added.id);

    let listed = list_tokens_impl(&store, &tokens).unwrap();
    assert_eq!(listed.len(), 1, "the record is retained");
    assert_eq!(listed[0].state, TokenState::Unavailable);
}

#[test]
fn gts_ts05_a_revoked_token_becomes_invalid_and_stays_stored() {
    // GTS-FR-05 / GTS-FR-07.
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
    let added = add_token_impl(&store, &tokens, "work", "ghp_secret_1234", "").unwrap();

    // The token is revoked on GitHub: it stops being recognised.
    verifier.revoke("ghp_secret_1234");

    let revalidated = validate_token_impl(&store, &tokens, &added.id).unwrap();
    assert_eq!(revalidated.state, TokenState::Invalid);
    assert!(revalidated.last_verified_at.is_some());
    assert_eq!(
        store.load_github_token_registry().unwrap().len(),
        1,
        "an invalid token stays stored so the author can see which one expired"
    );
}

#[test]
fn an_unreachable_github_never_relabels_a_good_token_as_bad() {
    // GTS-FR-07: a flaky network is not evidence about the token.
    let store = GlobalSettingsStore::in_memory();
    let secrets = std::sync::Arc::new(FakeSecrets::default());
    secrets.set("a", "ghp_secret_1234").unwrap();
    store
        .save_github_token_registry(vec![GithubTokenRecord {
            id: "a".into(),
            label: "work".into(),
            state: TokenState::Valid,
            ..Default::default()
        }])
        .unwrap();
    let tokens = GithubTokens::new(
        Box::new(ArcSecrets(secrets)),
        Box::new(FakeVerifier::offline()),
    );

    let err = validate_token_impl(&store, &tokens, "a").unwrap_err();
    assert_eq!(err, ERR_GITHUB_UNREACHABLE);
    assert_eq!(
        store.load_github_token_registry().unwrap()[0].state,
        TokenState::Valid,
        "the record must be untouched when GitHub never answered"
    );
}

#[test]
fn validating_an_unknown_id_is_a_typed_error() {
    let Harness { store, tokens, .. } = harness(FakeVerifier::rejecting());
    assert_eq!(
        validate_token_impl(&store, &tokens, "nope").unwrap_err(),
        ERR_UNKNOWN_TOKEN
    );
}

// -- rename (GTS-FR-05) ----------------------------------------------

#[test]
fn renaming_onto_another_label_is_refused_and_changes_nothing() {
    let Harness { store, tokens, .. } = harness(FakeVerifier::rejecting());
    store
        .save_github_token_registry(vec![record("a", "work"), record("b", "personal")])
        .unwrap();

    let err = rename_token_impl(&store, &tokens, "b", "work").unwrap_err();
    assert_eq!(err, ERR_DUPLICATE_LABEL);
    assert_eq!(store.load_github_token_registry().unwrap()[1].label, "personal");

    // Renaming a record to its own label is not a collision with itself.
    rename_token_impl(&store, &tokens, "b", "personal").unwrap();
    let renamed = rename_token_impl(&store, &tokens, "b", "home").unwrap();
    assert_eq!(renamed.label, "home");
}

// -- remove (GTS-FR-09) ----------------------------------------------

#[test]
fn gts_ts07_removal_drops_both_halves_and_is_idempotent() {
    // GTS-FR-08, GTS-FR-14.
    let Harness { store, tokens, secrets, .. } = harness(FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    let added = add_token_impl(&store, &tokens, "work", "ghp_secret_1234", "").unwrap();

    remove_token_impl(&store, &tokens, &added.id).unwrap();
    remove_token_impl(&store, &tokens, &added.id).unwrap();

    assert!(store.load_github_token_registry().unwrap().is_empty());
    assert!(!secrets.contains(&added.id));
    assert_eq!(secrets.count(), 0, "no orphaned secret is left behind");
}
