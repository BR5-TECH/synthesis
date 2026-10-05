//! The per-project binding and the secret it resolves
//! (GTS-FR-10, GTS-FR-11, GTS-FR-13).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- binding (GTS-FR-10 / FR-11) -------------------------------------

#[test]
fn gts_ts09_removing_the_bound_token_reopens_the_question() {
    // GTS-FR-10.
    let Harness { store, tokens, .. } = harness(FakeVerifier::rejecting());
    store
        .save_github_token_registry(vec![
            record("a", "work"),
            record("b", "personal"),
            record("c", "ci"),
        ])
        .unwrap();
    set_binding_impl(&store, "/dev/acme", "a").unwrap();
    assert_eq!(
        get_binding_impl(&store, "/dev/acme").unwrap().resolution,
        BindingResolution::Bound
    );

    remove_token_impl(&store, &tokens, "a").unwrap();
    let after = get_binding_impl(&store, "/dev/acme").unwrap();
    assert_eq!(after.resolution, BindingResolution::SelectionRequired);
    assert_eq!(after.token_id, None);

    // And with every token gone there is nothing to pick between.
    remove_token_impl(&store, &tokens, "b").unwrap();
    remove_token_impl(&store, &tokens, "c").unwrap();
    assert_eq!(
        get_binding_impl(&store, "/dev/acme").unwrap().resolution,
        BindingResolution::NoneStored
    );
}

#[test]
fn gts_ts10_a_binding_is_one_fact_per_project_not_per_worktree() {
    // GTS-FR-09, GTS-FR-10 / GTS-FR-11: the slot is keyed by project (GSS-FR-18), and
    // `ProjectState::slot_key` returns the repository anchor whichever
    // worktree is active — so the binding is read back unchanged after a
    // worktree switch, and nothing was written into a project.
    let Harness { store, .. } = harness(FakeVerifier::rejecting());
    store
        .save_github_token_registry(vec![record("a", "work"), record("b", "personal")])
        .unwrap();

    set_binding_impl(&store, "/dev/acme", "b").unwrap();

    // The anchor is what both worktrees resolve to, so one read serves both.
    let b = get_binding_impl(&store, "/dev/acme").unwrap();
    assert_eq!(b.resolution, BindingResolution::Bound);
    assert_eq!(b.token_id.as_deref(), Some("b"));
    // A different project keeps its own answer.
    assert_eq!(
        get_binding_impl(&store, "/dev/other").unwrap().resolution,
        BindingResolution::SelectionRequired
    );
}

#[test]
fn gts_ts11_a_binding_survives_a_relaunch() {
    // GTS-FR-11: through disk, not just through the in-memory store.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_github_token_registry(vec![record("a", "work"), record("b", "personal")])
            .unwrap();
        set_binding_impl(&store, "/dev/acme", "b").unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path.clone());
    let b = get_binding_impl(&reloaded, "/dev/acme").unwrap();
    assert_eq!(b.resolution, BindingResolution::Bound);
    assert_eq!(b.token_id.as_deref(), Some("b"));

    // GSS-FR-22: the file itself carries the descriptions and no secret.
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("personal"), "{text}");
}

#[test]
fn binding_to_an_unknown_token_or_with_no_project_open_is_refused() {
    let Harness { store, .. } = harness(FakeVerifier::rejecting());
    store
        .save_github_token_registry(vec![record("a", "work")])
        .unwrap();

    assert_eq!(
        set_binding_impl(&store, "/dev/acme", "ghost").unwrap_err(),
        ERR_UNKNOWN_TOKEN
    );
    // An empty key is "no project open"; a binding there would belong to no
    // project in particular.
    assert_eq!(
        set_binding_impl(&store, "", "a").unwrap_err(),
        ERR_NO_PROJECT
    );
}

#[test]
fn with_no_project_open_the_binding_reads_from_no_slot() {
    let Harness { store, .. } = harness(FakeVerifier::rejecting());
    store
        .save_github_token_registry(vec![record("a", "work"), record("b", "personal")])
        .unwrap();
    // The default slot must not leak in as some project's answer.
    assert_eq!(
        get_binding_impl(&store, "").unwrap().resolution,
        BindingResolution::SelectionRequired
    );
}

// -- resolve_github_token_secret (GTS-FR-13) -------------------------

#[test]
fn resolving_a_secret_reports_the_same_distinction_the_ui_routes_on() {
    // GTC-FR-10: selection-required opens the picker, missing routes to
    // Global settings — so the two must never collapse.
    let Harness { store, tokens, .. } =
        harness(FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));

    assert_eq!(
        resolve_github_token_secret(&store, &tokens, "/dev/acme").unwrap_err(),
        ERR_TOKEN_MISSING
    );

    add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();
    // One token: used implicitly, no prompt.
    assert_eq!(
        resolve_github_token_secret(&store, &tokens, "/dev/acme").unwrap(),
        "ghp_secret_1234"
    );

    // A second token with no binding: the caller must ask.
    store
        .save_github_token_registry({
            let mut r = store.load_github_token_registry().unwrap();
            r.push(record("b", "personal"));
            r
        })
        .unwrap();
    assert_eq!(
        resolve_github_token_secret(&store, &tokens, "/dev/acme").unwrap_err(),
        ERR_SELECTION_REQUIRED
    );
}

#[test]
fn resolving_a_bound_token_whose_secret_is_gone_refuses_rather_than_sending_nothing() {
    let Harness { store, tokens, secrets, .. } = harness(FakeVerifier::accepting("ghp_secret_1234", "raver119", &["repo"]));
    let added = add_token_impl(&store, &tokens, "work", "ghp_secret_1234").unwrap();
    secrets.forget(&added.id);

    assert_eq!(
        resolve_github_token_secret(&store, &tokens, "/dev/acme").unwrap_err(),
        ERR_KEYCHAIN_UNAVAILABLE
    );
}
