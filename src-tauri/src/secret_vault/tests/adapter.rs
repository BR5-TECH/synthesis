//! The adapter the owning modules hold.

use super::*;

// ---------------------------------------------------------------------------
// The adapter the owning modules hold
// ---------------------------------------------------------------------------

/// The namespace adapter maps an id onto the path its module owns, and turns
/// every typed vault failure into the one `SecretUnavailable` those modules
/// render as `keychain_unavailable` (ASV-FR-31).
#[test]
fn vault_secrets_maps_ids_onto_paths_and_collapses_errors() {
    use crate::ai_shared::SecretStore as AiSecretStore;

    let keyring = FakeKeyring::default();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.mark_migrated();
    let secrets = VaultSecrets::new(Arc::clone(&vault), &["ai_api", "providers"]);

    assert_eq!(
        secrets.path_of("openai"),
        p(&["ai_api", "providers", "openai"])
    );

    AiSecretStore::set(&secrets, "openai", "sk-key").unwrap();
    assert_eq!(
        vault
            .read_secret(&p(&["ai_api", "providers", "openai"]))
            .unwrap(),
        Some("sk-key".into())
    );
    assert!(AiSecretStore::has(&secrets, "openai").unwrap());
    let presence = AiSecretStore::presence(&secrets, &["openai", "anthropic"]).unwrap();
    assert_eq!(presence.get("openai"), Some(&true));
    assert_eq!(presence.get("anthropic"), Some(&false));

    AiSecretStore::delete(&secrets, "openai").unwrap();
    assert_eq!(AiSecretStore::get(&secrets, "openai").unwrap(), None);

    // Every typed vault failure collapses into the one error, and its message
    // names the outcome rather than anything about what the entry holds.
    // A process that starts on an undecodable entry. The first vault holds a
    // cache, so a second one stands for the next launch.
    keyring.set_entry("garbage");
    let restarted = Arc::new(Vault::new(Box::new(keyring.clone())));
    restarted.mark_migrated();
    let secrets = VaultSecrets::new(restarted, &["ai_api", "providers"]);
    let err = AiSecretStore::get(&secrets, "openai").unwrap_err();
    assert_eq!(err.0, "vault_malformed");
}

/// Two namespaces of one vault cannot collide, and clearing one never disturbs
/// the other — the property the per-level service names used to carry.
#[test]
fn vault_namespaces_do_not_collide() {
    use crate::ai_shared::SecretStore as AiSecretStore;

    let keyring = FakeKeyring::default();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.mark_migrated();
    let api = VaultSecrets::new(Arc::clone(&vault), &["ai_api", "providers"]);
    let agentic = VaultSecrets::new(Arc::clone(&vault), &["agentic", "vendors"]);

    AiSecretStore::set(&api, "shared_id", "api key").unwrap();
    AiSecretStore::set(&agentic, "shared_id", "agent credential").unwrap();

    assert_eq!(
        AiSecretStore::get(&api, "shared_id").unwrap(),
        Some("api key".into())
    );
    assert_eq!(
        AiSecretStore::get(&agentic, "shared_id").unwrap(),
        Some("agent credential".into())
    );

    AiSecretStore::delete(&api, "shared_id").unwrap();
    assert_eq!(AiSecretStore::get(&api, "shared_id").unwrap(), None);
    assert_eq!(
        AiSecretStore::get(&agentic, "shared_id").unwrap(),
        Some("agent credential".into())
    );
}

/// ASV-FR-33: the walking-skeleton contract holds for the GitHub adapter too,
/// which reaches the vault through its own `SecretStore` trait.
#[test]
fn vault_secrets_serves_the_github_trait() {
    use crate::github_tokens::SecretStore as GithubSecretStore;

    let keyring = FakeKeyring::default();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.mark_migrated();
    let secrets = VaultSecrets::new(vault, &["github", "tokens"]);

    GithubSecretStore::set(&secrets, "t1", "ghp_secret").unwrap();
    assert_eq!(
        GithubSecretStore::get(&secrets, "t1").unwrap(),
        Some("ghp_secret".into())
    );
    let presence = GithubSecretStore::presence(&secrets, &["t1", "t2"]).unwrap();
    assert_eq!(presence.get("t1"), Some(&true));
    assert_eq!(presence.get("t2"), Some(&false));
    GithubSecretStore::delete(&secrets, "t1").unwrap();
    GithubSecretStore::delete(&secrets, "t1").unwrap();
    assert_eq!(GithubSecretStore::get(&secrets, "t1").unwrap(), None);
}

/// ASV-FR-01: the application writes to no keyring entry but the one — which
/// rests on `keyring` being reachable from this module alone.
///
/// The three modules that own secrets each held a `KeyringSecretStore` of their
/// own before this consolidation. Nothing but review stops one coming back, and
/// a second entry would be invisible in a passing suite: every one of those
/// modules' tests runs against a fake. So the guard reads the sources.
#[test]
fn keyring_is_reachable_from_this_module_alone() {
    // Every `.rs` in the crate, so a module added later is covered without
    // anyone remembering to add it here.
    fn sources(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("the crate's source tree").flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                found.push(path);
            }
        }
    }
    let mut files = Vec::new();
    sources(std::path::Path::new("src"), &mut files);
    assert!(files.len() > 20, "the sweep found the source tree");

    for file in files {
        // The vault itself, and its own test files, which quote the crate
        // name in the guards below.
        if file.to_string_lossy().contains("secret_vault") {
            continue;
        }
        let source = std::fs::read_to_string(&file).expect("a source file");
        assert!(
            !source.contains("keyring::"),
            "{} reaches the keyring directly; every secret must go through the \
             application secret vault instead (ASV-FR-01)",
            file.display()
        );
    }
}

/// ASV-FR-32 read with the module's own claim: this module performs no
/// filesystem operation at all, which is what puts a vendor's own login
/// directory beyond its reach whatever it is asked to do.
///
/// Asserted rather than argued, because the `VaultBackend` seam gives a test no
/// way to observe a file that was never opened.
#[test]
fn the_vault_performs_no_filesystem_operation() {
    let source = include_str!("../../secret_vault.rs");
    for forbidden in ["std::fs", "FsAccess", "PathBuf", "File::"] {
        assert!(
            !source.contains(forbidden),
            "the vault reaches the filesystem via {forbidden}; a vendor's own \
             credential storage is outside this module entirely (ASV-FR-32)"
        );
    }
}

/// ASV-FR-01 / GTS-FR-01 / AAP-FR-07 / AIC-FR-20: the three modules that own
/// secrets reach the **process-wide** vault in production, at the namespace
/// each reserves.
///
/// Every module suite builds its own `Vault` over a fake keyring, so nothing
/// else executes the production wiring — a `Default` that quietly went back to
/// a per-level keyring store would leave the whole suite green.
#[test]
fn the_owning_modules_are_wired_to_the_global_vault() {
    for (name, source, namespace) in [
        (
            "github_tokens.rs",
            include_str!("../../github_tokens.rs"),
            "github_tokens::VAULT_NAMESPACE",
        ),
        (
            "ai_api.rs",
            include_str!("../../ai_api.rs"),
            "ai_api::VAULT_NAMESPACE",
        ),
        (
            "agentic.rs",
            include_str!("../../agentic.rs"),
            "agentic::VAULT_NAMESPACE",
        ),
    ] {
        let default = source
            .split_once("fn default() -> Self {")
            .unwrap_or_else(|| panic!("{name} has a Default impl"))
            .1;
        let default = default.split_once("\n}").expect("the end of the impl").0;
        assert!(
            default.contains("crate::secret_vault::VaultSecrets::new(")
                && default.contains("crate::secret_vault::global()")
                && default.contains("VAULT_NAMESPACE"),
            "{name}'s production credential store must be the {namespace} \
             namespace of the one application vault (ASV-FR-01)"
        );
    }
}

/// ASV-FR-01: the one entry the backend opens is the one the spec names, and it
/// opens no other.
///
/// The `VaultBackend` seam sits below the service and account names — `read`,
/// `write`, and `delete` take none — so no test driving a fake can observe
/// which entry the production backend actually reaches for. Reading the source
/// is what closes that gap: every `keyring::Entry::new` in the crate is the one
/// helper, and the consolidated operations pass it the two constants.
#[test]
fn keyring_entry_names_are_the_only_ones_used() {
    let source = include_str!("../../secret_vault.rs");
    assert_eq!(
        source.matches("keyring::Entry::new").count(),
        1,
        "every keyring entry must be opened through the one helper"
    );
    for operation in [
        "Self::read_entry(VAULT_SERVICE, VAULT_ACCOUNT)",
        "Self::entry(VAULT_SERVICE, VAULT_ACCOUNT)",
        "Self::delete_entry(VAULT_SERVICE, VAULT_ACCOUNT)",
    ] {
        assert!(
            source.contains(operation),
            "the consolidated entry must be opened as {operation} (ASV-FR-01)"
        );
    }
    // A legacy entry is only ever *read* or *deleted*; nothing writes one back
    // (ASV-FR-01: not for a secret, not for a copy, not for a value it could
    // not read). One `set_password` in the whole module, inside the one write
    // that targets the consolidated entry — a helper cannot smuggle a second.
    assert_eq!(
        source.matches("set_password").count(),
        1,
        "the module must store a credential in exactly one place"
    );
    // From the production impl, not the trait declaration above it.
    let backend = source
        .split_once("impl VaultBackend for KeyringVaultBackend")
        .expect("the production backend")
        .1;
    let write = backend
        .split_once("fn write(&self, value: &str)")
        .expect("the consolidated write")
        .1;
    let write = write.split_once("\n    fn ").expect("the end of write").0;
    assert!(
        write.contains("set_password"),
        "the one `set_password` must be the consolidated entry's own write"
    );
}

/// ASV-FR-03: the namespaces the owning modules reserve are distinct from each
/// other and from the two reserved root fields.
///
/// A collision would let one module read or overwrite another's secret, and the
/// per-level keyring service names that used to make that impossible are gone.
#[test]
fn the_owning_modules_reserve_distinct_namespaces() {
    let namespaces = [
        crate::github_tokens::VAULT_NAMESPACE,
        crate::ai_api::VAULT_NAMESPACE,
        crate::agentic::VAULT_NAMESPACE,
    ];
    for (i, a) in namespaces.iter().enumerate() {
        assert!(!a.is_empty());
        // Neither reserved root field is addressable, so a namespace naming one
        // would be silently unwritable.
        assert!(addressable(&path(a)));
        for b in namespaces.iter().skip(i + 1) {
            assert_ne!(a, b, "two modules reserve the same namespace");
        }
    }
}

/// Two `Set`s of one path in a single request: the last one wins, and the entry
/// never holds the first.
#[test]
fn the_last_set_of_a_path_in_a_request_wins() {
    let (vault, _keyring) = vault();

    vault
        .apply_secret_mutations(&[
            Mutation::Set {
                path: p(&["github", "tokens", "a"]),
                secret: "first".into(),
            },
            Mutation::Set {
                path: p(&["github", "tokens", "a"]),
                secret: "second".into(),
            },
        ])
        .unwrap();

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("second".into())
    );
}

/// A `Set` at a path that names an existing namespace rather than a leaf is
/// refused, because writing a string there would drop every secret beneath it.
#[test]
fn a_set_that_would_replace_a_namespace_is_refused() {
    let (vault, keyring) = vault();
    set(&vault, &["github", "tokens", "a"], "A").unwrap();

    // `["github","tokens"]` is an object holding a secret.
    assert_eq!(
        set(&vault, &["github", "tokens"], "clobber").unwrap_err(),
        VaultError::WriteFailed
    );
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );

    // And the mirror: a path *through* an existing leaf cannot create an object
    // where a secret sits.
    assert_eq!(
        set(&vault, &["github", "tokens", "a", "deeper"], "clobber").unwrap_err(),
        VaultError::WriteFailed
    );
    assert_eq!(
        keyring
            .object()
            .get("github")
            .and_then(|v| v.get("tokens"))
            .and_then(|v| v.get("a"))
            .and_then(Value::as_str),
        Some("A")
    );
}

/// An empty secret is a value like any other: this module gives a secret no
/// meaning, so it stores one and reports it present.
#[test]
fn an_empty_secret_is_stored_and_reads_as_present() {
    let (vault, _keyring) = vault();

    set(&vault, &["github", "tokens", "a"], "").unwrap();

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some(String::new())
    );
    let presence = vault
        .secret_presence(&[p(&["github", "tokens", "a"])])
        .unwrap();
    assert_eq!(presence.get(&p(&["github", "tokens", "a"])), Some(&true));
}

/// A candidate list naming one path twice adopts it once and deletes it once.
#[test]
fn a_duplicated_candidate_is_adopted_once() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");

    let outcome = vault
        .migrate_legacy_secrets(&[
            candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"]),
            candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"]),
        ])
        .unwrap();

    // The second sees the path already held (ASV-FR-21) and adopts nothing.
    assert_eq!(outcome.adopted, 1);
    assert_eq!(keyring.count(&Call::Write), 1);
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
    assert!(keyring.legacy(GITHUB_LEGACY, "a").is_none());
}

/// ASV-FR-06 in the round: a build that adds a secret type does not destroy the
/// secrets of a build that adds a different one.
#[test]
fn unknown_namespaces_survive_a_removal_too() {
    let (vault, keyring) = vault();
    keyring.set_entry(r#"{"version":1,"future":{"a":"kept"},"github":{"tokens":{"t":"T"}}}"#);

    vault
        .apply_secret_mutations(&[Mutation::Remove {
            path: p(&["github", "tokens", "t"]),
        }])
        .unwrap();

    let object = keyring.object();
    assert_eq!(
        object
            .get("future")
            .and_then(|v| v.get("a"))
            .and_then(Value::as_str),
        Some("kept")
    );
}

/// A `Remove` of a path that is not there succeeds, which is what makes the
/// owning modules' removals idempotent (GTS-FR-09, AAP-FR-15, AIC-FR-14).
#[test]
fn removing_an_absent_path_succeeds() {
    let (vault, _keyring) = vault();

    vault
        .apply_secret_mutations(&[Mutation::Remove {
            path: p(&["github", "tokens", "never-stored"]),
        }])
        .unwrap();
    vault
        .apply_secret_mutations(&[Mutation::Remove {
            path: p(&["no", "such", "namespace"]),
        }])
        .unwrap();
}

/// The version field is reserved the same way `quarantine` is: a path that
/// overwrote it would make the entry undecodable.
#[test]
fn the_version_field_is_not_addressable() {
    let (vault, keyring) = vault();
    set(&vault, &["github", "tokens", "a"], "A").unwrap();

    assert!(set(&vault, &["version"], "99").is_err());
    assert_eq!(
        keyring.object().get("version").and_then(Value::as_u64),
        Some(1)
    );
}
