//! The four states of the entry.

use super::*;

// ---------------------------------------------------------------------------
// ASV-FR-15 … ASV-FR-19 — the four states of the entry
// ---------------------------------------------------------------------------

/// ASV-FR-15: a keyring that refuses a read is not a path that holds nothing.
#[test]
fn asv_ts_10_refused_read_is_unavailable_not_empty() {
    let (vault, keyring) = vault();
    keyring.state().refuse_read = true;

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])),
        Err(VaultError::Unavailable)
    );
    assert_eq!(
        vault
            .secret_presence(&[p(&["github", "tokens", "a"])])
            .unwrap_err(),
        VaultError::Unavailable
    );
    assert_eq!(VaultError::Unavailable.code(), "vault_unavailable");
}

/// ASV-FR-16: no entry at all is the state of a machine that has stored no
/// secret, and the first mutation creates it.
#[test]
fn asv_ts_11_absent_entry_reads_empty_and_is_created() {
    let (vault, keyring) = vault();
    assert_eq!(keyring.entry(), None);

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        None
    );
    let presence = vault
        .secret_presence(&[
            p(&["github", "tokens", "a"]),
            p(&["ai_api", "providers", "x"]),
        ])
        .unwrap();
    assert!(presence.values().all(|v| !v));

    set(&vault, &["github", "tokens", "a"], "A").unwrap();

    let object = keyring.object();
    assert_eq!(object.get("version").and_then(Value::as_u64), Some(1));
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
}

/// ASV-FR-17, ASV-FR-18, ASV-FR-01: an undecodable value quarantines the entry — reads refuse, the
/// value is untouched, and the next mutation carries it verbatim into the one
/// object it writes.
#[test]
fn asv_ts_12_malformed_value_is_quarantined_by_the_next_mutation() {
    let (vault, keyring) = vault();
    let garbage = "not json at all";
    keyring.set_entry(garbage);

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])),
        Err(VaultError::Malformed)
    );
    assert_eq!(
        vault
            .secret_presence(&[p(&["github", "tokens", "a"])])
            .unwrap_err(),
        VaultError::Malformed
    );
    // Nothing was written and nothing was deleted.
    assert_eq!(keyring.entry().as_deref(), Some(garbage));
    assert_eq!(keyring.count(&Call::Write), 0);
    assert_eq!(keyring.count(&Call::Delete), 0);

    set(&vault, &["github", "tokens", "a"], "A").unwrap();

    let object = keyring.object();
    assert_eq!(
        object.get("quarantine").and_then(Value::as_str),
        Some(garbage)
    );
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
}

/// ASV-FR-17, ASV-FR-12, ASV-FR-01: a quarantined entry plus a refused write leaves the undecodable
/// value where it is, and creates no second entry.
#[test]
fn asv_ts_13_quarantine_write_failure_leaves_the_value() {
    let (vault, keyring) = vault();
    let garbage = "{ this is not }";
    keyring.set_entry(garbage);
    keyring.state().refuse_write = true;

    let err = set(&vault, &["github", "tokens", "a"], "A").unwrap_err();

    assert_eq!(err, VaultError::WriteFailed);
    assert_eq!(keyring.entry().as_deref(), Some(garbage));
    // Nothing was written anywhere else either: the one write attempted was the
    // refused one, and no other entry was so much as named.
    assert_eq!(keyring.count(&Call::Write), 1);
    assert!(keyring.legacy_entries_touched().is_empty());
}

/// ASV-FR-18, ASV-FR-28, ASV-FR-29: the quarantine field survives later mutations unchanged, no path
/// resolves it, and it appears in no return value.
#[test]
fn asv_ts_14_quarantine_is_carried_and_never_addressable() {
    let (vault, keyring) = vault();
    let garbage = "secret-looking garbage";
    keyring.set_entry(garbage);
    set(&vault, &["github", "tokens", "a"], "A").unwrap();

    set(&vault, &["ai_api", "providers", "openai"], "K").unwrap();
    vault
        .apply_secret_mutations(&[Mutation::Remove {
            path: p(&["github", "tokens", "a"]),
        }])
        .unwrap();

    assert_eq!(
        keyring.object().get("quarantine").and_then(Value::as_str),
        Some(garbage)
    );
    // ASV-FR-18: no `SecretPath` resolves the reserved field, whatever shape it
    // is asked for.
    assert_eq!(vault.read_secret(&p(&["quarantine"])).unwrap(), None);
    assert_eq!(vault.read_secret(&p(&["quarantine", "x"])).unwrap(), None);
    let presence = vault.secret_presence(&[p(&["quarantine"])]).unwrap();
    assert_eq!(presence.get(&p(&["quarantine"])), Some(&false));
    // Nor can a mutation write to it or remove it.
    assert!(set(&vault, &["quarantine"], "overwritten").is_err());
    assert_eq!(
        keyring.object().get("quarantine").and_then(Value::as_str),
        Some(garbage)
    );
}

/// ASV-FR-18, ASV-FR-28, ASV-FR-29 (second half): a value quarantined a second time is carried whole
/// rather than accumulating a chain of fields.
#[test]
fn asv_ts_14_second_quarantine_does_not_accumulate() {
    let (vault, keyring) = vault();
    keyring.set_entry("first garbage");
    set(&vault, &["github", "tokens", "a"], "A").unwrap();
    let first_object = keyring.entry().unwrap();

    // The whole prior value — quarantine field and all — becomes garbage again.
    keyring.set_entry(&format!("corrupted: {first_object}"));
    set(&vault, &["github", "tokens", "b"], "B").unwrap();

    let object = keyring.object();
    let quarantined = object.get("quarantine").and_then(Value::as_str).unwrap();
    assert!(quarantined.starts_with("corrupted: "));
    // Exactly one such field, holding the prior value whole.
    assert_eq!(
        object.keys().filter(|k| k.contains("quarantine")).count(),
        1
    );
}

/// ASV-FR-19: an object from a newer build is never overwritten and never
/// quarantined.
#[test]
fn asv_ts_15_unsupported_version_refuses_everything() {
    let (vault, keyring) = vault();
    let future = r#"{"version":99,"github":{"tokens":{"a":"A"}}}"#;
    keyring.set_entry(future);

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])),
        Err(VaultError::UnsupportedVersion)
    );
    assert_eq!(
        set(&vault, &["github", "tokens", "b"], "B").unwrap_err(),
        VaultError::UnsupportedVersion
    );
    assert_eq!(
        vault
            .secret_presence(&[p(&["github", "tokens", "a"])])
            .unwrap_err(),
        VaultError::UnsupportedVersion
    );

    assert_eq!(keyring.entry().as_deref(), Some(future));
    assert_eq!(keyring.count(&Call::Write), 0);
    assert_eq!(keyring.count(&Call::Delete), 0);
}

