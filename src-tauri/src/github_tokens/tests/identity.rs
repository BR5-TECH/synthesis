//! Resolving the identity a token authenticates as
//! (GTS-FR-16, GTS-FR-10).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- resolve_github_identity (GTS-FR-16 / GTS-FR-10) ------------------

#[test]
fn ts17_resolving_an_identity_returns_the_account_and_never_reads_the_keychain() {
    let Harness { store, tokens, secrets, .. } = harness(FakeVerifier::accepting_account(
        "ghp_secret_1234",
        "raver119",
        Some("Demo Author"),
        Some("author@example.com"),
        &["repo"],
    ));
    let added = add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();
    assert_eq!(
        added.account_display_name.as_deref(),
        Some("Demo Author"),
        "verification retains the account's facts beside the login (GTS-FR-16)"
    );
    assert_eq!(added.account_email.as_deref(), Some("author@example.com"));

    // The keychain is wiped: the secret is gone, but the account is a
    // registry fact, so attribution still works.
    secrets.forget(&added.id);

    let identity = resolve_github_identity(&store, "/dev/acme").unwrap();
    assert_eq!(identity.login, "raver119");
    assert_eq!(identity.display_name.as_deref(), Some("Demo Author"));
    assert_eq!(identity.email.as_deref(), Some("author@example.com"));
}

#[test]
fn ts17_an_account_reporting_no_email_yields_absence_rather_than_an_empty_string() {
    let Harness { store, tokens, .. } = harness(FakeVerifier::accepting_account(
        "ghp_secret_1234",
        "octocat",
        None,
        None,
        &["repo"],
    ));
    add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();

    let identity = resolve_github_identity(&store, "/dev/acme").unwrap();
    assert_eq!(identity.login, "octocat");
    assert!(identity.display_name.is_none());
    assert!(identity.email.is_none());
}

#[test]
fn ts17_identity_resolution_reports_the_same_distinction_the_ui_routes_on() {
    // The rail routes selection-required to the picker (CMT-FR-25) and
    // nothing-stored to Global settings (CMT-FR-26), so the two must not
    // collapse here any more than they do for the secret path.
    let Harness { store, tokens, .. } =
        harness(FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));

    assert_eq!(
        resolve_github_identity(&store, "/dev/acme").unwrap_err(),
        ERR_TOKEN_MISSING
    );

    add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();
    assert_eq!(
        resolve_github_identity(&store, "/dev/acme").unwrap().login,
        "raver119",
        "a single stored token is used implicitly — no prompt"
    );

    store
        .save_github_token_registry({
            let mut r = store.load_github_token_registry().unwrap();
            r.push(record("b", "personal"));
            r
        })
        .unwrap();
    assert_eq!(
        resolve_github_identity(&store, "/dev/acme").unwrap_err(),
        ERR_SELECTION_REQUIRED
    );

    // And binding the project resolves it again.
    set_binding_impl(&store, "/dev/acme", "b").unwrap();
    assert_eq!(
        resolve_github_identity(&store, "/dev/acme").unwrap_err(),
        ERR_IDENTITY_UNRESOLVED,
        "record `b` was never verified, so it names no account to attribute to"
    );
}

#[test]
fn ts17_re_verifying_refreshes_the_account_facts_with_the_login() {
    let verifier = FakeVerifier::accepting_account(
        "ghp_secret_1234",
        "raver119",
        Some("Old Name"),
        None,
        &["repo"],
    );
    let Harness { store, tokens, .. } = harness(verifier);
    let added = add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();
    assert_eq!(added.account_display_name.as_deref(), Some("Old Name"));

    // The account is renamed on GitHub and gains a public email.
    let Harness { store: store2, tokens: tokens2, .. } =
        harness(FakeVerifier::accepting_account(
            "ghp_secret_1234",
            "raver119",
            Some("New Name"),
            Some("new@example.com"),
            &["repo"],
        ));
    let re_added = add_token_impl(&store2, &tokens2, "work", "ghp_secret_1234").unwrap();
    let validated = validate_token_impl(&store2, &tokens2, &re_added.id).unwrap();
    assert_eq!(validated.account_display_name.as_deref(), Some("New Name"));
    assert_eq!(validated.account_email.as_deref(), Some("new@example.com"));
}

#[test]
fn account_payload_parsing_tolerates_the_nullable_fields() {
    // GitHub returns `name` and `email` as null far more often than not.
    let full = account_from_user_payload(
        r#"{"login":"raver119","name":"Demo Author","email":"a@b.c"}"#,
    )
    .unwrap();
    assert_eq!(full.login, "raver119");
    assert_eq!(full.display_name.as_deref(), Some("Demo Author"));
    assert_eq!(full.email.as_deref(), Some("a@b.c"));

    let sparse =
        account_from_user_payload(r#"{"login":"octocat","name":null,"email":null}"#).unwrap();
    assert_eq!(sparse.login, "octocat");
    assert!(sparse.display_name.is_none());
    assert!(sparse.email.is_none());

    // A blank name is absence wearing a different shape.
    let blank = account_from_user_payload(r#"{"login":"octocat","name":"   "}"#).unwrap();
    assert!(blank.display_name.is_none());

    // No login at all is not a response this module understands.
    assert!(account_from_user_payload(r#"{"name":"nobody"}"#).is_none());
    assert!(account_from_user_payload("not json").is_none());
    // And the older single-field helper still agrees with it.
    assert_eq!(
        login_from_user_payload(r#"{"login":"raver119"}"#).as_deref(),
        Some("raver119")
    );
}

#[test]
fn a_registry_written_before_the_account_fields_existed_still_loads() {
    // GSS-FR-13 would otherwise repair the whole store to defaults, wiping
    // recents and both registries, over two fields that were simply absent.
    let decoded: GithubTokenRecord = serde_json::from_str(
        r#"{"id":"a","label":"work","accountLogin":"raver119","scopes":[],"maskedHint":"a3f9","addedAt":"t","state":"valid"}"#,
    )
    .unwrap();
    assert_eq!(decoded.account_login.as_deref(), Some("raver119"));
    assert!(decoded.account_display_name.is_none());
    assert!(decoded.account_email.is_none());
}
