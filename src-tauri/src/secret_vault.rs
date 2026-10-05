//! Application secret vault — `specifications/core/ASV-application-secret-vault.md`.
//!
//! The one place in the application that reads or writes a secret in the
//! operating-system keyring. Every application-managed secret — each GitHub
//! token, each AI API provider key, each agent credential, and every secret
//! type added later — lives inside **one** keyring entry that holds one
//! extensible JSON object (ASV-FR-01, ASV-FR-02).
//!
//! Three properties carry the module:
//!
//! - **One entry, one access.** An operation makes at most one keyring read,
//!   one write, and one read-back however many secrets are stored, so the
//!   author answers one authentication prompt for an operation instead of one
//!   per secret (ASV-FR-10, ASV-FR-30).
//! - **A whole-object write, verified.** A mutation reads the entry, edits a
//!   decoded copy, writes the complete object, and reads it back to compare;
//!   anything that fails leaves the entry exactly as it was (ASV-FR-11,
//!   ASV-FR-12).
//! - **Nothing leaks.** No secret, no path, and no id reaches a log line, an
//!   error payload, or a `Debug` rendering (ASV-FR-28). The types that carry
//!   secret material hand-roll `Debug` rather than deriving it, because a
//!   derived one is a single stray `{:?}` away from a disclosure.
//!
//! The object is held as a `serde_json` map rather than as a struct of named
//! namespaces. That is what makes ASV-FR-05 and ASV-FR-06 true by
//! construction: a path of any depth is accepted without a schema change, and
//! a namespace this build does not know is written back byte for byte instead
//! of being dropped by a deserialiser that never heard of it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Map, Value};

use crate::log_fields;
use crate::logging::{log_debug, log_error, log_info, log_warn, Domain, Fields, BUFFER};

// ---------------------------------------------------------------------------
// Keyring identity and schema constants
// ---------------------------------------------------------------------------

/// ASV-FR-01: the service name of the single consolidated entry. Stable — a
/// change to it orphans every stored secret.
pub const VAULT_SERVICE: &str = "com.synthesis.secrets";
/// ASV-FR-01: the account name of the single consolidated entry.
pub const VAULT_ACCOUNT: &str = "application-secrets";
/// ASV-FR-02: the schema version this build writes and is willing to read.
pub const SCHEMA_VERSION: u64 = 1;

/// ASV-FR-02: the root field carrying the schema version.
const FIELD_VERSION: &str = "version";
/// ASV-FR-18: the reserved root field that carries a prior value of the entry
/// which would not decode. No `SecretPath` addresses it.
const FIELD_QUARANTINE: &str = "quarantine";

// ---------------------------------------------------------------------------
// Paths, mutations, candidates
// ---------------------------------------------------------------------------

/// ASV-FR-04: the ordered list of object keys from the root of `AppSecrets` to
/// the leaf that holds the secret string. This module gives a path no meaning.
pub type SecretPath = Vec<String>;

/// Build a [`SecretPath`] from string-ish segments.
pub fn path(segments: &[&str]) -> SecretPath {
    segments.iter().map(|s| (*s).to_string()).collect()
}

/// One edit of the object. A request carries a list of these and they reach the
/// keyring in one write (ASV-FR-13).
///
/// `Debug` is hand-rolled and discloses neither the secret nor the path: a
/// mutation routinely sits inside an error path, and ASV-FR-28 forbids either
/// reaching a rendering.
pub enum Mutation {
    Set { path: SecretPath, secret: String },
    Remove { path: SecretPath },
}

impl std::fmt::Debug for Mutation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mutation::Set { .. } => f.write_str("Set { path: <redacted>, secret: <redacted> }"),
            Mutation::Remove { .. } => f.write_str("Remove { path: <redacted> }"),
        }
    }
}

/// ASV-FR-20: one secret of the earlier one-entry-per-secret layout, named by
/// the owning module because a keyring enumerates no entry.
///
/// `Debug` discloses neither the account name (an id) nor the path, for the
/// same reason [`Mutation`]'s does.
pub struct Candidate {
    pub legacy_service: String,
    pub legacy_account: String,
    pub path: SecretPath,
}

impl std::fmt::Debug for Candidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Candidate { legacy_service: <redacted>, legacy_account: <redacted>, path: <redacted> }")
    }
}

/// What one migration did. Counts only — no path, no id, no secret
/// (contract surface).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MigrationOutcome {
    /// Legacy secrets written into the consolidated object.
    pub adopted: usize,
    /// Legacy entries removed after the verified write.
    pub deleted: usize,
    /// Legacy entries the keyring refused to delete.
    pub undeleted: usize,
    /// Candidates whose legacy entry the keyring refused to read.
    pub unreadable: usize,
    /// Candidates that have no legacy entry at all.
    pub absent: usize,
}

/// ASV-FR-31: the typed failures. Every owning module renders all five as its
/// existing `keychain_unavailable` error, so the vocabulary the frontend sees
/// does not depend on how secrets are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultError {
    /// The keyring refused a read. Never reported as "the path holds nothing"
    /// (ASV-FR-15).
    Unavailable,
    /// The keyring refused the write (ASV-FR-12).
    WriteFailed,
    /// The read-back did not match what was written, or could not be made
    /// (ASV-FR-11).
    VerifyFailed,
    /// The stored value is not a well-formed `AppSecrets` object (ASV-FR-17).
    Malformed,
    /// The stored object's version is higher than this build knows
    /// (ASV-FR-19).
    UnsupportedVersion,
}

impl VaultError {
    /// The stable code this failure is known by. Safe to log: it names the
    /// outcome and nothing about what the entry holds.
    pub fn code(self) -> &'static str {
        match self {
            VaultError::Unavailable => "vault_unavailable",
            VaultError::WriteFailed => "vault_write_failed",
            VaultError::VerifyFailed => "vault_verify_failed",
            VaultError::Malformed => "vault_malformed",
            VaultError::UnsupportedVersion => "vault_unsupported_version",
        }
    }
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

// ---------------------------------------------------------------------------
// The keyring seam
// ---------------------------------------------------------------------------

/// The keyring, narrowed to what this module needs: the one consolidated entry,
/// plus read and delete of a legacy entry during migration.
///
/// Behind a trait because CI runners have no keychain, and a test that needed
/// one would either fail there or be deleted until it passed.
pub trait VaultBackend: Send + Sync {
    /// `Ok(None)` means the entry does not exist — the state of a machine that
    /// has stored no secret (ASV-FR-16), not a failure.
    fn read(&self) -> Result<Option<String>, String>;
    fn write(&self, value: &str) -> Result<(), String>;
    /// Used only to undo a write whose verification failed on an entry that did
    /// not exist before it (ASV-FR-12).
    fn delete(&self) -> Result<(), String>;
    /// `Ok(None)` means the candidate has no legacy entry at all (ASV-FR-23).
    fn read_legacy(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    fn delete_legacy(&self, service: &str, account: &str) -> Result<(), String>;
}

/// Production backend: the platform-native keyring, one entry.
pub struct KeyringVaultBackend;

impl KeyringVaultBackend {
    fn entry(service: &str, account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(service, account)
            .map_err(|e| format!("credential store unavailable: {e}"))
    }

    fn read_entry(service: &str, account: &str) -> Result<Option<String>, String> {
        match Self::entry(service, account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("could not read credential: {e}")),
        }
    }

    fn delete_entry(service: &str, account: &str) -> Result<(), String> {
        match Self::entry(service, account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("could not delete credential: {e}")),
        }
    }
}

impl VaultBackend for KeyringVaultBackend {
    fn read(&self) -> Result<Option<String>, String> {
        Self::read_entry(VAULT_SERVICE, VAULT_ACCOUNT)
    }

    fn write(&self, value: &str) -> Result<(), String> {
        Self::entry(VAULT_SERVICE, VAULT_ACCOUNT)?
            .set_password(value)
            .map_err(|e| format!("could not store credential: {e}"))
    }

    fn delete(&self) -> Result<(), String> {
        Self::delete_entry(VAULT_SERVICE, VAULT_ACCOUNT)
    }

    fn read_legacy(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        Self::read_entry(service, account)
    }

    fn delete_legacy(&self, service: &str, account: &str) -> Result<(), String> {
        Self::delete_entry(service, account)
    }
}

// ---------------------------------------------------------------------------
// The vault
// ---------------------------------------------------------------------------

/// What one candidate collection returns. Supplied by the owning modules,
/// because a keyring enumerates no entry (ASV-FR-20).
type CandidateSource = Box<dyn Fn() -> Vec<Candidate> + Send + Sync>;

/// The state of the entry as one operation found it.
enum Loaded {
    /// A decoded object of a version this build understands.
    Object(Map<String, Value>),
    /// The entry does not exist (ASV-FR-16).
    Absent,
    /// The stored value would not decode. Carries it verbatim so a mutation can
    /// quarantine it (ASV-FR-17).
    Malformed(String),
}

/// The one read an operation makes, kept whole.
///
/// `raw` is what the entry held, retained for the length of the operation so a
/// write whose verification fails can put it back without a second read — the
/// non-functional budget is one read, one write, and one read-back.
struct Snapshot {
    raw: Option<String>,
    state: Loaded,
}

/// The one keyring entry, the object inside it, and the process-wide lock that
/// guards it.
pub struct Vault {
    backend: Box<dyn VaultBackend>,
    /// ASV-FR-09: one process-wide lock. Every read, mutation, and migration
    /// holds it for its whole sequence, so two sequences never touch the entry
    /// at the same time and no update is lost (ASV-FR-14).
    lock: Mutex<()>,
    /// ASV-FR-20: whether migration has already run in this process. Read and
    /// written only while `lock` is held.
    migrated: AtomicBool,
    candidates: Mutex<Option<CandidateSource>>,
    /// Where a log record goes. Absent until the application installs one, so
    /// the module is usable in a unit test without a Tauri runtime.
    sink: Mutex<Option<tauri::AppHandle>>,
    /// Every record this module emitted, kept only in a test build.
    ///
    /// ASV-FR-28 — no log line may carry a secret, a path, an id, or a
    /// quarantined value — is the highest-stakes requirement here and the one a
    /// passing suite is least likely to catch: the production sink is absent in
    /// a unit test, so without this every `log_fields!` call site in the module
    /// would execute zero times under test and a field added tomorrow could
    /// leak a path without turning anything red.
    #[cfg(test)]
    emitted: Mutex<Vec<(String, Fields)>>,
}

impl Vault {
    pub fn new(backend: Box<dyn VaultBackend>) -> Self {
        Self {
            backend,
            lock: Mutex::new(()),
            migrated: AtomicBool::new(false),
            candidates: Mutex::new(None),
            sink: Mutex::new(None),
            #[cfg(test)]
            emitted: Mutex::new(Vec::new()),
        }
    }

    /// Every record this module has emitted so far, as `(message, fields)`.
    #[cfg(test)]
    pub(crate) fn emitted(&self) -> Vec<(String, Fields)> {
        self.emitted.lock().unwrap().clone()
    }

    /// Where migration gets its candidates. Installed at startup by the
    /// application, which is the only place that can reach all three owning
    /// registries at once.
    pub fn set_candidate_source(&self, source: CandidateSource) {
        if let Ok(mut slot) = self.candidates.lock() {
            *slot = Some(source);
        }
    }

    /// Point the module's log records at the running application.
    pub fn install_log_sink(&self, handle: tauri::AppHandle) {
        if let Ok(mut slot) = self.sink.lock() {
            *slot = Some(handle);
        }
    }

    /// Mark migration as already performed. Used by tests that drive migration
    /// explicitly and by callers that have no legacy layout to adopt.
    #[cfg(test)]
    pub(crate) fn mark_migrated(&self) {
        self.migrated.store(true, Ordering::SeqCst);
    }

    // -- Logging ---------------------------------------------------------
    //
    // ASV-FR-28: a log line about a vault operation names the operation and its
    // outcome, and never a path, an id, a quarantined value, or a secret. Every
    // emit site below is held to that by construction — the only field values
    // these helpers are ever handed are counts, booleans, and error codes.

    fn emit(&self, level: LogLevel, message: &str, fields: Fields) {
        #[cfg(test)]
        self.emitted
            .lock()
            .unwrap()
            .push((message.to_string(), fields.clone()));
        let handle = match self.sink.lock() {
            Ok(slot) => slot.clone(),
            Err(_) => None,
        };
        let Some(handle) = handle else { return };
        match level {
            LogLevel::Debug => log_debug(&handle, &BUFFER, &[Domain::Backend], message, fields),
            LogLevel::Info => log_info(&handle, &BUFFER, &[Domain::Backend], message, fields),
            LogLevel::Warn => log_warn(&handle, &BUFFER, &[Domain::Backend], message, fields),
            LogLevel::Error => log_error(&handle, &BUFFER, &[Domain::Backend], message, fields),
        }
    }

    fn failed(&self, operation: &'static str, error: VaultError) -> VaultError {
        self.emit(
            LogLevel::Error,
            "secret vault operation failed",
            log_fields! { "operation" => operation, "error" => error.code() },
        );
        error
    }

    // -- Reads -----------------------------------------------------------

    /// ASV-FR-04 / ASV-FR-07: the secret at `path`, or `None` where the
    /// namespace, an intermediate object, or the leaf is absent — which is not
    /// an error.
    pub fn read_secret(&self, path: &[String]) -> Result<Option<String>, VaultError> {
        // Gathered before the lock is taken: the source reaches into the
        // owning modules' registries, and the non-functional requirement is
        // that the lock is held for the keyring access alone.
        let pending = self.pending_candidates();
        let _guard = self.acquire();
        self.ensure_migrated_locked(pending);
        let object = match self.load_locked("read_secret")?.state {
            Loaded::Object(object) => object,
            Loaded::Absent => return Ok(None),
            Loaded::Malformed(_) => return Err(self.failed("read_secret", VaultError::Malformed)),
        };
        Ok(lookup(&object, path))
    }

    /// ASV-FR-30: presence for every supplied path from **one** read of the
    /// entry, as booleans rather than values. A listing that describes many
    /// records therefore makes one keyring access.
    pub fn secret_presence(
        &self,
        paths: &[SecretPath],
    ) -> Result<HashMap<SecretPath, bool>, VaultError> {
        // Gathered before the lock is taken: the source reaches into the
        // owning modules' registries, and the non-functional requirement is
        // that the lock is held for the keyring access alone.
        let pending = self.pending_candidates();
        let _guard = self.acquire();
        self.ensure_migrated_locked(pending);
        let object = match self.load_locked("secret_presence")?.state {
            Loaded::Object(object) => object,
            Loaded::Absent => Map::new(),
            Loaded::Malformed(_) => {
                return Err(self.failed("secret_presence", VaultError::Malformed))
            }
        };
        let mut answers = HashMap::with_capacity(paths.len());
        for path in paths {
            answers.insert(path.clone(), lookup(&object, path).is_some());
        }
        Ok(answers)
    }

    // -- Mutation --------------------------------------------------------

    /// ASV-FR-10 / ASV-FR-13: apply every mutation of the request to a decoded
    /// copy of the whole object and write that object back as one keyring
    /// value, verified. Nothing is applied where anything fails (ASV-FR-12).
    pub fn apply_secret_mutations(&self, mutations: &[Mutation]) -> Result<(), VaultError> {
        // Gathered before the lock is taken: the source reaches into the
        // owning modules' registries, and the non-functional requirement is
        // that the lock is held for the keyring access alone.
        let pending = self.pending_candidates();
        let _guard = self.acquire();
        self.ensure_migrated_locked(pending);
        let snapshot = self.load_locked("apply_secret_mutations")?;
        let (mut object, quarantined) = match snapshot.state {
            Loaded::Object(object) => (object, false),
            Loaded::Absent => (empty_object(), false),
            // ASV-FR-17: recovery destroys nothing. The undecodable value rides
            // along in the same single verified write as the new secrets.
            Loaded::Malformed(raw) => {
                let mut object = empty_object();
                object.insert(FIELD_QUARANTINE.to_string(), Value::String(raw));
                (object, true)
            }
        };
        for mutation in mutations {
            match mutation {
                Mutation::Set { path, secret } => {
                    if !insert_at(&mut object, path, secret) {
                        // A path whose parent is a string rather than an object
                        // cannot be created without destroying the secret that
                        // sits there. Refuse rather than overwrite.
                        return Err(self.failed("apply_secret_mutations", VaultError::WriteFailed));
                    }
                }
                Mutation::Remove { path } => remove_at(&mut object, path),
            }
        }
        self.write_verified_locked("apply_secret_mutations", &object, snapshot.raw.as_deref())?;
        self.emit(
            LogLevel::Debug,
            "secret vault mutated",
            log_fields! { "mutations" => mutations.len(), "quarantined" => quarantined },
        );
        Ok(())
    }

    // -- Migration -------------------------------------------------------

    /// ASV-FR-20: move the secrets of the earlier one-entry-per-secret layout
    /// into the consolidated entry.
    pub fn migrate_legacy_secrets(
        &self,
        candidates: &[Candidate],
    ) -> Result<MigrationOutcome, VaultError> {
        let _guard = self.acquire();
        let outcome = self.migrate_locked(candidates, &mut false)?;
        self.migrated.store(true, Ordering::SeqCst);
        Ok(outcome)
    }

    // -- Internals (the lock is held throughout) --------------------------

    /// Take the process-wide lock, recovering a poisoned one.
    ///
    /// A poisoned lock means a previous holder panicked; the entry itself is
    /// untouched by that, and refusing every subsequent secret operation for
    /// the life of the process would be a worse outcome than carrying on.
    fn acquire(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The candidates migration would use, or `None` where it has already run
    /// or nothing has told this vault what the legacy layout held.
    ///
    /// Deliberately **outside** the lock. The source is supplied by the
    /// application and reaches into three registries; running it while the
    /// process-wide lock is held would put work this module does not control
    /// between every other secret operation and the keyring, against the
    /// requirement that the lock covers the keyring access alone. Two callers
    /// may gather concurrently and both hand their list in — the second finds
    /// migration already done and drops it, which costs a registry read and
    /// nothing else.
    fn pending_candidates(&self) -> Option<Vec<Candidate>> {
        if self.migrated.load(Ordering::SeqCst) {
            return None;
        }
        let slot = self.candidates.lock().ok()?;
        let source = slot.as_ref()?;
        Some(source())
    }

    /// ASV-FR-20: migration runs once per process, before the first read or
    /// mutation the process performs.
    ///
    /// Whether a failed migration is retried turns on **how far it got**, which
    /// is what keeps the cost of a broken machine bounded. A migration that
    /// failed before it read a single legacy entry — an unreadable, malformed,
    /// or newer-than-this-build consolidated entry — leaves the flag unset,
    /// because retrying it costs one read of the one entry and the state it
    /// failed on is one a mutation can clear. A migration that got as far as
    /// reading legacy entries marks itself done whatever happened next: the
    /// alternative is re-reading every legacy entry on every subsequent secret
    /// operation for the life of the process, which on a platform that prompts
    /// per keyring access means one authentication prompt per stored secret per
    /// operation. ASV-FR-26 covers the rest — the next launch migrates again.
    fn ensure_migrated_locked(&self, candidates: Option<Vec<Candidate>>) {
        // `None` is either "already done" or "nothing has told this vault what
        // the legacy layout held". The second is not marked done: the source is
        // installed at startup, and a caller that ran first must not cost the
        // process its migration.
        let Some(candidates) = candidates else { return };
        if self.migrated.load(Ordering::SeqCst) {
            return;
        }
        let mut read_a_legacy_entry = false;
        match self.migrate_locked(&candidates, &mut read_a_legacy_entry) {
            Ok(_) => self.migrated.store(true, Ordering::SeqCst),
            // Already logged, and the caller's own read reports the failure.
            //
            // `UnsupportedVersion` is marked done alongside a migration that
            // got as far as the legacy entries, for the opposite reason: no
            // mutation of this build can clear it (ASV-FR-19 refuses every one
            // of them), so retrying would cost a second keyring read — and a
            // second authentication prompt where the platform asks — on every
            // secret operation for the life of the process, forever. The build
            // that wrote the entry owns it.
            Err(VaultError::UnsupportedVersion) => self.migrated.store(true, Ordering::SeqCst),
            Err(_) if read_a_legacy_entry => self.migrated.store(true, Ordering::SeqCst),
            // `Unavailable` and `Malformed` are both states a later moment can
            // clear — the keychain unlocks, or a mutation quarantines the value
            // — and a retry costs one read of the one entry.
            Err(_) => {}
        }
    }

    /// `read_a_legacy_entry` is set as soon as this reaches the legacy entries,
    /// which is what [`Self::ensure_migrated_locked`] decides retry policy on.
    fn migrate_locked(
        &self,
        candidates: &[Candidate],
        read_a_legacy_entry: &mut bool,
    ) -> Result<MigrationOutcome, VaultError> {
        let mut outcome = MigrationOutcome::default();
        if candidates.is_empty() {
            return Ok(outcome);
        }

        let snapshot = self.load_locked("migrate_legacy_secrets")?;
        let mut object = match snapshot.state {
            Loaded::Object(object) => object,
            Loaded::Absent => empty_object(),
            // ASV-FR-17: an undecodable value is quarantined by **a mutation**,
            // and until one asks, this module writes nothing and deletes
            // nothing. Migration is not that mutation: recovering here would
            // make a plain `read_secret` write to the keyring, delete legacy
            // entries, and then answer `Ok` where ASV-FR-17 requires
            // `vault_malformed`. So migration stands down instead, and the flag
            // stays unset — the first mutation quarantines the value (ASV-FR-17)
            // and the operation after it finds a decodable entry and migrates
            // normally. Nothing is lost either way: the legacy entries are still
            // there, untouched, because nothing was deleted.
            Loaded::Malformed(_) => {
                return Err(self.failed("migrate_legacy_secrets", VaultError::Malformed))
            }
        };
        *read_a_legacy_entry = true;

        // Every candidate whose secret the consolidated object holds once the
        // write below has been verified. ASV-FR-25: no legacy entry is deleted
        // before that, whatever the write had to change.
        let mut deletable: Vec<&Candidate> = Vec::new();
        for candidate in candidates {
            match self
                .backend
                .read_legacy(&candidate.legacy_service, &candidate.legacy_account)
            {
                // ASV-FR-22: skipped rather than fatal, and **not deleted** —
                // deleting a secret this module could not read would destroy
                // the only copy of it.
                Err(_) => outcome.unreadable += 1,
                // ASV-FR-23: the normal state once migration has completed, and
                // of every candidate on a machine that never used the earlier
                // layout.
                Ok(None) => outcome.absent += 1,
                // ASV-FR-21: the consolidated object wins where both hold a
                // value for one path — the legacy value is adopted nowhere, and
                // its entry is still deleted after the same verified write
                // (ASV-FR-25).
                Ok(Some(_)) if lookup(&object, &candidate.path).is_some() => {
                    deletable.push(candidate)
                }
                Ok(Some(secret)) => {
                    if insert_at(&mut object, &candidate.path, &secret) {
                        outcome.adopted += 1;
                        deletable.push(candidate);
                    } else {
                        // A level of the path is occupied by something that is
                        // not an object. Nothing was adopted, so the legacy
                        // entry must stay: it is the only copy of the secret.
                        self.emit(
                            LogLevel::Warn,
                            "secret vault could not adopt a legacy secret",
                            log_fields! { "reason" => "path_blocked" },
                        );
                    }
                }
            }
        }

        // ASV-FR-24: one whole-object write and no more, performed whenever
        // there is a secret to adopt or a legacy entry to delete — including
        // where nothing was adopted, because a deletion may follow only a
        // verified write.
        if deletable.is_empty() {
            self.emit(
                LogLevel::Debug,
                "secret vault migration found nothing to adopt",
                log_fields! { "candidates" => candidates.len(), "absent" => outcome.absent,
                "unreadable" => outcome.unreadable },
            );
            return Ok(outcome);
        }

        self.write_verified_locked("migrate_legacy_secrets", &object, snapshot.raw.as_deref())?;

        for candidate in deletable {
            match self
                .backend
                .delete_legacy(&candidate.legacy_service, &candidate.legacy_account)
            {
                Ok(()) => outcome.deleted += 1,
                // ASV-FR-26: partial migration is safe and repeatable. The
                // entry stays where it is and the next migration deletes it.
                Err(_) => outcome.undeleted += 1,
            }
        }

        self.emit(
            LogLevel::Info,
            "secret vault migrated legacy secrets",
            log_fields! {
                "adopted" => outcome.adopted,
                "deleted" => outcome.deleted,
                "undeleted" => outcome.undeleted,
                "unreadable" => outcome.unreadable,
                "absent" => outcome.absent,
            },
        );
        Ok(outcome)
    }

    /// Read the entry and decide which of the three states it is in.
    fn load_locked(&self, operation: &'static str) -> Result<Snapshot, VaultError> {
        // ASV-FR-15: a keyring that refuses a read is never reported as a path
        // that holds nothing.
        let raw = match self.backend.read() {
            Ok(raw) => raw,
            Err(_) => return Err(self.failed(operation, VaultError::Unavailable)),
        };
        let Some(raw) = raw else {
            return Ok(Snapshot {
                raw: None,
                state: Loaded::Absent,
            });
        };
        let state = match decode(&raw) {
            Decoded::Object(object) => Loaded::Object(object),
            // ASV-FR-19: an older build never overwrites an object a newer one
            // wrote, and never quarantines one either.
            Decoded::Unsupported => {
                return Err(self.failed(operation, VaultError::UnsupportedVersion))
            }
            Decoded::Malformed => Loaded::Malformed(raw.clone()),
        };
        Ok(Snapshot {
            raw: Some(raw),
            state,
        })
    }

    /// ASV-FR-10 / ASV-FR-11 / ASV-FR-12: write the complete object as one
    /// keyring value, read it back inside the same held lock, and compare. A
    /// write whose verification fails is undone.
    fn write_verified_locked(
        &self,
        operation: &'static str,
        object: &Map<String, Value>,
        previous: Option<&str>,
    ) -> Result<(), VaultError> {
        let serialized = match serde_json::to_string(&Value::Object(object.clone())) {
            Ok(serialized) => serialized,
            // A map built from JSON values cannot fail to serialise; treated as
            // a write failure rather than a panic all the same.
            Err(_) => return Err(self.failed(operation, VaultError::WriteFailed)),
        };

        if self.backend.write(&serialized).is_err() {
            return Err(self.failed(operation, VaultError::WriteFailed));
        }

        let verified = match self.backend.read() {
            Ok(Some(read_back)) => matches!(decode(&read_back), Decoded::Object(o) if &o == object),
            Ok(None) | Err(_) => false,
        };
        if verified {
            return Ok(());
        }

        // Best effort: the entry must hold what it held before the request.
        // A rollback the keyring also refuses leaves the entry where the failed
        // write put it, which is the one outcome nothing here can improve on —
        // it is still reported as a failure.
        let restored = match previous {
            Some(previous) => self.backend.write(previous).is_ok(),
            None => self.backend.delete().is_ok(),
        };
        self.emit(
            LogLevel::Warn,
            "secret vault write could not be verified",
            log_fields! { "operation" => operation, "rolled_back" => restored },
        );
        Err(self.failed(operation, VaultError::VerifyFailed))
    }
}

enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

// ---------------------------------------------------------------------------
// The object
// ---------------------------------------------------------------------------

enum Decoded {
    Object(Map<String, Value>),
    Unsupported,
    Malformed,
}

/// An `AppSecrets` object of the current version holding nothing.
fn empty_object() -> Map<String, Value> {
    let mut object = Map::new();
    object.insert(FIELD_VERSION.to_string(), Value::from(SCHEMA_VERSION));
    object
}

/// Decide what a stored value is: a decodable object of a version this build
/// knows, an object from a newer build, or something that is neither.
fn decode(raw: &str) -> Decoded {
    let value: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return Decoded::Malformed,
    };
    let Value::Object(object) = value else {
        return Decoded::Malformed;
    };
    match object.get(FIELD_VERSION).and_then(Value::as_u64) {
        Some(version) if version == SCHEMA_VERSION => Decoded::Object(object),
        Some(version) if version > SCHEMA_VERSION => Decoded::Unsupported,
        // No version at all, a version of zero, or a version that is not a
        // number: not a well-formed `AppSecrets` object (ASV-FR-17).
        _ => Decoded::Malformed,
    }
}

/// ASV-FR-07: the secret at `path`, or `None` where any level of it is absent
/// or is not the shape the path implies.
///
/// ASV-FR-18: no path resolves the reserved `quarantine` field, whatever it is
/// asked for.
fn lookup(object: &Map<String, Value>, path: &[String]) -> Option<String> {
    if !addressable(path) {
        return None;
    }
    let (leaf, parents) = path.split_last()?;
    let mut current = object;
    for key in parents {
        current = current.get(key)?.as_object()?;
    }
    current.get(leaf)?.as_str().map(str::to_string)
}

/// Write `secret` at `path`, creating every absent object between the root and
/// the leaf (ASV-FR-05, ASV-FR-07).
///
/// Returns `false` where a level of the path is occupied by a value that is not
/// an object — replacing it would destroy whatever sits there.
fn insert_at(object: &mut Map<String, Value>, path: &[String], secret: &str) -> bool {
    if !addressable(path) {
        return false;
    }
    let Some((leaf, parents)) = path.split_last() else {
        return false;
    };
    let mut current = object;
    for key in parents {
        let entry = current
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        match entry.as_object_mut() {
            Some(next) => current = next,
            None => return false,
        }
    }
    match current.get(leaf) {
        // The leaf exists and is not a secret string; replacing it would drop a
        // whole namespace a later build owns (ASV-FR-06).
        Some(existing) if !existing.is_string() => false,
        _ => {
            current.insert(leaf.clone(), Value::String(secret.to_string()));
            true
        }
    }
}

/// Remove the leaf at `path`. Removing a leaf that is not there succeeds, which
/// is what makes a `Remove` idempotent.
///
/// Intermediate objects left empty are kept rather than pruned: an empty
/// namespace and an absent one read identically (ASV-FR-07), and pruning would
/// risk removing an object a build that does not know the namespace owns.
fn remove_at(object: &mut Map<String, Value>, path: &[String]) {
    if !addressable(path) {
        return;
    }
    let Some((leaf, parents)) = path.split_last() else {
        return;
    };
    let mut current = object;
    for key in parents {
        match current.get_mut(key).and_then(Value::as_object_mut) {
            Some(next) => current = next,
            None => return,
        }
    }
    current.remove(leaf);
}

/// Whether a path names something this module will read or write.
///
/// ASV-FR-18: `quarantine` is a reserved root field rather than a namespace, so
/// no `SecretPath` addresses it. The version field is reserved the same way —
/// a path that overwrote it would make the entry undecodable.
fn addressable(path: &[String]) -> bool {
    match path.first() {
        Some(root) => root != FIELD_QUARANTINE && root != FIELD_VERSION,
        None => false,
    }
}

// ---------------------------------------------------------------------------
// The process-wide instance
// ---------------------------------------------------------------------------

static GLOBAL: OnceLock<Arc<Vault>> = OnceLock::new();

/// ASV-FR-09: the one vault this process uses, and therefore the one lock.
///
/// A `OnceLock` rather than Tauri-managed state because the modules that own
/// secrets build their credential stores in `Default::default()`, before any
/// app handle exists — and because "process-wide" is the requirement, not "one
/// per managed state".
pub fn global() -> Arc<Vault> {
    GLOBAL
        .get_or_init(|| Arc::new(Vault::new(Box::new(KeyringVaultBackend))))
        .clone()
}

// ---------------------------------------------------------------------------
// The adapter the owning modules hold
// ---------------------------------------------------------------------------

/// One namespace of the vault, presented as the `SecretStore` the owning
/// modules already talk to.
///
/// The modules address their secrets by id; this turns an id into the
/// `SecretPath` the vault reserves for that namespace, and turns every typed
/// vault failure into the single `SecretUnavailable` those modules render as
/// `keychain_unavailable` (ASV-FR-31).
pub struct VaultSecrets {
    vault: Arc<Vault>,
    namespace: &'static [&'static str],
}

impl VaultSecrets {
    pub fn new(vault: Arc<Vault>, namespace: &'static [&'static str]) -> Self {
        Self { vault, namespace }
    }

    /// The vault path of `id` within this namespace.
    pub fn path_of(&self, id: &str) -> SecretPath {
        let mut path: SecretPath = self.namespace.iter().map(|s| (*s).to_string()).collect();
        path.push(id.to_string());
        path
    }

    fn set_secret(&self, id: &str, secret: &str) -> Result<(), VaultError> {
        self.vault.apply_secret_mutations(&[Mutation::Set {
            path: self.path_of(id),
            secret: secret.to_string(),
        }])
    }

    fn get_secret(&self, id: &str) -> Result<Option<String>, VaultError> {
        self.vault.read_secret(&self.path_of(id))
    }

    fn delete_secret(&self, id: &str) -> Result<(), VaultError> {
        self.vault.apply_secret_mutations(&[Mutation::Remove {
            path: self.path_of(id),
        }])
    }

    fn presence_of(&self, ids: &[&str]) -> Result<HashMap<String, bool>, VaultError> {
        let paths: Vec<SecretPath> = ids.iter().map(|id| self.path_of(id)).collect();
        let answers = self.vault.secret_presence(&paths)?;
        Ok(ids
            .iter()
            .zip(paths.iter())
            .map(|(id, path)| {
                (
                    (*id).to_string(),
                    answers.get(path).copied().unwrap_or(false),
                )
            })
            .collect())
    }
}

/// Every typed vault failure becomes the one error the owning modules render as
/// `keychain_unavailable` (ASV-FR-31). The code travels in the message so a
/// developer reading a `Debug` can still tell them apart; it names an outcome
/// and never a path, an id, or a secret (ASV-FR-28).
fn unavailable_github(e: VaultError) -> crate::github_tokens::SecretUnavailable {
    crate::github_tokens::SecretUnavailable(e.code().to_string())
}

fn unavailable_ai(e: VaultError) -> crate::ai_shared::SecretUnavailable {
    crate::ai_shared::SecretUnavailable(e.code().to_string())
}

impl crate::github_tokens::SecretStore for VaultSecrets {
    fn set(&self, id: &str, secret: &str) -> Result<(), crate::github_tokens::SecretUnavailable> {
        self.set_secret(id, secret).map_err(unavailable_github)
    }

    fn get(&self, id: &str) -> Result<Option<String>, crate::github_tokens::SecretUnavailable> {
        self.get_secret(id).map_err(unavailable_github)
    }

    fn delete(&self, id: &str) -> Result<(), crate::github_tokens::SecretUnavailable> {
        self.delete_secret(id).map_err(unavailable_github)
    }

    fn presence(
        &self,
        ids: &[&str],
    ) -> Result<HashMap<String, bool>, crate::github_tokens::SecretUnavailable> {
        self.presence_of(ids).map_err(unavailable_github)
    }
}

impl crate::ai_shared::SecretStore for VaultSecrets {
    fn set(&self, id: &str, secret: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        self.set_secret(id, secret).map_err(unavailable_ai)
    }

    fn get(&self, id: &str) -> Result<Option<String>, crate::ai_shared::SecretUnavailable> {
        self.get_secret(id).map_err(unavailable_ai)
    }

    fn delete(&self, id: &str) -> Result<(), crate::ai_shared::SecretUnavailable> {
        self.delete_secret(id).map_err(unavailable_ai)
    }

    fn presence(
        &self,
        ids: &[&str],
    ) -> Result<HashMap<String, bool>, crate::ai_shared::SecretUnavailable> {
        self.presence_of(ids).map_err(unavailable_ai)
    }
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
