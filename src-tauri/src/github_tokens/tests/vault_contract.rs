//! The contract with the consolidated vault
//! (GTS-FR-01, GTS-FR-14, GTS-FR-17).
//!
//! One part of `../tests/mod.rs`, which holds the harness these run against.

use super::*;

// -- The consolidated vault (GTS-FR-01 / GTS-FR-14 / GTS-FR-17) -------
//
// These are the only tests here wired to the real vault rather than to
// `FakeSecrets`. Everything above is about this module's own rules and is
// better served by a store it can break on demand; what is asserted below
// is the *contract with the vault*, which a fake of this module's own would
// assert against itself.
/// GTS-FR-17, GTS-FR-01, GTS-FR-13: two records whose secrets are still in the legacy per-token
/// entries are adopted into the one entry on the first secret read, and the
/// legacy entries are gone afterwards.
#[test]
fn gts_ts18_legacy_secrets_are_adopted_on_the_first_read() {
    let keyring = FakeKeyring::default();
    keyring.set_legacy(LEGACY_KEYCHAIN_SERVICE, "a", "ghp_secret_a");
    keyring.set_legacy(LEGACY_KEYCHAIN_SERVICE, "b", "ghp_secret_b");
    let (store, tokens) = vault_harness(
        &keyring,
        vec![record("a", "work"), record("b", "personal")],
        FakeVerifier::rejecting(),
    );
    store.save_github_token_binding("proj", "a").unwrap();

    let secret = resolve_github_token_secret(&store, &tokens, "proj").unwrap();

    assert_eq!(secret, "ghp_secret_a");
    assert!(keyring.legacy(LEGACY_KEYCHAIN_SERVICE, "a").is_none());
    assert!(keyring.legacy(LEGACY_KEYCHAIN_SERVICE, "b").is_none());
    // Both secrets are in the one entry, at the paths GTS-FR-01 names.
    let object = keyring.object();
    let tokens_ns = object
        .get("github")
        .and_then(|v| v.get("tokens"))
        .and_then(|v| v.as_object())
        .expect("the github.tokens namespace exists");
    assert_eq!(tokens_ns.get("a").and_then(|v| v.as_str()), Some("ghp_secret_a"));
    assert_eq!(tokens_ns.get("b").and_then(|v| v.as_str()), Some("ghp_secret_b"));
    // And the listing reads both as present rather than `unavailable`.
    let listed = list_tokens_impl(&store, &tokens).unwrap();
    assert!(listed.iter().all(|r| r.state != TokenState::Unavailable));
}

/// GTS-FR-17: a candidate is supplied for every record, whether or not the
/// vault already holds its secret — the vault decides what is adopted.
#[test]
fn gts_fr17_a_candidate_is_supplied_for_every_record() {
    let candidates =
        migration_candidates(&[record("a", "work"), record("b", "personal")]);
    assert_eq!(candidates.len(), 2);
    for (candidate, id) in candidates.iter().zip(["a", "b"]) {
        assert_eq!(candidate.legacy_service, "com.synthesis.github-token");
        assert_eq!(candidate.legacy_account, id);
        assert_eq!(
            candidate.path,
            vec!["github".to_string(), "tokens".to_string(), id.to_string()]
        );
    }
}

/// The five typed vault failures, each with the switch that provokes it.
fn vault_provocations() -> Vec<(&'static str, Box<dyn Fn(&FakeKeyring)>)> {
    vec![
        (
            "vault_unavailable",
            Box::new(|k: &FakeKeyring| k.state().refuse_read = true),
        ),
        (
            "vault_write_failed",
            Box::new(|k: &FakeKeyring| k.state().refuse_write = true),
        ),
        (
            "vault_verify_failed",
            Box::new(|k: &FakeKeyring| k.state().mangle_next_writes = 1),
        ),
        (
            "vault_malformed",
            Box::new(|k: &FakeKeyring| k.set_entry("not an AppSecrets object")),
        ),
        (
            "vault_unsupported_version",
            Box::new(|k: &FakeKeyring| k.set_entry(r#"{"version":99}"#)),
        ),
    ]
}

/// GTS-FR-14 / ASV-FR-31: every typed vault failure that can refuse a
/// *write* crosses the boundary as one `keychain_unavailable`, leaves the
/// registry as it was, and never stops the listing working.
///
/// `vault_malformed` is not among them and is asserted separately below:
/// ASV-FR-17 has a mutation *recover* from an undecodable value rather than
/// refuse over it, which is the one place the five outcomes differ from
/// each other on the write path.
#[test]
fn gts_ts15_every_vault_write_failure_is_one_keychain_unavailable() {
    for (name, provoke) in vault_provocations() {
        if name == "vault_malformed" {
            continue;
        }
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, tokens) = vault_harness(
            &keyring,
            vec![record("a", "work")],
            FakeVerifier::accepting("ghp_new_secret", "raver119", &["repo"]),
        );

        let err = match add_token_impl(&store, &tokens, "second", "ghp_new_secret") {
            Err(err) => err,
            Ok(record) => panic!("provoked {name} but the add succeeded: {}", record.id),
        };
        assert_eq!(err, ERR_KEYCHAIN_UNAVAILABLE, "provoked {name}");

        // The registry is exactly as it was, and listing still works
        // (GTS-FR-14): it reads the registry alone.
        let listed = list_tokens_impl(&store, &tokens).unwrap();
        assert_eq!(listed.len(), 1, "provoked {name}");
        assert_eq!(listed[0].id, "a");
    }
}

/// GTS-FR-03 read with `ASV-application-secret-vault.md` ASV-FR-30: a
/// listing costs **one** vault access, and that stays true as the number of
/// records grows.
///
/// This is the module where the count actually varies — AAP and AIC each
/// list a fixed descriptor table — so it is the one where "one access" and
/// "one access per record" can be told apart at all. Before the
/// consolidation each record cost its own keyring entry; now twenty records
/// cost the same single read as one, which on a platform that prompts per
/// access is the difference between one authentication and twenty.
#[test]
fn gts_fr03_a_listing_costs_one_vault_access_however_many_records() {
    use crate::secret_vault::test_support::Call;

    let mut reads_for = Vec::new();
    for count in [1usize, 20] {
        let keyring = FakeKeyring::default();
        let registry: Vec<GithubTokenRecord> = (0..count)
            .map(|i| record(&format!("t{i}"), &format!("label {i}")))
            .collect();
        let (store, tokens) =
            vault_harness(&keyring, registry, FakeVerifier::rejecting());
        // Let migration run first; it is the one sequence allowed to read
        // more than once.
        let _ = list_tokens_impl(&store, &tokens).unwrap();
        let before = keyring.count(&Call::Read);

        let listed = list_tokens_impl(&store, &tokens).unwrap();

        assert_eq!(listed.len(), count);
        reads_for.push(keyring.count(&Call::Read) - before);
    }
    assert_eq!(reads_for, vec![1, 1], "one read for one record and for twenty");
}

/// ASV-FR-08: a project-scoped secret and a project's token binding are
/// stored in different places, and neither place holds the other's content.
///
/// Asserted against a genuinely disk-backed store rather than the in-memory
/// one, because the claim is about what `synthesis.toml` contains as text —
/// the file an author can copy, back up, or attach to a bug report
/// (GSS-FR-22, ASV-FR-08).
#[test]
fn asv_ts05_the_binding_is_in_the_file_and_the_secret_is_in_the_vault() {
    use crate::secret_vault::{Mutation, Vault, VaultSecrets};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    let keyring = FakeKeyring::default();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    let tokens = GithubTokens::new(
        Box::new(VaultSecrets::new(Arc::clone(&vault), VAULT_NAMESPACE)),
        Box::new(FakeVerifier::accepting("ghp_supersecret1234", "raver119", &["repo"])),
    );

    // A stored token, a binding for the project, and a project-scoped
    // secret written straight at its own vault path.
    let added = add_token_impl(&store, &tokens, "work", "ghp_supersecret1234").unwrap();
    set_binding_impl(&store, "acme", &added.id).unwrap();
    vault
        .apply_secret_mutations(&[Mutation::Set {
            path: crate::secret_vault::path(&["projects", "acme", "github", "deploy_key"]),
            secret: "deploy-key-supersecret".into(),
        }])
        .unwrap();

    // The file holds the binding and the description, and no secret.
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(&added.id), "the binding names the token");
    assert!(text.contains("1234"), "the masked hint is a description");
    assert!(!text.contains("ghp_supersecret1234"));
    assert!(!text.contains("ghp_supersecret"));
    assert!(!text.contains("deploy-key-supersecret"));
    assert!(!text.contains("deploy_key"));

    // The entry holds both secrets and no binding.
    let object = keyring.object();
    assert_eq!(
        object
            .get("projects")
            .and_then(|v| v.get("acme"))
            .and_then(|v| v.get("github"))
            .and_then(|v| v.get("deploy_key"))
            .and_then(|v| v.as_str()),
        Some("deploy-key-supersecret")
    );
    let serialized = serde_json::to_string(&object).unwrap();
    assert!(!serialized.contains("binding"));
    assert!(!serialized.contains("work"), "no label reaches the entry");
}

/// GTS-FR-14 / ASV-FR-31 on the read path: every one of the five — the
/// malformed value included — makes a stored token unusable as one
/// `keychain_unavailable`, and never as a wrong secret or a lost record.
#[test]
fn gts_ts15_every_vault_failure_refuses_a_read_the_same_way() {
    for (name, provoke) in vault_provocations() {
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, tokens) = vault_harness(
            &keyring,
            vec![record("a", "work")],
            FakeVerifier::rejecting(),
        );
        store.save_github_token_binding("proj", "a").unwrap();

        assert_eq!(
            resolve_github_token_secret(&store, &tokens, "proj"),
            Err(ERR_KEYCHAIN_UNAVAILABLE.to_string()),
            "provoked {name}"
        );
        // GTS-FR-08: the record is retained and reads `unavailable` rather
        // than the listing failing.
        let listed = list_tokens_impl(&store, &tokens).unwrap();
        assert_eq!(listed.len(), 1, "provoked {name}");
        assert_eq!(listed[0].state, TokenState::Unavailable, "provoked {name}");
    }
}

/// ASV-FR-31: **no other error value** crosses the boundary. The vault's own
/// codes travel inside `SecretUnavailable` so a developer can tell the five
/// apart in a debugger, and every call site is responsible for collapsing
/// them — a `map_err` that passed one through would be invisible otherwise.
#[test]
fn no_vault_error_code_reaches_the_ipc_boundary() {
    for (name, provoke) in vault_provocations() {
        let keyring = FakeKeyring::default();
        provoke(&keyring);
        let (store, tokens) = vault_harness(
            &keyring,
            vec![record("a", "work")],
            FakeVerifier::accepting("ghp_new_secret", "raver119", &["repo"]),
        );
        store.save_github_token_binding("proj", "a").unwrap();

        let errors = [
            add_token_impl(&store, &tokens, "second", "ghp_new_secret").err(),
            remove_token_impl(&store, &tokens, "a").err(),
            resolve_github_token_secret(&store, &tokens, "proj").err(),
            validate_token_impl(&store, &tokens, "a").err(),
        ];
        for error in errors.into_iter().flatten() {
            // `contains` rather than `starts_with`: a code wrapped in prose
            // (`"keychain: vault_malformed"`) has crossed the boundary just
            // as surely as a bare one.
            for code in [
                "vault_unavailable",
                "vault_write_failed",
                "vault_verify_failed",
                "vault_malformed",
                "vault_unsupported_version",
            ] {
                assert!(
                    !error.contains(code),
                    "provoked {name}: a vault error code crossed the boundary: {error}"
                );
            }
        }
    }
}

/// ASV-FR-17 read together with GTS-FR-04: an undecodable vault value does
/// not cost the author the token they are adding. The mutation recovers —
/// the prior value is quarantined and the new secret is stored — and the
/// registry gains its record.
#[test]
fn a_malformed_vault_value_does_not_refuse_an_add() {
    let keyring = FakeKeyring::default();
    keyring.set_entry("not an AppSecrets object");
    let (store, tokens) = vault_harness(
        &keyring,
        vec![],
        FakeVerifier::accepting("ghp_new_secret", "raver119", &["repo"]),
    );

    let added = add_token_impl(&store, &tokens, "work", "ghp_new_secret").unwrap();

    assert_eq!(added.label, "work");
    let object = keyring.object();
    assert_eq!(
        object.get("quarantine").and_then(|v| v.as_str()),
        Some("not an AppSecrets object")
    );
    assert_eq!(
        resolve_github_token_secret(&store, &tokens, "proj").unwrap(),
        "ghp_new_secret"
    );
}
