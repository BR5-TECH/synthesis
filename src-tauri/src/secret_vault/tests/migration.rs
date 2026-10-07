//! Migration from the legacy entries.

use super::*;

// ---------------------------------------------------------------------------
// ASV-FR-20, ASV-FR-24, ASV-FR-25 … ASV-FR-26, ASV-FR-12 — migration
// ---------------------------------------------------------------------------

/// ASV-FR-20, ASV-FR-24, ASV-FR-25: three legacy entries of three kinds are adopted in one verified
/// write, and only then deleted.
#[test]
fn asv_ts_16_migration_adopts_then_deletes() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "t1", "ghp_secret");
    keyring.set_legacy(AI_API_LEGACY, "openai", "sk-key");
    keyring.set_legacy(AGENTIC_LEGACY, "claude_code", "sk-ant-oat01-x");

    let outcome = vault
        .migrate_legacy_secrets(&[
            candidate(GITHUB_LEGACY, "t1", &["github", "tokens", "t1"]),
            candidate(AI_API_LEGACY, "openai", &["ai_api", "providers", "openai"]),
            candidate(
                AGENTIC_LEGACY,
                "claude_code",
                &["agentic", "vendors", "claude_code"],
            ),
        ])
        .unwrap();

    assert_eq!(outcome.adopted, 3);
    assert_eq!(outcome.deleted, 3);
    assert_eq!(outcome.undeleted, 0);
    assert_eq!(outcome.unreadable, 0);
    assert_eq!(outcome.absent, 0);

    // ASV-FR-24: exactly one whole-object write.
    assert_eq!(keyring.count(&Call::Write), 1);
    // ASV-FR-25: the write and its verification precede every deletion.
    let calls = keyring.calls();
    let write_at = calls.iter().position(|c| *c == Call::Write).unwrap();
    let first_delete = calls
        .iter()
        .position(|c| matches!(c, Call::DeleteLegacy(..)))
        .unwrap();
    assert!(write_at < first_delete);
    // The read-back that verified it sits between them.
    assert!(calls[write_at + 1..first_delete].contains(&Call::Read));

    assert_eq!(keyring.legacy_count(), 0);
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

/// ASV-FR-21, ASV-FR-24, ASV-FR-25: where both hold a value for one path, the consolidated object
/// wins — and the legacy entry is still deleted after the same verified write.
#[test]
fn asv_ts_17_consolidated_value_wins_and_legacy_is_still_deleted() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_entry(r#"{"version":1,"ai_api":{"providers":{"openai":"kept"}}}"#);
    keyring.set_legacy(AI_API_LEGACY, "openai", "legacy value");

    let outcome = vault
        .migrate_legacy_secrets(&[candidate(
            AI_API_LEGACY,
            "openai",
            &["ai_api", "providers", "openai"],
        )])
        .unwrap();

    assert_eq!(outcome.adopted, 0);
    assert_eq!(outcome.deleted, 1);
    assert_eq!(
        vault
            .read_secret(&p(&["ai_api", "providers", "openai"]))
            .unwrap(),
        Some("kept".into())
    );

    let calls = keyring.calls();
    let write_at = calls.iter().position(|c| *c == Call::Write).unwrap();
    let delete_at = calls
        .iter()
        .position(|c| matches!(c, Call::DeleteLegacy(..)))
        .unwrap();
    assert!(write_at < delete_at, "the write precedes the deletion");
    assert!(calls[write_at + 1..delete_at].contains(&Call::Read));
    assert!(keyring.legacy(AI_API_LEGACY, "openai").is_none());
}

/// ASV-FR-22, ASV-FR-25: an unreadable legacy entry is skipped rather than fatal, is never
/// deleted, and is offered again at the next migration.
#[test]
fn asv_ts_18_unreadable_candidate_is_skipped_not_deleted() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    keyring.set_legacy(GITHUB_LEGACY, "b", "B");
    keyring.set_legacy(GITHUB_LEGACY, "c", "C");
    keyring
        .state()
        .refuse_legacy_read
        .push((GITHUB_LEGACY.to_string(), "c".to_string()));

    let candidates = vec![
        candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"]),
        candidate(GITHUB_LEGACY, "b", &["github", "tokens", "b"]),
        candidate(GITHUB_LEGACY, "c", &["github", "tokens", "c"]),
    ];
    let outcome = vault.migrate_legacy_secrets(&candidates).unwrap();

    assert_eq!(outcome.adopted, 2);
    assert_eq!(outcome.unreadable, 1);
    assert_eq!(outcome.deleted, 2);
    assert!(keyring.legacy(GITHUB_LEGACY, "a").is_none());
    assert!(keyring.legacy(GITHUB_LEGACY, "b").is_none());
    // The one it could not read still holds its value.
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "c").as_deref(), Some("C"));

    // Offered again, now readable.
    keyring.state().refuse_legacy_read.clear();
    let outcome = vault.migrate_legacy_secrets(&candidates).unwrap();
    assert_eq!(outcome.adopted, 1);
    assert_eq!(outcome.deleted, 1);
    assert_eq!(outcome.absent, 2);
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "c"])).unwrap(),
        Some("C".into())
    );
    assert_eq!(keyring.legacy_count(), 0);
}

/// ASV-FR-23, ASV-FR-24: candidates with no legacy entry at all adopt nothing, delete
/// nothing, and write nothing.
#[test]
fn asv_ts_19_absent_candidates_write_nothing() {
    let (vault, keyring) = unmigrated_vault();

    let outcome = vault
        .migrate_legacy_secrets(&[
            candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"]),
            candidate(AI_API_LEGACY, "openai", &["ai_api", "providers", "openai"]),
            candidate(
                AGENTIC_LEGACY,
                "claude_code",
                &["agentic", "vendors", "claude_code"],
            ),
        ])
        .unwrap();

    assert_eq!(outcome.absent, 3);
    assert_eq!(outcome.adopted, 0);
    assert_eq!(outcome.deleted, 0);
    assert_eq!(outcome.unreadable, 0);
    assert_eq!(keyring.count(&Call::Write), 0);
    assert_eq!(keyring.entry(), None);
}

/// ASV-FR-26, ASV-FR-25: a legacy entry the keyring refuses to delete stays where it is
/// and is deleted at the next migration, adopting nothing twice.
#[test]
fn asv_ts_20_undeleted_entry_is_retried() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    keyring.set_legacy(GITHUB_LEGACY, "b", "B");
    keyring.set_legacy(GITHUB_LEGACY, "c", "C");
    keyring
        .state()
        .refuse_legacy_delete
        .push((GITHUB_LEGACY.to_string(), "c".to_string()));

    let candidates = vec![
        candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"]),
        candidate(GITHUB_LEGACY, "b", &["github", "tokens", "b"]),
        candidate(GITHUB_LEGACY, "c", &["github", "tokens", "c"]),
    ];
    let outcome = vault.migrate_legacy_secrets(&candidates).unwrap();

    assert_eq!(outcome.adopted, 3);
    assert_eq!(outcome.deleted, 2);
    assert_eq!(outcome.undeleted, 1);
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "c").as_deref(), Some("C"));

    keyring.state().refuse_legacy_delete.clear();
    let writes_before = keyring.count(&Call::Write);
    let outcome = vault.migrate_legacy_secrets(&candidates).unwrap();

    // Nothing adopted a second time: the consolidated object already holds it.
    assert_eq!(outcome.adopted, 0);
    assert_eq!(outcome.deleted, 1);
    assert_eq!(outcome.absent, 2);
    // The object was written and verified once more before the deletion.
    assert_eq!(keyring.count(&Call::Write) - writes_before, 1);
    assert_eq!(keyring.legacy_count(), 0);
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "c"])).unwrap(),
        Some("C".into())
    );
}

/// ASV-FR-26, ASV-FR-12: a migration whose consolidated write is refused leaves every
/// legacy entry and the consolidated entry exactly as they were.
#[test]
fn asv_ts_21_refused_migration_write_deletes_nothing() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_entry(r#"{"version":1}"#);
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    keyring.set_legacy(GITHUB_LEGACY, "b", "B");
    keyring.set_legacy(GITHUB_LEGACY, "c", "C");
    keyring.state().refuse_write = true;

    let err = vault
        .migrate_legacy_secrets(&[
            candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"]),
            candidate(GITHUB_LEGACY, "b", &["github", "tokens", "b"]),
            candidate(GITHUB_LEGACY, "c", &["github", "tokens", "c"]),
        ])
        .unwrap_err();

    assert_eq!(err, VaultError::WriteFailed);
    assert_eq!(keyring.legacy_count(), 3);
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "a").as_deref(), Some("A"));
    assert_eq!(keyring.entry().as_deref(), Some(r#"{"version":1}"#));
    assert_eq!(
        keyring
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::DeleteLegacy(..)))
            .count(),
        0
    );
}

/// ASV-FR-20: migration runs once per process, before the first read the
/// process performs — and is not repeated on the operations after it.
#[test]
fn asv_fr_20_migration_precedes_the_first_operation() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    vault.set_candidate_source(Box::new(|| {
        vec![candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])]
    }));

    // The very first operation is a read, and it already sees the adopted value.
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
    assert_eq!(keyring.legacy_count(), 0);

    let reads_of_legacy = keyring
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::ReadLegacy(..)))
        .count();
    vault.read_secret(&p(&["github", "tokens", "a"])).unwrap();
    assert_eq!(
        keyring
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::ReadLegacy(..)))
            .count(),
        reads_of_legacy,
        "migration did not run a second time"
    );
}

/// ASV-FR-17 read against ASV-FR-20: a **read** of a quarantined entry reports
/// `vault_malformed` and writes nothing and deletes nothing — even where
/// migration is pending and has legacy entries it could adopt.
///
/// The tempting implementation has migration recover the entry on the way past,
/// which would make a plain read write to the keyring, delete the legacy
/// entries, and then answer `Ok`. Nothing about the caller's request asked for
/// any of that.
#[test]
fn a_read_of_a_quarantined_entry_never_migrates_over_it() {
    let (vault, keyring) = unmigrated_vault();
    let garbage = "not an AppSecrets object";
    keyring.set_entry(garbage);
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    vault.set_candidate_source(Box::new(|| {
        vec![candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])]
    }));

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])),
        Err(VaultError::Malformed)
    );

    assert_eq!(keyring.entry().as_deref(), Some(garbage));
    assert_eq!(keyring.count(&Call::Write), 0);
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "a").as_deref(), Some("A"));
    // Not one legacy entry was even read, so a quarantined entry costs no
    // keyring access per candidate either.
    assert!(keyring.legacy_entries_touched().is_empty());
}

/// The other half of the same rule: the first **mutation** quarantines the
/// value (ASV-FR-17), and the operation after it — now finding a decodable
/// entry — migrates normally. Nothing was lost on the way through.
#[test]
fn a_mutation_quarantines_and_the_next_operation_migrates() {
    let (vault, keyring) = unmigrated_vault();
    let garbage = "not an AppSecrets object";
    keyring.set_entry(garbage);
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    vault.set_candidate_source(Box::new(|| {
        vec![candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])]
    }));

    set(&vault, &["github", "tokens", "b"], "B").unwrap();

    // The value is quarantined and the legacy entry is still untouched.
    assert_eq!(
        keyring.object().get("quarantine").and_then(Value::as_str),
        Some(garbage)
    );
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "a").as_deref(), Some("A"));

    // The next operation migrates.
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
    assert!(keyring.legacy(GITHUB_LEGACY, "a").is_none());
    assert_eq!(
        keyring.object().get("quarantine").and_then(Value::as_str),
        Some(garbage),
        "the quarantined value survives the migration write too"
    );
}

/// ASV-FR-20 and the non-functional budget: a migration that failed **after**
/// reading the legacy entries is not retried for the life of the process.
///
/// The alternative costs one read of every legacy entry on every subsequent
/// secret operation — one authentication prompt per stored secret per
/// operation on a platform that prompts per access. ASV-FR-26 covers the
/// recovery: the next launch migrates again.
#[test]
fn a_migration_that_failed_after_reading_is_not_retried_this_process() {
    let (vault, keyring) = unmigrated_vault();
    for id in ["a", "b", "c"] {
        keyring.set_legacy(GITHUB_LEGACY, id, "secret");
    }
    keyring.state().refuse_write = true;
    vault.set_candidate_source(Box::new(|| {
        ["a", "b", "c"]
            .iter()
            .map(|id| candidate(GITHUB_LEGACY, id, &["github", "tokens", id]))
            .collect()
    }));

    for _ in 0..3 {
        let _ = vault.read_secret(&p(&["github", "tokens", "a"]));
    }

    let legacy_reads = keyring
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::ReadLegacy(..)))
        .count();
    assert_eq!(legacy_reads, 3, "each legacy entry was read once, not once per operation");
    // And nothing was deleted, because the write never verified.
    assert_eq!(keyring.legacy_count(), 3);
}

/// The mirror: a migration that failed **before** reading a legacy entry is
/// retried, because the retry costs one read of the one entry and the state it
/// failed on is one a mutation can clear.
#[test]
fn a_migration_that_failed_before_reading_is_retried() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_entry("not an AppSecrets object");
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    vault.set_candidate_source(Box::new(|| {
        vec![candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])]
    }));

    // Two reads against the quarantined entry: both refuse, neither migrates.
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])),
        Err(VaultError::Malformed)
    );
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])),
        Err(VaultError::Malformed)
    );
    assert!(keyring.legacy_entries_touched().is_empty());

    // A mutation clears the state, and migration then runs after all.
    set(&vault, &["github", "tokens", "b"], "B").unwrap();
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
}

/// ASV-FR-22 read with ASV-FR-25, at its sharpest: a candidate whose secret
/// could **not** be adopted keeps its legacy entry.
///
/// A path can be blocked — a level of it occupied by something that is not an
/// object — and `insert_at` then refuses. This is the one branch where deleting
/// the legacy entry would destroy the author's only copy of the secret while
/// the consolidated object holds nothing at that path, and the write that
/// precedes the deletion succeeds, so ASV-FR-25's ordering guard does not catch
/// it. Move the deletion out of the success arm and the entry is gone and the
/// secret with it.
#[test]
fn a_candidate_whose_path_is_blocked_keeps_its_legacy_entry() {
    let (vault, keyring) = unmigrated_vault();
    // `["github","tokens","a"]` holds a secret, so `["github","tokens","a","b"]`
    // cannot be created without destroying it.
    keyring.set_entry(r#"{"version":1,"github":{"tokens":{"a":"A"}}}"#);
    keyring.set_legacy(GITHUB_LEGACY, "blocked", "the only copy");
    keyring.set_legacy(GITHUB_LEGACY, "fine", "adoptable");

    let outcome = vault
        .migrate_legacy_secrets(&[
            candidate(GITHUB_LEGACY, "blocked", &["github", "tokens", "a", "b"]),
            candidate(GITHUB_LEGACY, "fine", &["github", "tokens", "fine"]),
        ])
        .unwrap();

    assert_eq!(outcome.adopted, 1, "only the unblocked candidate was adopted");
    assert_eq!(outcome.deleted, 1);
    // The blocked candidate's entry is still there, still holding its value.
    assert_eq!(
        keyring.legacy(GITHUB_LEGACY, "blocked").as_deref(),
        Some("the only copy"),
        "a secret that was adopted nowhere must keep its legacy entry"
    );
    assert!(keyring.legacy(GITHUB_LEGACY, "fine").is_none());
    // And the secret that blocked it is untouched.
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
    // The refusal is reported, because a candidate counted in none of the five
    // outcome fields is otherwise invisible to the caller.
    assert!(vault
        .emitted()
        .iter()
        .any(|(message, _)| message == "secret vault could not adopt a legacy secret"));
}

/// ASV-FR-19 read with the read budget: an entry a newer build wrote is not
/// re-migrated against on every operation.
///
/// No mutation of this build can clear `vault_unsupported_version` (ASV-FR-19
/// refuses every one), so a migration that retried on it would cost a second
/// keyring read — and a second authentication prompt where the platform asks —
/// on every secret operation for the life of the process.
#[test]
fn an_entry_from_a_newer_build_is_not_re_migrated_against() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_entry(r#"{"version":99}"#);
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    vault.set_candidate_source(Box::new(|| {
        vec![candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])]
    }));

    // The first operation attempts migration, finds an entry it must not touch,
    // and stands down for good. Each later operation is an independent request
    // and starts its own initialization attempt (ASV-FR-FTFU).
    for _ in 0..4 {
        assert_eq!(
            vault.read_secret(&p(&["github", "tokens", "a"])),
            Err(VaultError::UnsupportedVersion)
        );
    }

    // One read per operation: the first attempt's migration read the entry
    // once, and the later attempts read it once each without migrating.
    assert_eq!(keyring.count(&Call::Read), 4);
    assert_eq!(keyring.entry().as_deref(), Some(r#"{"version":99}"#));
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "a").as_deref(), Some("A"));
}

/// The mirror of the rule above: `vault_unavailable` **is** retried, because a
/// keychain that unlocks mid-session should migrate rather than wait for a
/// relaunch. The retry costs one read of the one entry, not one per candidate.
#[test]
fn a_keychain_that_unlocks_mid_session_still_migrates() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    vault.set_candidate_source(Box::new(|| {
        vec![candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])]
    }));
    keyring.state().refuse_read = true;

    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])),
        Err(VaultError::Unavailable)
    );
    assert!(keyring.legacy_entries_touched().is_empty());

    keyring.state().refuse_read = false;
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
    assert!(keyring.legacy(GITHUB_LEGACY, "a").is_none());
}

/// The contract surface's own return: the public operation reports
/// `vault_malformed` rather than recovering (ASV-FR-17).
#[test]
fn the_public_migration_refuses_an_undecodable_entry() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_entry("not an AppSecrets object");
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");

    let err = vault
        .migrate_legacy_secrets(&[candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])])
        .unwrap_err();

    assert_eq!(err, VaultError::Malformed);
    assert_eq!(keyring.count(&Call::Write), 0);
    assert!(keyring.legacy_entries_touched().is_empty());
}

/// ASV-FR-20: a vault with no candidate source yet does not consume its one
/// migration. The source is installed at startup, and an operation that ran
/// first must not cost the process its adoption.
#[test]
fn an_absent_candidate_source_does_not_consume_the_migration() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");

    // No source installed yet.
    assert_eq!(vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(), None);
    assert!(keyring.legacy_entries_touched().is_empty());

    vault.set_candidate_source(Box::new(|| {
        vec![candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])]
    }));
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "a"])).unwrap(),
        Some("A".into())
    );
}

/// ASV-FR-09 / ASV-FR-20: migration runs exactly once however many threads
/// race to be the first operation.
#[test]
fn migration_runs_once_however_many_threads_race_for_it() {
    let keyring = FakeKeyring::default();
    for i in 0..3 {
        keyring.set_legacy(GITHUB_LEGACY, &format!("t{i}"), "secret");
    }
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.set_candidate_source(Box::new(|| {
        (0..3)
            .map(|i| {
                let id = format!("t{i}");
                Candidate {
                    legacy_service: GITHUB_LEGACY.to_string(),
                    legacy_account: id.clone(),
                    path: path(&["github", "tokens", &id]),
                }
            })
            .collect()
    }));

    let mut handles = Vec::new();
    for i in 0..8 {
        let vault = Arc::clone(&vault);
        handles.push(std::thread::spawn(move || {
            vault
                .read_secret(&path(&["github", "tokens", &format!("t{}", i % 3)]))
                .unwrap()
        }));
    }
    for handle in handles {
        assert_eq!(handle.join().unwrap(), Some("secret".to_string()));
    }

    let legacy_reads = keyring
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::ReadLegacy(..)))
        .count();
    assert_eq!(legacy_reads, 3, "three candidates, read once each");
}

/// ASV-FR-25 in its worst case: a migration whose write is made but does not
/// verify deletes nothing, and the legacy entries still hold their values.
///
/// The `refuse_write` variant (ASV-FR-26, ASV-FR-12) never reaches the rollback path; this
/// one does, which is where a bug would cost the author their only copy.
#[test]
fn a_migration_whose_write_does_not_verify_deletes_nothing() {
    let (vault, keyring) = unmigrated_vault();
    for id in ["a", "b", "c"] {
        keyring.set_legacy(GITHUB_LEGACY, id, "secret");
    }
    keyring.state().mangle_next_writes = 1;

    let err = vault
        .migrate_legacy_secrets(
            &["a", "b", "c"]
                .iter()
                .map(|id| candidate(GITHUB_LEGACY, id, &["github", "tokens", id]))
                .collect::<Vec<_>>(),
        )
        .unwrap_err();

    assert_eq!(err, VaultError::VerifyFailed);
    assert_eq!(keyring.legacy_count(), 3);
    for id in ["a", "b", "c"] {
        assert_eq!(keyring.legacy(GITHUB_LEGACY, id).as_deref(), Some("secret"));
    }
    assert_eq!(
        keyring
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::DeleteLegacy(..)))
            .count(),
        0
    );
}

/// ASV-FR-12 read with ASV-FR-17: a quarantine recovery whose write does not
/// verify puts the undecodable value back, byte for byte.
#[test]
fn a_quarantine_recovery_that_does_not_verify_restores_the_garbage() {
    let (vault, keyring) = vault();
    let garbage = "not an AppSecrets object";
    keyring.set_entry(garbage);
    keyring.state().mangle_next_writes = 1;

    let err = set(&vault, &["github", "tokens", "a"], "A").unwrap_err();

    assert_eq!(err, VaultError::VerifyFailed);
    assert_eq!(keyring.entry().as_deref(), Some(garbage));
}

/// ASV-FR-12: a rollback the keyring also refuses is still reported as a
/// failure, and the log record says the entry was not put back.
///
/// This is the one outcome nothing here can improve on, so what matters is that
/// it is neither hidden nor reported as success.
#[test]
fn a_rollback_the_keyring_refuses_is_still_a_reported_failure() {
    let (vault, keyring) = vault();
    set(&vault, &["github", "tokens", "a"], "A").unwrap();
    {
        let mut state = keyring.state();
        state.mangle_next_writes = 1;
        // The next write after the mangled one — the rollback — is refused.
        state.refuse_write_from = Some(state.writes + 1);
    }

    let err = set(&vault, &["github", "tokens", "b"], "B").unwrap_err();

    assert_eq!(err, VaultError::VerifyFailed);
    let rolled_back = vault
        .emitted()
        .iter()
        .find(|(message, _)| message == "secret vault write could not be verified")
        .and_then(|(_, fields)| fields.get("rolled_back").cloned())
        .expect("the failed verification was logged");
    assert_eq!(rolled_back, serde_json::json!(false));
}

/// ASV-FR-11: a read-back that finds no entry at all is a failed verification,
/// not a successful write.
#[test]
fn a_read_back_that_finds_nothing_fails_verification() {
    let (vault, keyring) = vault();
    keyring.state().vanish_after_write = true;

    let err = set(&vault, &["github", "tokens", "a"], "A").unwrap_err();

    assert_eq!(err, VaultError::VerifyFailed);
}

/// ASV-FR-12: a migration whose read of the consolidated entry is refused
/// deletes nothing and reads no legacy entry.
#[test]
fn a_migration_that_cannot_read_the_entry_touches_no_legacy_entry() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "A");
    keyring.state().refuse_read = true;

    let err = vault
        .migrate_legacy_secrets(&[candidate(GITHUB_LEGACY, "a", &["github", "tokens", "a"])])
        .unwrap_err();

    assert_eq!(err, VaultError::Unavailable);
    assert!(keyring.legacy_entries_touched().is_empty());
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "a").as_deref(), Some("A"));
}

/// ASV-FR-24: migration reads every legacy entry it is going to adopt *before*
/// it writes, rather than writing once per candidate.
#[test]
fn migration_collects_every_candidate_before_it_writes() {
    let (vault, keyring) = unmigrated_vault();
    for id in ["a", "b", "c"] {
        keyring.set_legacy(GITHUB_LEGACY, id, "secret");
    }

    vault
        .migrate_legacy_secrets(
            &["a", "b", "c"]
                .iter()
                .map(|id| candidate(GITHUB_LEGACY, id, &["github", "tokens", id]))
                .collect::<Vec<_>>(),
        )
        .unwrap();

    let calls = keyring.calls();
    let write_at = calls.iter().position(|c| *c == Call::Write).unwrap();
    let last_legacy_read = calls
        .iter()
        .rposition(|c| matches!(c, Call::ReadLegacy(..)))
        .unwrap();
    assert!(last_legacy_read < write_at);
    assert_eq!(keyring.count(&Call::Write), 1);
}

/// ASV-FR-09 / ASV-FR-14, with the read-modify-write window widened until a
/// lockless implementation could not survive it.
///
/// Every other concurrency test here would pass against a vault with no lock at
/// all, because the fake keyring's own mutex makes each access atomic and the
/// threads rarely interleave. A delay inside the read is what turns "might
/// interleave" into "must".
#[test]
fn concurrent_mutations_are_serialised_even_with_a_slow_read() {
    let keyring = FakeKeyring::default();
    keyring.state().read_delay = std::time::Duration::from_millis(20);
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.mark_migrated();

    let mut handles = Vec::new();
    for i in 0..4 {
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

    for i in 0..4 {
        assert_eq!(
            vault
                .read_secret(&path(&["github", "tokens", &format!("t{i}")]))
                .unwrap(),
            Some(format!("secret-{i}")),
            "the write of thread {i} survived every other"
        );
    }
}

/// ASV-FR-13: a request's mutations reach the keyring in one write, so a
/// reader between two requests sees all of them or none.
///
/// The guarantee is structural — `apply_secret_mutations` serialises one object
/// and writes it once — so this cannot fail while that shape holds; it is here
/// to fail if a future edit ever writes per mutation, which is the shape that
/// would make a half-applied observation possible.
#[test]
fn presence_never_observes_half_a_request() {
    let keyring = FakeKeyring::default();
    keyring.state().read_delay = std::time::Duration::from_millis(10);
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.mark_migrated();

    let writer = {
        let vault = Arc::clone(&vault);
        std::thread::spawn(move || {
            for _ in 0..4 {
                vault
                    .apply_secret_mutations(&[
                        Mutation::Set {
                            path: path(&["github", "tokens", "a"]),
                            secret: "A".into(),
                        },
                        Mutation::Set {
                            path: path(&["github", "tokens", "b"]),
                            secret: "B".into(),
                        },
                    ])
                    .unwrap();
                vault
                    .apply_secret_mutations(&[
                        Mutation::Remove {
                            path: path(&["github", "tokens", "a"]),
                        },
                        Mutation::Remove {
                            path: path(&["github", "tokens", "b"]),
                        },
                    ])
                    .unwrap();
            }
        })
    };

    let paths = vec![p(&["github", "tokens", "a"]), p(&["github", "tokens", "b"])];
    for _ in 0..20 {
        let answers = vault.secret_presence(&paths).unwrap();
        let a = answers[&paths[0]];
        let b = answers[&paths[1]];
        assert_eq!(a, b, "a request's two paths were observed half-applied");
    }
    writer.join().unwrap();
}

/// A panic inside a held lock must not cost the process every subsequent secret
/// operation. `acquire` recovers the poisoned lock deliberately.
#[test]
fn a_poisoned_lock_does_not_wedge_the_vault() {
    let keyring = FakeKeyring::default();
    let vault = Arc::new(Vault::new(Box::new(PanickingOnce {
        inner: keyring.clone(),
        panics: Arc::new(std::sync::atomic::AtomicBool::new(true)),
    })));
    vault.mark_migrated();

    let poisoner = {
        let vault = Arc::clone(&vault);
        std::thread::spawn(move || set(&vault, &["github", "tokens", "a"], "A"))
    };
    assert!(poisoner.join().is_err(), "the backend panicked under the lock");

    // The vault still serves.
    set(&vault, &["github", "tokens", "b"], "B").unwrap();
    assert_eq!(
        vault.read_secret(&p(&["github", "tokens", "b"])).unwrap(),
        Some("B".into())
    );
}

/// A backend that panics on its first read, to poison the vault's lock.
struct PanickingOnce {
    inner: FakeKeyring,
    panics: Arc<std::sync::atomic::AtomicBool>,
}

impl VaultBackend for PanickingOnce {
    fn read(&self) -> Result<Option<String>, String> {
        if self
            .panics
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("the keyring blew up");
        }
        self.inner.read()
    }
    fn write(&self, value: &str) -> Result<(), String> {
        self.inner.write(value)
    }
    fn delete(&self) -> Result<(), String> {
        self.inner.delete()
    }
    fn read_legacy(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        self.inner.read_legacy(service, account)
    }
    fn delete_legacy(&self, service: &str, account: &str) -> Result<(), String> {
        self.inner.delete_legacy(service, account)
    }
}

/// ASV-FR-09: "one process-wide lock" rests entirely on there being one vault.
#[test]
fn the_global_vault_is_one_instance() {
    assert!(Arc::ptr_eq(&global(), &global()));
}

