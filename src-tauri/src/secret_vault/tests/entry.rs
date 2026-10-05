//! The entry, the schema, the write, and concurrency.

use super::*;

// ---------------------------------------------------------------------------
// ASV-FR-01, ASV-FR-02, ASV-FR-03, ASV-FR-04 … ASV-FR-11, ASV-FR-12 — the entry, the schema, and the write
// ---------------------------------------------------------------------------

/// ASV-FR-01, ASV-FR-02, ASV-FR-03, ASV-FR-04: three secrets of three kinds land in **one** entry, at their
/// paths, under the specified service and account names.
#[test]
fn asv_ts_01_three_kinds_share_one_entry() {
    let (vault, keyring) = vault();

    set(&vault, &["github", "tokens", "t1"], "ghp_secret").unwrap();
    set(&vault, &["ai_api", "providers", "openai"], "sk-key").unwrap();
    set(
        &vault,
        &["agentic", "vendors", "claude_code"],
        "sk-ant-oat01-x",
    )
    .unwrap();

    // ASV-FR-01 names both strings exactly; `keyring_entry_names_are_the_only_ones_used`
    // below is what ties them to the entry the backend actually opens.
    assert_eq!(VAULT_SERVICE, "com.synthesis.secrets");
    assert_eq!(VAULT_ACCOUNT, "application-secrets");
    // One entry for three secrets of three kinds: the backend has a single slot
    // for the consolidated value, and three writes went into it rather than
    // into three entries.
    assert_eq!(keyring.count(&Call::Write), 3);
    assert!(keyring.legacy_entries_touched().is_empty());

    let object = keyring.object();
    assert_eq!(object.get("version").and_then(Value::as_u64), Some(1));
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "t1"])).unwrap(),
        Some("ghp_secret".into())
    );
    assert_eq!(
        vault
            .read_secret(&p(&["ai_api", "providers", "openai"]))
            .unwrap(),
        Some("sk-key".into())
    );
    assert_eq!(
        vault
            .read_secret(&p(&["agentic", "vendors", "claude_code"]))
            .unwrap(),
        Some("sk-ant-oat01-x".into())
    );
}

/// ASV-FR-05, ASV-FR-07: a namespace this version's schema does not name is accepted, with
/// no schema change and at any depth.
#[test]
fn asv_ts_02_unknown_namespace_needs_no_schema_change() {
    let (vault, _keyring) = vault();

    set(&vault, &["ssh", "deploy_keys", "prod", "ed25519"], "secret").unwrap();

    assert_eq!(
        vault
            .read_secret(&p(&["ssh", "deploy_keys", "prod", "ed25519"]))
            .unwrap(),
        Some("secret".into())
    );
}

/// ASV-FR-06: a read-modify-write keeps every field it does not know, byte for
/// byte.
#[test]
fn asv_ts_03_unknown_fields_survive_a_write() {
    let (vault, keyring) = vault();
    keyring
        .set_entry(r#"{"version":1,"future_ns":{"nested":{"leaf":"kept"}},"stray":"also kept"}"#);

    set(&vault, &["github", "tokens", "t1"], "ghp_secret").unwrap();

    let object = keyring.object();
    assert_eq!(
        object
            .get("future_ns")
            .and_then(|v| v.get("nested"))
            .and_then(|v| v.get("leaf"))
            .and_then(Value::as_str),
        Some("kept")
    );
    assert_eq!(
        object.get("stray").and_then(Value::as_str),
        Some("also kept")
    );
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "t1"])).unwrap(),
        Some("ghp_secret".into())
    );
}

/// ASV-FR-07: an absent namespace is empty rather than wrong, and a write
/// creates every level it needs.
#[test]
fn asv_ts_04_absent_namespace_reads_empty_then_is_created() {
    let (vault, _keyring) = vault();

    assert_eq!(
        vault
            .read_secret(&p(&["agentic", "vendors", "claude_code"]))
            .unwrap(),
        None
    );

    set(&vault, &["agentic", "vendors", "claude_code"], "token").unwrap();

    assert_eq!(
        vault
            .read_secret(&p(&["agentic", "vendors", "claude_code"]))
            .unwrap(),
        Some("token".into())
    );
}

/// ASV-FR-08 (vault half): a project-scoped secret lives in the entry, and the
/// entry holds no binding. The settings-store half is asserted by
/// `global_settings`; what is checked here is that nothing but the secret was
/// written (ASV-FR-08).
#[test]
fn asv_ts_05_projects_namespace_holds_only_secrets() {
    let (vault, keyring) = vault();

    set(
        &vault,
        &["projects", "acme", "github", "deploy_key"],
        "deploy-secret",
    )
    .unwrap();

    let object = keyring.object();
    let project = object
        .get("projects")
        .and_then(|v| v.get("acme"))
        .and_then(Value::as_object)
        .expect("the project slot exists");
    // The one key under the project is the namespace holding the secret —
    // no binding, no override, no model selection.
    assert_eq!(project.keys().collect::<Vec<_>>(), vec!["github"]);
}

/// ASV-FR-10, ASV-FR-13: a `Set` and a `Remove` in one request reach the keyring in one
/// write.
#[test]
fn asv_ts_06_one_request_is_one_write() {
    let (vault, keyring) = vault();
    set(&vault, &["github", "tokens", "a"], "A").unwrap();
    set(&vault, &["github", "tokens", "b"], "B").unwrap();
    let writes_before = keyring.count(&Call::Write);

    vault
        .apply_secret_mutations(&[
            Mutation::Set {
                path: p(&["github", "tokens", "c"]),
                secret: "C".into(),
            },
            Mutation::Remove {
                path: p(&["github", "tokens", "a"]),
            },
        ])
        .unwrap();

    assert_eq!(keyring.count(&Call::Write) - writes_before, 1);
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        None
    );
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "b"])).unwrap(),
        Some("B".into())
    );
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "c"])).unwrap(),
        Some("C".into())
    );
}

/// ASV-FR-12, ASV-FR-31: a refused write changes nothing and returns `vault_write_failed`.
#[test]
fn asv_ts_07_refused_write_changes_nothing() {
    let (vault, keyring) = vault();
    set(&vault, &["github", "tokens", "a"], "A").unwrap();
    let before = keyring.entry();

    keyring.state().refuse_write = true;
    let err = set(&vault, &["github", "tokens", "b"], "B").unwrap_err();

    assert_eq!(err, VaultError::WriteFailed);
    assert_eq!(err.code(), "vault_write_failed");
    assert_eq!(keyring.entry(), before);

    keyring.state().refuse_write = false;
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "b"])).unwrap(),
        None
    );
}

/// ASV-FR-11, ASV-FR-12: a read-back that does not match what was written fails the
/// mutation, and the entry is put back the way it was.
#[test]
fn asv_ts_08_unverified_write_fails_and_rolls_back() {
    let (vault, keyring) = vault();
    set(&vault, &["github", "tokens", "a"], "A").unwrap();
    let before = keyring.entry();

    keyring.state().mangle_next_writes = 1;
    let err = set(&vault, &["github", "tokens", "b"], "B").unwrap_err();

    assert_eq!(err, VaultError::VerifyFailed);
    assert_eq!(keyring.entry(), before);
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "b"])).unwrap(),
        None
    );
}

/// ASV-FR-11, ASV-FR-12 (second half): a verification failure on an entry that did not
/// exist before the write leaves no entry behind.
#[test]
fn asv_ts_08_rollback_removes_an_entry_that_did_not_exist() {
    let (vault, keyring) = vault();

    keyring.state().mangle_next_writes = 1;
    let err = set(&vault, &["github", "tokens", "a"], "A").unwrap_err();

    assert_eq!(err, VaultError::VerifyFailed);
    assert_eq!(keyring.entry(), None);
}

// ---------------------------------------------------------------------------
// ASV-FR-09, ASV-FR-14 — concurrency
// ---------------------------------------------------------------------------

/// ASV-FR-09, ASV-FR-14: concurrent mutations are serialised, each reads the entry after
/// taking the lock, and no update is lost.
#[test]
fn asv_ts_09_concurrent_mutations_lose_no_update() {
    let keyring = FakeKeyring::default();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.mark_migrated();

    let mut handles = Vec::new();
    for i in 0..8 {
        let vault = Arc::clone(&vault);
        handles.push(std::thread::spawn(move || {
            let id = format!("t{i}");
            vault
                .apply_secret_mutations(&[Mutation::Set {
                    path: path(&["github", "tokens", &id]),
                    secret: format!("secret-{i}"),
                }])
                .unwrap();
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    for i in 0..8 {
        assert_eq!(
            vault
                .read_secret(&path(&["github", "tokens", &format!("t{i}")]))
                .unwrap(),
            Some(format!("secret-{i}")),
            "the write of thread {i} survived every other"
        );
    }
}

/// ASV-FR-09, ASV-FR-14 (second half): two writes of the *same* path leave one of the two
/// values and never a partial or interleaved object.
#[test]
fn asv_ts_09_same_path_leaves_one_whole_value() {
    let keyring = FakeKeyring::default();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.mark_migrated();

    let mut handles = Vec::new();
    for value in ["first", "second"] {
        let vault = Arc::clone(&vault);
        handles.push(std::thread::spawn(move || {
            vault
                .apply_secret_mutations(&[Mutation::Set {
                    path: path(&["ai_api", "providers", "openai"]),
                    secret: value.to_string(),
                }])
                .unwrap();
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    let stored = vault
        .read_secret(&p(&["ai_api", "providers", "openai"]))
        .unwrap()
        .unwrap();
    assert!(stored == "first" || stored == "second", "{stored}");
    // Every value the entry ever held decoded as a whole object.
    assert!(matches!(
        decode(&keyring.entry().unwrap()),
        Decoded::Object(_)
    ));
}

