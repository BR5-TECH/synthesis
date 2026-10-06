//! The process cache — `ASV-application-secret-vault.md` ASV-FR-ZUGZ,
//! ASV-FR-ZRWU, ASV-FR-DYCW, ASV-FR-CIDB, ASV-FR-FTFU, ASV-FR-DQHY,
//! ASV-FR-GLJD, ASV-FR-DIEF, ASV-FR-SUXZ, ASV-FR-ELQN.
//!
//! No test here waits for time. A scenario that needs requests to meet inside
//! the keyring holds the fake's reads and waits for a state: the number of
//! requests that wait at the gate.

use super::*;
use crate::secret_vault::cache::Cached;

const A: &[&str] = &["github", "tokens", "a"];
const B: &[&str] = &["ai_api", "providers", "b"];
const C: &[&str] = &["agentic", "vendors", "c"];

/// A vault over a keyring that already holds three secrets in three
/// namespaces and one namespace this build does not name.
fn stocked() -> (Vault, FakeKeyring) {
    let keyring = FakeKeyring::default();
    keyring.set_entry(
        r#"{"version":1,"github":{"tokens":{"a":"A"}},"ai_api":{"providers":{"b":"B"}},"agentic":{"vendors":{"c":"C"}},"later":{"x":{"y":"Y"}}}"#,
    );
    let vault = Vault::new(Box::new(keyring.clone()));
    vault.mark_migrated();
    (vault, keyring)
}

/// Wait, without a sleep, until `count` requests wait at the gate and the
/// attempt that they wait for has reached the keyring.
fn wait_for_waiters(vault: &Vault, keyring: &FakeKeyring, count: usize) {
    // The bound turns a gate that never fills into a failure, not a hang.
    for _ in 0..50_000_000u64 {
        if vault.gate.waiting() >= count && keyring.count(&Call::Read) >= 1 {
            return;
        }
        std::thread::yield_now();
    }
    panic!("the requests never reached the gate");
}

/// ASV-FR-ZUGZ: building the vault and installing its sources reads no entry.
#[test]
fn startup_reads_no_keyring_entry() {
    let keyring = FakeKeyring::default();
    let vault = Vault::new(Box::new(keyring.clone()));
    vault.set_candidate_source(Box::new(Vec::new));
    assert!(keyring.calls().is_empty());
    assert!(vault.cache_slot().is_none());
}

/// ASV-FR-ZUGZ, ASV-FR-ZRWU, ASV-FR-DYCW: the first read initializes the cache
/// from one read of the entry, and the cache then holds every secret.
#[test]
fn the_first_read_initializes_the_cache_with_one_read() {
    let (vault, keyring) = stocked();

    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));

    assert_eq!(keyring.calls(), vec![Call::Read]);
    let slot = vault.cache_slot();
    let cached = slot.as_ref().expect("the cache is initialized");
    assert_eq!(lookup(&cached.object, &p(B)), Some("B".into()));
    assert_eq!(lookup(&cached.object, &p(C)), Some("C".into()));
    assert_eq!(lookup(&cached.object, &p(&["later", "x", "y"])), Some("Y".into()));
}

/// ASV-FR-ZUGZ: a presence check is a secret-dependent operation and
/// initializes the cache.
#[test]
fn the_first_presence_check_initializes_the_cache() {
    let (vault, keyring) = stocked();

    let answers = vault.secret_presence(&[p(A), p(&["github", "tokens", "z"])]).unwrap();

    assert_eq!(answers.get(&p(A)), Some(&true));
    assert_eq!(answers.get(&p(&["github", "tokens", "z"])), Some(&false));
    assert_eq!(keyring.calls(), vec![Call::Read]);
}

/// ASV-FR-DQHY, ASV-FR-DYCW, ASV-FR-30: after initialization, reads and
/// presence checks of every secret make no keyring access.
#[test]
fn cached_reads_and_presence_checks_make_no_keyring_access() {
    let (vault, keyring) = stocked();
    vault.read_secret(&p(A)).unwrap();
    let before = keyring.calls().len();

    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
    assert_eq!(vault.read_secret(&p(B)).unwrap(), Some("B".into()));
    assert_eq!(vault.read_secret(&p(C)).unwrap(), Some("C".into()));
    assert_eq!(vault.read_secret(&p(&["github", "tokens", "z"])).unwrap(), None);
    let answers = vault.secret_presence(&[p(A), p(B), p(C)]).unwrap();
    assert!(answers.values().all(|present| *present));

    assert_eq!(keyring.calls().len(), before);
}

/// ASV-FR-ELQN: the cache serves the value it holds. A change that another
/// process makes to the entry is not seen, and the module does not claim it.
#[test]
fn a_change_made_outside_the_process_is_not_seen() {
    let (vault, keyring) = stocked();
    vault.read_secret(&p(A)).unwrap();

    keyring.set_entry(r#"{"version":1}"#);

    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
}

/// ASV-FR-ZRWU: migration completes before the cache serves a request, so the
/// first read sees a migrated value, and the entry is read once to do it.
#[test]
fn migration_completes_before_the_cache_serves() {
    let (vault, keyring) = unmigrated_vault();
    keyring.set_legacy(GITHUB_LEGACY, "a", "L");
    vault.set_candidate_source(Box::new(|| vec![candidate(GITHUB_LEGACY, "a", A)]));

    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("L".into()));
    assert_eq!(keyring.legacy(GITHUB_LEGACY, "a"), None);
    let reads = keyring.count(&Call::Read);

    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("L".into()));
    assert_eq!(keyring.count(&Call::Read), reads);
    // One read to initialize, and one read-back to verify the migration write.
    assert_eq!(reads, 2);
}

/// ASV-FR-ZRWU: a candidate source that arrives after the cache was filled
/// still has its migration completed, and the cache takes the migrated value.
#[test]
fn a_late_candidate_source_still_migrates_into_the_cache() {
    let (vault, keyring) = unmigrated_vault();
    assert_eq!(vault.read_secret(&p(A)).unwrap(), None);
    keyring.set_legacy(GITHUB_LEGACY, "a", "L");
    vault.set_candidate_source(Box::new(|| vec![candidate(GITHUB_LEGACY, "a", A)]));

    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("L".into()));
    assert_eq!(keyring.object().get("github").is_some(), true);
}

/// ASV-FR-ZUGZ, ASV-FR-GLJD: a mutation as the first operation initializes the
/// cache with one read, and the cache takes the verified object.
#[test]
fn a_first_mutation_initializes_the_cache_and_fills_it_after_verification() {
    let (vault, keyring) = stocked();

    set(&vault, A, "A2").unwrap();

    // The read of initialization, the write, and the read-back.
    assert_eq!(keyring.calls(), vec![Call::Read, Call::Write, Call::Read]);
    let before = keyring.calls().len();
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A2".into()));
    assert_eq!(vault.read_secret(&p(B)).unwrap(), Some("B".into()));
    assert_eq!(keyring.calls().len(), before);
}

/// ASV-FR-GLJD, ASV-FR-DQHY, ASV-FR-10: a later mutation makes no read for its
/// base. The cache shows a set and a removal as soon as they are verified.
#[test]
fn a_later_mutation_writes_from_the_cache_and_updates_it() {
    let (vault, keyring) = stocked();
    vault.read_secret(&p(A)).unwrap();
    let reads = keyring.count(&Call::Read);

    set(&vault, &["github", "tokens", "n"], "N").unwrap();
    vault
        .apply_secret_mutations(&[Mutation::Remove { path: p(A) }])
        .unwrap();

    // Only the read-back of each mutation.
    assert_eq!(keyring.count(&Call::Read) - reads, 2);
    assert_eq!(vault.read_secret(&p(&["github", "tokens", "n"])).unwrap(), Some("N".into()));
    assert_eq!(vault.read_secret(&p(A)).unwrap(), None);
    assert_eq!(keyring.count(&Call::Read) - reads, 2);
    // The unknown namespace rides along in every write (ASV-FR-06).
    assert!(keyring.object().get("later").is_some());
}

/// ASV-FR-GLJD, ASV-FR-12: every failed mutation leaves the cache, and the
/// entry, as they were, and reports a failure.
#[test]
fn a_failed_mutation_leaves_the_cache_unchanged() {
    let (vault, keyring) = stocked();
    vault.read_secret(&p(A)).unwrap();
    let entry = keyring.entry();

    keyring.state().refuse_write = true;
    assert_eq!(set(&vault, A, "refused"), Err(VaultError::WriteFailed));
    keyring.state().refuse_write = false;

    keyring.state().mangle_next_writes = 1;
    assert_eq!(set(&vault, A, "mangled"), Err(VaultError::VerifyFailed));

    keyring.state().vanish_after_write = true;
    assert_eq!(set(&vault, A, "vanished"), Err(VaultError::VerifyFailed));
    keyring.state().vanish_after_write = false;
    keyring.set_entry(entry.as_deref().unwrap());

    // A path that cannot be created because a secret sits in its way.
    assert_eq!(
        set(&vault, &["github", "tokens", "a", "deeper"], "blocked"),
        Err(VaultError::WriteFailed)
    );

    let reads = keyring.count(&Call::Read);
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
    assert_eq!(vault.read_secret(&p(B)).unwrap(), Some("B".into()));
    assert_eq!(keyring.count(&Call::Read), reads);
    // The cache still holds the value that the entry held, so the next
    // successful mutation writes the whole of it.
    set(&vault, &["github", "tokens", "n"], "N").unwrap();
    assert_eq!(keyring.object().get("later").is_some(), true);
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
}

/// ASV-FR-GLJD, ASV-FR-12: a mutation that fails while the entry did not exist
/// leaves the cache as an empty object, and the next mutation succeeds.
#[test]
fn a_failed_first_mutation_on_an_absent_entry_leaves_an_empty_cache_object() {
    let keyring = FakeKeyring::default();
    let vault = Vault::new(Box::new(keyring.clone()));
    vault.mark_migrated();

    keyring.state().mangle_next_writes = 1;
    assert_eq!(set(&vault, A, "A"), Err(VaultError::VerifyFailed));

    assert_eq!(keyring.entry(), None);
    assert_eq!(vault.read_secret(&p(A)).unwrap(), None);
    set(&vault, A, "A").unwrap();
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
}

/// ASV-FR-GLJD: where the rollback of a failed verification also fails, the
/// entry has unknown content, and the cache is emptied rather than kept stale.
#[test]
fn a_failed_rollback_empties_the_cache() {
    let (vault, keyring) = stocked();
    vault.read_secret(&p(A)).unwrap();
    keyring.state().mangle_next_writes = 1;
    let from = keyring.state().writes + 1;
    keyring.state().refuse_write_from = Some(from);

    assert_eq!(set(&vault, A, "A2"), Err(VaultError::VerifyFailed));

    assert!(vault.cache_slot().is_none());
    keyring.state().refuse_write_from = None;
    keyring.set_entry(r#"{"version":1,"github":{"tokens":{"a":"E"}}}"#);
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("E".into()));
}

/// ASV-FR-FTFU: a failed initialization is not retried for the same operation.
/// It leaves the cache empty, and the next independent request starts a new
/// attempt.
#[test]
fn a_failed_initialization_is_retried_only_by_the_next_request() {
    let (vault, keyring) = stocked();
    keyring.state().refuse_read = true;

    assert_eq!(vault.read_secret(&p(A)), Err(VaultError::Unavailable));
    assert_eq!(keyring.count(&Call::Read), 1);
    assert!(vault.cache_slot().is_none());
    assert_eq!(vault.secret_presence(&[p(A)]), Err(VaultError::Unavailable));
    assert_eq!(keyring.count(&Call::Read), 2);

    keyring.state().refuse_read = false;
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
    assert_eq!(keyring.count(&Call::Read), 3);
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
    assert_eq!(keyring.count(&Call::Read), 3);
}

/// ASV-FR-FTFU: every typed failure of initialization reaches the requesting
/// operation unchanged and leaves the cache empty, and a mutation fails the
/// same way where it cannot recover.
#[test]
fn initialization_reports_the_existing_typed_errors() {
    let (vault, keyring) = stocked();
    keyring.set_entry(r#"{"version":99}"#);
    assert_eq!(vault.read_secret(&p(A)), Err(VaultError::UnsupportedVersion));
    assert_eq!(set(&vault, A, "x"), Err(VaultError::UnsupportedVersion));
    keyring.state().refuse_read = true;
    assert_eq!(set(&vault, A, "x"), Err(VaultError::Unavailable));
    assert!(vault.cache_slot().is_none());
}

/// ASV-FR-CIDB: requests that arrive while an attempt runs share its one read.
#[test]
fn concurrent_requests_share_one_initialization() {
    const REQUESTS: usize = 6;
    let (vault, keyring) = stocked();
    keyring.hold_reads();

    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..REQUESTS)
            .map(|i| {
                let vault = &vault;
                scope.spawn(move || match i % 3 {
                    0 => vault.read_secret(&p(A)).map(|secret| secret.is_some()),
                    1 => vault.secret_presence(&[p(B)]).map(|answers| answers.values().all(|v| *v)),
                    _ => vault.read_secret(&p(C)).map(|secret| secret.is_some()),
                })
            })
            .collect();
        wait_for_waiters(&vault, &keyring, REQUESTS - 1);
        keyring.release_reads();
        for handle in handles {
            assert_eq!(handle.join().unwrap(), Ok(true));
        }
    });

    assert_eq!(keyring.count(&Call::Read), 1);
}

/// ASV-FR-CIDB, ASV-FR-FTFU: requests that wait for an attempt that fails take
/// its failure and start no read of their own, and the next request retries.
#[test]
fn waiting_requests_take_the_failure_of_the_attempt() {
    const REQUESTS: usize = 5;
    let (vault, keyring) = stocked();
    keyring.state().refuse_read = true;
    keyring.hold_reads();

    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..REQUESTS)
            .map(|_| {
                let vault = &vault;
                scope.spawn(move || vault.read_secret(&p(A)))
            })
            .collect();
        wait_for_waiters(&vault, &keyring, REQUESTS - 1);
        keyring.release_reads();
        for handle in handles {
            assert_eq!(handle.join().unwrap(), Err(VaultError::Unavailable));
        }
    });

    assert_eq!(keyring.count(&Call::Read), 1);
    assert!(vault.cache_slot().is_none());

    keyring.state().refuse_read = false;
    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
    assert_eq!(keyring.count(&Call::Read), 2);
}

/// ASV-FR-DIEF, ASV-FR-17: an entry that would not decode fails every read and
/// leaves the cache empty. A mutation recovers it, and the cache then holds the
/// verified object.
#[test]
fn a_mutation_recovers_a_malformed_entry_and_fills_the_cache() {
    let (vault, keyring) = stocked();
    keyring.set_entry("garbage");

    assert_eq!(vault.read_secret(&p(A)), Err(VaultError::Malformed));
    assert_eq!(vault.secret_presence(&[p(A)]), Err(VaultError::Malformed));
    assert!(vault.cache_slot().is_none());

    set(&vault, A, "A").unwrap();
    let reads = keyring.count(&Call::Read);

    assert_eq!(vault.read_secret(&p(A)).unwrap(), Some("A".into()));
    assert_eq!(keyring.count(&Call::Read), reads);
    assert_eq!(keyring.object().get("quarantine").and_then(Value::as_str), Some("garbage"));
}

/// ASV-FR-DIEF, ASV-FR-12: a recovery whose write fails leaves the cache empty
/// and the undecodable value where it was.
#[test]
fn a_failed_recovery_leaves_the_cache_empty() {
    let (vault, keyring) = stocked();
    keyring.set_entry("garbage");
    keyring.state().refuse_write = true;

    assert_eq!(set(&vault, A, "A"), Err(VaultError::WriteFailed));

    assert!(vault.cache_slot().is_none());
    assert_eq!(keyring.entry().as_deref(), Some("garbage"));
    assert_eq!(vault.read_secret(&p(A)), Err(VaultError::Malformed));
}

/// ASV-FR-SUXZ, ASV-FR-28: the `Debug` rendering of the cache shows nothing it
/// holds, and the cache has no serialization.
#[test]
fn the_cache_renders_nothing_it_holds() {
    let (vault, _keyring) = stocked();
    vault.read_secret(&p(A)).unwrap();
    let slot = vault.cache_slot();

    let rendered = format!("{:?}", slot.as_ref().unwrap());

    for needle in ["A", "github", "tokens", "later", "version"] {
        assert!(!rendered.replace("Cached", "").replace("redacted", "").contains(needle));
    }
    let _: &Cached = slot.as_ref().unwrap();
}
