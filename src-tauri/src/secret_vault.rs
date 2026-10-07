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
//! - **One entry, one cache.** The first secret-dependent operation reads the
//!   entry once and fills a process cache of every secret; later reads and
//!   presence checks make no keyring access, so the author answers one
//!   authentication prompt instead of one per operation (ASV-FR-ZUGZ,
//!   ASV-FR-DQHY). The cache lives in memory only and assumes no other process
//!   changes the entry (ASV-FR-SUXZ, ASV-FR-ELQN).
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
    /// ASV-FR-ZUGZ: the process cache of the whole decoded object. Empty until
    /// the first secret-dependent operation fills it, and discarded with the
    /// process. Locked only while `lock` is held, except for the brief check
    /// that decides whether an initialization attempt is needed.
    cache: Mutex<Option<Cached>>,
    /// ASV-FR-CIDB: allows one initialization attempt at a time.
    gate: InitGate,
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
            cache: Mutex::new(None),
            gate: InitGate::new(),
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
    ///
    /// ASV-FR-DQHY: answered from the process cache. Only the request that
    /// initializes the cache touches the keyring.
    pub fn read_secret(&self, path: &[String]) -> Result<Option<String>, VaultError> {
        self.ensure_cache("read_secret")?;
        let _guard = self.acquire();
        match self.cache_slot().as_ref() {
            Some(cached) => Ok(lookup(&cached.object, path)),
            None => Err(self.failed("read_secret", VaultError::Unavailable)),
        }
    }

    /// ASV-FR-30: presence for every supplied path from the process cache, as
    /// booleans rather than values. A listing that describes many records
    /// therefore makes no keyring access once the cache is initialized.
    pub fn secret_presence(
        &self,
        paths: &[SecretPath],
    ) -> Result<HashMap<SecretPath, bool>, VaultError> {
        self.ensure_cache("secret_presence")?;
        let _guard = self.acquire();
        let slot = self.cache_slot();
        let Some(cached) = slot.as_ref() else {
            return Err(self.failed("secret_presence", VaultError::Unavailable));
        };
        let mut answers = HashMap::with_capacity(paths.len());
        for path in paths {
            answers.insert(path.clone(), lookup(&cached.object, path).is_some());
        }
        Ok(answers)
    }

    // -- Mutation --------------------------------------------------------

    /// ASV-FR-10 / ASV-FR-13: apply every mutation of the request to a copy of
    /// the cached object and write that object back as one keyring value,
    /// verified. Nothing is applied where anything fails (ASV-FR-12), and the
    /// cache takes the new object only after verification (ASV-FR-GLJD).
    pub fn apply_secret_mutations(&self, mutations: &[Mutation]) -> Result<(), VaultError> {
        const OPERATION: &str = "apply_secret_mutations";
        // ASV-FR-DIEF: an entry that would not decode leaves the cache empty,
        // and a mutation is what recovers it.
        match self.ensure_cache(OPERATION) {
            Ok(()) | Err(VaultError::Malformed) => {}
            Err(error) => return Err(error),
        }
        let _guard = self.acquire();
        let (mut object, previous, quarantined) = match self.cache_base() {
            Some((object, serialized)) => (object, serialized, false),
            None => {
                let snapshot = self.load_locked(OPERATION)?;
                match snapshot.state {
                    Loaded::Object(object) => (object, snapshot.raw, false),
                    Loaded::Absent => (empty_object(), None, false),
                    // ASV-FR-17: recovery destroys nothing. The undecodable
                    // value rides along in the same single verified write as
                    // the new secrets.
                    Loaded::Malformed(raw) => {
                        let mut object = empty_object();
                        object.insert(FIELD_QUARANTINE.to_string(), Value::String(raw.clone()));
                        (object, Some(raw), true)
                    }
                }
            }
        };
        for mutation in mutations {
            match mutation {
                Mutation::Set { path, secret } => {
                    if !insert_at(&mut object, path, secret) {
                        // A path whose parent is a string rather than an object
                        // cannot be created without destroying the secret that
                        // sits there. Refuse rather than overwrite.
                        return Err(self.failed(OPERATION, VaultError::WriteFailed));
                    }
                }
                Mutation::Remove { path } => remove_at(&mut object, path),
            }
        }
        let serialized = self.write_verified_locked(OPERATION, &object, previous.as_deref())?;
        self.store_cache_locked(OPERATION, object, Some(serialized));
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
        let outcome =
            self.migrate_locked("migrate_legacy_secrets", candidates, &mut false)?;
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
    /// requirement that the lock covers the keyring access alone.
    fn pending_candidates(&self) -> Option<Vec<Candidate>> {
        if self.migrated.load(Ordering::SeqCst) {
            return None;
        }
        let slot = self.candidates.lock().ok()?;
        let source = slot.as_ref()?;
        Some(source())
    }

    /// Lock the cache slot. The caller holds `lock`, except in
    /// [`Self::needs_initialization`].
    fn cache_slot(&self) -> std::sync::MutexGuard<'_, Option<Cached>> {
        match self.cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// A copy of the cached object and the serialized value it agrees with.
    fn cache_base(&self) -> Option<(Map<String, Value>, Option<String>)> {
        self.cache_slot()
            .as_ref()
            .map(|cached| (cached.object.clone(), cached.serialized.clone()))
    }

    /// Put `object` in the cache. Called only with an object that the entry
    /// verifiably holds: one just read from it, or one just written and read
    /// back (ASV-FR-GLJD).
    fn store_cache_locked(
        &self,
        operation: &'static str,
        object: Map<String, Value>,
        serialized: Option<String>,
    ) {
        let first = {
            let mut slot = self.cache_slot();
            let first = slot.is_none();
            *slot = Some(Cached { object, serialized });
            first
        };
        if first {
            self.emit(
                LogLevel::Debug,
                "secret vault cache initialized",
                log_fields! { "operation" => operation },
            );
        }
    }

    /// ASV-FR-ZUGZ: whether a request has to start an initialization attempt.
    /// The cache is empty, or a candidate source arrived after the cache was
    /// filled and migration has not run yet.
    fn needs_initialization(&self) -> bool {
        if self.cache_slot().is_none() {
            return true;
        }
        if self.migrated.load(Ordering::SeqCst) {
            return false;
        }
        matches!(self.candidates.lock().map(|slot| slot.is_some()), Ok(true))
    }

    /// ASV-FR-ZUGZ / ASV-FR-CIDB / ASV-FR-FTFU: make sure the cache serves
    /// requests. The first request of the process starts an attempt; a request
    /// that arrives during it waits and takes its result. A failure leaves the
    /// cache empty and reaches the requesting operation as the vault's typed
    /// error.
    fn ensure_cache(&self, operation: &'static str) -> Result<(), VaultError> {
        self.gate.run(
            || self.needs_initialization(),
            || {
                // Gathered before the lock is taken (see `pending_candidates`).
                let pending = self.pending_candidates();
                let _guard = self.acquire();
                self.initialize_locked(operation, pending)
            },
        )
    }

    /// ASV-FR-ZRWU: complete migration (ASV-FR-20), then hold the consolidated
    /// object. Migration reads the entry itself, so a successful attempt reads
    /// it once however it got there.
    ///
    /// Whether a failed migration is retried turns on **how far it got**, which
    /// is what keeps the cost of a broken machine bounded. A migration that
    /// failed before it read a single legacy entry — an unreadable, malformed,
    /// or newer-than-this-build consolidated entry — leaves the flag unset,
    /// because retrying it costs one read of the one entry and the state it
    /// failed on is one a mutation can clear. A migration that got as far as
    /// reading legacy entries marks itself done whatever happened next: the
    /// alternative is re-reading every legacy entry on every subsequent
    /// attempt, which on a platform that prompts per keyring access means one
    /// authentication prompt per stored secret. ASV-FR-26 covers the rest — the
    /// next launch migrates again. `UnsupportedVersion` is marked done too: no
    /// mutation of this build can clear it (ASV-FR-19), so retrying would only
    /// repeat a prompt for the same answer.
    fn initialize_locked(
        &self,
        operation: &'static str,
        pending: Option<Vec<Candidate>>,
    ) -> Result<(), VaultError> {
        // `None` is either "already done" or "nothing has told this vault what
        // the legacy layout held". The second is not marked done: the source is
        // installed at startup, and a caller that ran first must not cost the
        // process its migration.
        if let Some(candidates) = pending {
            if !self.migrated.load(Ordering::SeqCst) {
                let mut read_a_legacy_entry = false;
                match self.migrate_locked(operation, &candidates, &mut read_a_legacy_entry) {
                    Ok(_) => self.migrated.store(true, Ordering::SeqCst),
                    Err(VaultError::UnsupportedVersion) => {
                        self.migrated.store(true, Ordering::SeqCst);
                        return Err(VaultError::UnsupportedVersion);
                    }
                    // The legacy entries were reached and the write failed. The
                    // plain load below decides what the entry holds now.
                    Err(_) if read_a_legacy_entry => self.migrated.store(true, Ordering::SeqCst),
                    // `Unavailable` and `Malformed` are both states a later
                    // moment can clear — the keychain unlocks, or a mutation
                    // quarantines the value. Already logged.
                    Err(error) => return Err(error),
                }
            }
        }
        if self.cache_slot().is_some() {
            return Ok(());
        }
        let snapshot = self.load_locked(operation)?;
        match snapshot.state {
            Loaded::Object(object) => self.store_cache_locked(operation, object, snapshot.raw),
            Loaded::Absent => self.store_cache_locked(operation, empty_object(), None),
            Loaded::Malformed(_) => return Err(self.failed(operation, VaultError::Malformed)),
        }
        Ok(())
    }

    /// `read_a_legacy_entry` is set as soon as this reaches the legacy entries,
    /// which is what [`Self::ensure_migrated_locked`] decides retry policy on.
    fn migrate_locked(
        &self,
        operation: &'static str,
        candidates: &[Candidate],
        read_a_legacy_entry: &mut bool,
    ) -> Result<MigrationOutcome, VaultError> {
        let mut outcome = MigrationOutcome::default();
        if candidates.is_empty() {
            return Ok(outcome);
        }

        // Once the cache holds the object, migration works on it rather than
        // reading the entry again (ASV-FR-DQHY).
        let snapshot = match self.cache_base() {
            Some((object, serialized)) => Snapshot {
                raw: serialized,
                state: Loaded::Object(object),
            },
            None => self.load_locked("migrate_legacy_secrets")?,
        };
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
            // Nothing was written, so `object` is what the entry holds.
            if self.cache_slot().is_none() {
                self.store_cache_locked(operation, object, snapshot.raw);
            }
            return Ok(outcome);
        }

        let serialized = self.write_verified_locked(
            "migrate_legacy_secrets",
            &object,
            snapshot.raw.as_deref(),
        )?;
        self.store_cache_locked(operation, object, Some(serialized));

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
    ) -> Result<String, VaultError> {
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
            return Ok(serialized);
        }

        // Best effort: the entry must hold what it held before the request.
        // A rollback the keyring also refuses leaves the entry where the failed
        // write put it, which is the one outcome nothing here can improve on —
        // it is still reported as a failure. The entry is then of unknown
        // state, so the cache is emptied and the next request reads the entry
        // again (ASV-FR-GLJD).
        let restored = match previous {
            Some(previous) => self.backend.write(previous).is_ok(),
            None => self.backend.delete().is_ok(),
        };
        if !restored {
            *self.cache_slot() = None;
        }
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

mod adapter;
mod cache;
mod object;

pub use adapter::VaultSecrets;
use cache::{Cached, InitGate};
#[cfg(test)]
use object::addressable;
use object::{decode, empty_object, insert_at, lookup, remove_at, Decoded};

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
