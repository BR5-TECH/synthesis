//! GitHub token storage — `specifications/core/GTS-github-token-storage.md`.
//!
//! The backend home for GitHub credentials, and the only component in the
//! application that can read a token's secret. The split that makes that claim
//! hold is the load-bearing part of this module:
//!
//! - **The secret** lives in the OS credential store, one entry per token keyed
//!   by the token's id (GTS-FR-01). Nothing else ever holds it.
//! - **The description** — id, label, resolved account, granted scopes, the
//!   last four characters, timestamps, verification state — lives in the
//!   user-global store (`global_settings.rs`, GSS-FR-22). That file therefore
//!   stays safe to read, copy, or attach to a bug report.
//!
//! `GithubTokenRecord` is the *only* representation of a token that crosses the
//! IPC boundary (GTS-FR-02), and it carries no secret material: the closest it
//! comes is `masked_hint`, the last four characters. The one function that
//! returns a secret — `resolve_github_token_secret` — is deliberately not a
//! `#[tauri::command]` and so is unreachable from the frontend (GTS-FR-13).
//!
//! Both outside dependencies are behind traits (`SecretStore`, `GithubVerifier`)
//! so the logic is unit-testable without an OS keychain or a network: CI runners
//! have neither, and a test that needed them would either fail there or be
//! deleted until it passed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::global_settings::{now_iso8601, GlobalSettingsStore};
use crate::log_fields;
use crate::logging::{log_info, log_warn, Domain, BUFFER};
use crate::project::ProjectState;

mod discussion_identity;
pub mod host;
mod verifier;
pub use host::{
    default_host, github_api_host,
    github_api_base, github_graphql_url, github_web_base, normalize_host, token_creation_url,
    DEFAULT_HOST, ERR_HOST_MISMATCH, ERR_INVALID_HOST,
};
pub use verifier::{
    account_from_user_payload, login_from_user_payload, parse_scopes, verify_failure_for_status,
    GithubIdentity, GithubVerifier, HttpGithubVerifier, VerifiedIdentity, VerifyError,
};
pub use discussion_identity::{
    resolve_github_identity_if_stored, GITHUB_TOKENS_CHANGED,
};
use discussion_identity::emit_tokens_changed;

// ---------------------------------------------------------------------------
// Typed errors (GTS-FR-04 / GTS-FR-10 / GTS-FR-14)
// ---------------------------------------------------------------------------
//
// The codebase's error channel is `Result<_, String>`, so a "typed" error is a
// stable string the frontend matches on. They are constants rather than inline
// literals because both sides depend on the exact spelling: `src/types.ts`
// mirrors this list, and a silent divergence would leave the UI unable to tell
// a rejected token from an unreachable GitHub.

/// GitHub refused the token (GTS-FR-04).
pub const ERR_INVALID_TOKEN: &str = "invalid_token";
/// Another stored token already carries this label (GTS-FR-05).
pub const ERR_DUPLICATE_LABEL: &str = "duplicate_label";
/// GitHub could not be reached — distinct from a token it rejected (GTS-FR-04).
pub const ERR_GITHUB_UNREACHABLE: &str = "github_unreachable";
/// The OS credential store is locked, absent, or refused access (GTS-FR-14).
pub const ERR_KEYCHAIN_UNAVAILABLE: &str = "keychain_unavailable";
/// No registry record carries the requested id.
pub const ERR_UNKNOWN_TOKEN: &str = "unknown_token";
/// Tokens are stored but this project has not been pointed at one (GTS-FR-10).
/// `GIT-git.md` GHA-FR-16 answers this one by opening the token picker.
pub const ERR_SELECTION_REQUIRED: &str = "github_token_selection_required";
/// No token is stored at all (GTS-FR-10). Nothing to pick between.
pub const ERR_TOKEN_MISSING: &str = "github_token_missing";
/// GTS-FR-16: a token resolves, but the registry holds no account for it — so
/// there is nobody to attribute a comment to. Distinct from a missing token,
/// because the author's fix is to verify the token rather than to add one.
pub const ERR_IDENTITY_UNRESOLVED: &str = "github_identity_unresolved";
/// A binding can only be set while a project is open.
pub const ERR_NO_PROJECT: &str = "no project is open";

/// GTS-FR-17: the service name of the **earlier** one-entry-per-token keyring
/// layout.
///
/// Nothing writes to it any more — every token secret lives in the application
/// secret vault (`ASV-application-secret-vault.md` ASV-FR-01) — but it is what
/// a migration candidate names, so a machine that stored tokens under the old
/// layout has them adopted into the one entry on the next launch.
pub const LEGACY_KEYCHAIN_SERVICE: &str = "com.synthesis.github-token";

/// GTS-FR-01: the vault namespace this module owns
/// (`ASV-application-secret-vault.md` ASV-FR-03). A token's secret is addressed
/// at `["github", "tokens", <id>]` and nowhere else.
pub const VAULT_NAMESPACE: &[&str] = &["github", "tokens"];

// ---------------------------------------------------------------------------
// Wire types (GTS-FR-02)
// ---------------------------------------------------------------------------

/// A record's verification state.
///
/// `Unavailable` is not persisted as a stored fact — it is derived at listing
/// time for a record whose keychain entry cannot be read (GTS-FR-08), which is
/// what a wiped keychain or a machine restored from a backup looks like.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenState {
    Valid,
    Invalid,
    #[default]
    Unverified,
    Unavailable,
}

/// The single representation of a token that crosses the IPC boundary
/// (GTS-FR-02). Carries no secret material — `masked_hint` is the last four
/// characters and is the closest this shape comes to one.
///
/// This is also the persisted shape of a registry entry (GSS-FR-22), so
/// `#[serde(default)]` matters: a `synthesis.toml` written before a field
/// existed must still load rather than sending the whole store through
/// GSS-FR-13's repair-to-defaults, which would wipe recents and both
/// registries.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct GithubTokenRecord {
    /// Stable and opaque. Also the keychain entry's account name, which is why
    /// it must never be reused after a removal.
    pub id: String,
    /// Author-chosen and unique across the registry (GTS-FR-05).
    pub label: String,
    /// The normalized host the token belongs to (GTS-FR-BJCN). A record stored
    /// without one reads as `github.com` (GTS-FR-VRYL), which is why the default
    /// is not the empty string.
    pub host: String,
    /// Resolved by verification; `None` until one succeeds.
    pub account_login: Option<String>,
    /// The account's display name, resolved by the same verification that
    /// resolves the login and retained beside it (GTS-FR-16). `None` when GitHub
    /// reports none, and on a record written before this field existed — which is
    /// why every field here carries `#[serde(default)]`.
    pub account_display_name: Option<String>,
    /// The account's email, on the same terms as `account_display_name`. Absent
    /// for an account that keeps its email private.
    pub account_email: Option<String>,
    /// Granted scopes as GitHub reports them. Empty for a fine-grained token,
    /// which reports none through this header.
    pub scopes: Vec<String>,
    /// The secret's last four characters (GTS-FR-02).
    pub masked_hint: String,
    pub added_at: String,
    pub last_verified_at: Option<String>,
    pub state: TokenState,
}

impl GithubTokenRecord {
    /// The host of this record, `github.com` where none is stored (GTS-FR-VRYL).
    pub fn effective_host(&self) -> String {
        host::stored_host(&self.host)
    }
}

/// How `get_project_github_token_binding` resolved the open project's token —
/// the value that tells the UI whether to prompt (GTS-FR-10).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BindingResolution {
    /// A recorded binding whose token still exists.
    Bound,
    /// No recorded binding, but exactly one token is stored — so it is used
    /// without asking. The single-token author is never prompted.
    Implicit,
    /// Two or more tokens and nothing chosen. The picker opens.
    SelectionRequired,
    /// No token is stored at all. Nothing to pick between; the caller routes to
    /// the Global settings GitHub section instead.
    #[default]
    NoneStored,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTokenBinding {
    pub token_id: Option<String>,
    pub resolution: BindingResolution,
}

// ---------------------------------------------------------------------------
// The two outside dependencies, behind traits
// ---------------------------------------------------------------------------

/// Why a credential-store operation failed. Only one variant, because the
/// distinction the caller acts on is "the entry is not there" (`Ok(None)` from
/// `get`) versus "the store itself would not answer".
#[derive(Debug)]
pub struct SecretUnavailable(pub String);

/// The OS credential store, narrowed to the three operations this module needs.
pub trait SecretStore: Send + Sync {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable>;
    /// `Ok(None)` means no entry exists for `id` — a normal state after a
    /// keychain wipe, and what GTS-FR-08 renders as `Unavailable`.
    fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable>;
    /// Removing an entry that is not there succeeds (GTS-FR-09 is idempotent).
    fn delete(&self, id: &str) -> Result<(), SecretUnavailable>;

    /// Whether each of `ids` has a stored secret, answered from **one** access
    /// of the store (`ASV-application-secret-vault.md` ASV-FR-30).
    ///
    /// This is what listing uses: the state a record reports depends on whether
    /// its secret is still readable, and a per-record `get` would cost one
    /// keyring access — and, where the platform asks, one authentication prompt
    /// — for every stored token. Booleans rather than values, so no secret
    /// enters the listing path at all.
    ///
    /// The default is the honest one for a store with no batch access; the
    /// vault overrides it with a single read.
    fn presence(
        &self,
        ids: &[&str],
    ) -> Result<std::collections::HashMap<String, bool>, SecretUnavailable> {
        let mut answers = std::collections::HashMap::with_capacity(ids.len());
        for id in ids {
            answers.insert((*id).to_string(), self.get(id)?.is_some());
        }
        Ok(answers)
    }
}


// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

/// The module's two collaborators, as Tauri-managed state. The registry itself
/// is not here — it lives in the user-global store (GSS-FR-22), which
/// `GlobalSettingsStore` owns; this holds only what that store deliberately
/// does not: the secrets, and the way to check them.
pub struct GithubTokens {
    secrets: Box<dyn SecretStore>,
    verifier: Box<dyn GithubVerifier>,
    /// Serialises read-modify-write cycles over the registry. Each command
    /// reads the registry, edits it, and writes it back; without this two
    /// concurrent `invoke`s could interleave and one would silently discard
    /// the other's record.
    write_lock: Mutex<()>,
}

impl Default for GithubTokens {
    fn default() -> Self {
        Self::new(
            // GTS-FR-01: the `github.tokens` namespace of the one application
            // keyring entry. This module holds no keyring entry of its own.
            Box::new(crate::secret_vault::VaultSecrets::new(
                crate::secret_vault::global(),
                VAULT_NAMESPACE,
            )),
            Box::new(HttpGithubVerifier),
        )
    }
}

impl GithubTokens {
    pub fn new(secrets: Box<dyn SecretStore>, verifier: Box<dyn GithubVerifier>) -> Self {
        Self {
            secrets,
            verifier,
            write_lock: Mutex::new(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// GTS-FR-02: the last four characters of a secret, the only token-derived text
/// that ever leaves this module.
///
/// Counts *characters* rather than bytes: a byte slice of a multi-byte
/// character would panic, and a GitHub token is ASCII only by convention —
/// never by anything this function can rely on.
pub fn mask_hint(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    let start = chars.len().saturating_sub(4);
    chars[start..].iter().collect()
}

/// GTS-FR-11 / GTC-FR-11: strip a secret from text that is about to be shown.
///
/// Applied to the output of an authenticated Git operation, where a remote URL
/// echoed by the transport can carry an embedded credential. Anything shorter
/// than a plausible token is left alone: replacing a two-character string would
/// corrupt unrelated output far more often than it would hide anything.
pub fn redact(text: &str, secret: &str) -> String {
    if secret.chars().count() < 8 {
        return text.to_string();
    }
    text.replace(secret, "***")
}

/// GTS-FR-10: resolve a project's token from the registry and its recorded
/// binding, and say *how* — which is what tells the UI whether to prompt.
///
/// A dangling binding (its record has since been removed) is treated exactly as
/// no binding at all: it falls through to the count rule, so an author whose
/// bound token was deleted while one other remains is not asked to choose
/// between a set of one.
pub fn resolve_binding(records: &[GithubTokenRecord], bound: Option<&str>) -> ProjectTokenBinding {
    if records.is_empty() {
        return ProjectTokenBinding {
            token_id: None,
            resolution: BindingResolution::NoneStored,
        };
    }
    if let Some(id) = bound {
        if records.iter().any(|r| r.id == id) {
            return ProjectTokenBinding {
                token_id: Some(id.to_string()),
                resolution: BindingResolution::Bound,
            };
        }
    }
    if records.len() == 1 {
        return ProjectTokenBinding {
            token_id: Some(records[0].id.clone()),
            resolution: BindingResolution::Implicit,
        };
    }
    ProjectTokenBinding {
        token_id: None,
        resolution: BindingResolution::SelectionRequired,
    }
}

/// Two labels an author could not tell apart are the same label (GTS-FR-05).
fn label_conflicts(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// GTS-FR-05: the label to file a token under when the author gave none.
///
/// The account the token authenticates as is the name they would have typed
/// anyway, and it is already resolved by the time this is called — so a label
/// is a convenience for telling two tokens of the *same* account apart, not a
/// thing to demand up front. A derived label is deduplicated silently
/// (`raver119`, `raver119 (2)`, …) rather than colliding: the author did not
/// choose it, so refusing their token over it would be nonsense.
pub fn derive_label_from_login(records: &[GithubTokenRecord], login: &str) -> String {
    let base = match login.trim() {
        "" => "github token",
        name => name,
    };
    if !records.iter().any(|r| label_conflicts(&r.label, base)) {
        return base.to_string();
    }
    // At most `records.len()` labels exist, so one of these must be free.
    (2..=records.len() + 2)
        .map(|n| format!("{base} ({n})"))
        .find(|candidate| !records.iter().any(|r| label_conflicts(&r.label, candidate)))
        .expect("a suffix beyond every existing label is always free")
}

/// A fresh, opaque, never-reused token id. Also the keychain account name, so
/// reuse would hand a new record the previous one's secret.
fn new_token_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ght-{nanos:x}-{seq:x}")
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------
//
// Each takes its collaborators explicitly rather than reaching for managed
// state, so every one is exercisable without a Tauri runtime. The
// `#[tauri::command]` wrappers below are the only Tauri-aware code here.

/// GTS-FR-08: the registry, with any record whose secret cannot be read marked
/// `Unavailable`. The probe is local — no network — so the Global settings
/// section still renders offline.
pub fn list_tokens_impl(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
) -> Result<Vec<GithubTokenRecord>, String> {
    let mut records = store.load_github_token_registry()?;
    // One vault access for the whole listing, whatever the number of records
    // (`ASV-application-secret-vault.md` ASV-FR-30). A vault that will not
    // answer downgrades the records rather than failing the listing
    // (GTS-FR-14): every record then reads `unavailable`, which is what the
    // author needs to see and act on.
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    let present = tokens.secrets.presence(&ids).unwrap_or_default();
    for record in records.iter_mut() {
        // The record is retained rather than pruned: an author is better served
        // by a row they can remove deliberately than by a token that silently
        // vanished.
        if !present.get(&record.id).copied().unwrap_or(false) {
            record.state = TokenState::Unavailable;
        }
    }
    Ok(records)
}

/// GTS-FR-04: verify first, then store — so a rejected token, an unreachable
/// GitHub, and a refusing keychain each leave nothing behind.
pub fn add_token_impl(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    label: &str,
    secret: &str,
    host: &str,
) -> Result<GithubTokenRecord, String> {
    let label = label.trim();
    let secret = secret.trim();
    // GTS-FR-BJCN: refused before any lock, request, or write.
    let host = normalize_host(host)?;
    if secret.is_empty() {
        return Err("token is empty".into());
    }

    let _guard = tokens
        .write_lock
        .lock()
        .map_err(|e| format!("github token store poisoned: {e}"))?;

    let mut records = store.load_github_token_registry()?;
    // A label the author *chose* is checked before the network call, so a
    // collision costs them a round trip rather than a wait.
    if !label.is_empty() && records.iter().any(|r| label_conflicts(&r.label, label)) {
        return Err(ERR_DUPLICATE_LABEL.into());
    }

    let identity = match tokens.verifier.verify(&host, secret) {
        Ok(identity) => identity,
        Err(VerifyError::Rejected) => return Err(ERR_INVALID_TOKEN.into()),
        Err(VerifyError::Unreachable(_)) => return Err(ERR_GITHUB_UNREACHABLE.into()),
        Err(VerifyError::TlsUntrusted(failure)) => return Err(failure.wire()),
    };

    // GTS-FR-05: an omitted label is derived from the account now that
    // verification has resolved it.
    let label = if label.is_empty() {
        derive_label_from_login(&records, &identity.login)
    } else {
        label.to_string()
    };

    let id = new_token_id();
    tokens
        .secrets
        .set(&id, secret)
        .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;

    let now = now_iso8601();
    let record = GithubTokenRecord {
        id: id.clone(),
        label,
        host,
        account_login: Some(identity.login),
        account_display_name: identity.display_name,
        account_email: identity.email,
        scopes: identity.scopes,
        masked_hint: mask_hint(secret),
        added_at: now.clone(),
        last_verified_at: Some(now),
        state: TokenState::Valid,
    };
    records.push(record.clone());

    // The keychain entry is already written, so a failed registry write would
    // otherwise strand a secret no record names — unreachable, undeletable, and
    // invisible. Roll it back rather than leaving one behind.
    if let Err(e) = store.save_github_token_registry(records) {
        let _ = tokens.secrets.delete(&id);
        return Err(e);
    }
    Ok(record)
}

/// GTS-FR-07: re-check a stored token and write what GitHub said back to its
/// record. A token GitHub rejects becomes `Invalid` and stays stored — losing
/// the row would hide exactly the fact the author needs.
pub fn validate_token_impl(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    id: &str,
) -> Result<GithubTokenRecord, String> {
    let _guard = tokens
        .write_lock
        .lock()
        .map_err(|e| format!("github token store poisoned: {e}"))?;

    let mut records = store.load_github_token_registry()?;
    let index = records
        .iter()
        .position(|r| r.id == id)
        .ok_or(ERR_UNKNOWN_TOKEN)?;

    let secret = match tokens.secrets.get(id) {
        Ok(Some(secret)) => secret,
        Ok(None) => {
            // Derived, not stored (GTS-FR-08): persisting it would leave the
            // record reading `unavailable` even after its keychain entry came
            // back, until someone pressed Verify again. `list_tokens_impl`
            // re-derives it on every read, so nothing is lost by not writing.
            let mut record = records[index].clone();
            record.state = TokenState::Unavailable;
            return Ok(record);
        }
        Err(_) => return Err(ERR_KEYCHAIN_UNAVAILABLE.into()),
    };

    match tokens.verifier.verify(&records[index].effective_host(), &secret) {
        Ok(identity) => {
            let record = &mut records[index];
            record.state = TokenState::Valid;
            record.account_login = Some(identity.login);
            // GTS-FR-16: the account's describable facts are refreshed together
            // with the login, so a rename on GitHub reaches the attribution the
            // next comment is stamped with.
            record.account_display_name = identity.display_name;
            record.account_email = identity.email;
            record.scopes = identity.scopes;
            record.last_verified_at = Some(now_iso8601());
        }
        Err(VerifyError::Rejected) => {
            let record = &mut records[index];
            record.state = TokenState::Invalid;
            record.last_verified_at = Some(now_iso8601());
        }
        // GitHub never answered, so nothing is known that was not known before.
        // Leaving the record untouched is what keeps a flaky network from
        // relabelling a good token as bad.
        Err(VerifyError::Unreachable(_)) => return Err(ERR_GITHUB_UNREACHABLE.into()),
        Err(VerifyError::TlsUntrusted(failure)) => return Err(failure.wire()),
    }

    let record = records[index].clone();
    store.save_github_token_registry(records)?;
    Ok(record)
}

/// GTS-FR-05: relabel, refusing a label another record already carries.
pub fn rename_token_impl(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    id: &str,
    label: &str,
) -> Result<GithubTokenRecord, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("label is empty".into());
    }

    let _guard = tokens
        .write_lock
        .lock()
        .map_err(|e| format!("github token store poisoned: {e}"))?;

    let mut records = store.load_github_token_registry()?;
    let index = records
        .iter()
        .position(|r| r.id == id)
        .ok_or(ERR_UNKNOWN_TOKEN)?;
    if records
        .iter()
        .any(|r| r.id != id && label_conflicts(&r.label, label))
    {
        return Err(ERR_DUPLICATE_LABEL.into());
    }
    records[index].label = label.to_string();
    let record = records[index].clone();
    store.save_github_token_registry(records)?;
    Ok(record)
}

/// GTS-FR-09: delete the keychain entry and the registry record together.
/// Idempotent — the outcome is that neither exists.
pub fn remove_token_impl(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    id: &str,
) -> Result<(), String> {
    let _guard = tokens
        .write_lock
        .lock()
        .map_err(|e| format!("github token store poisoned: {e}"))?;

    // Order matters: drop the secret first. A registry write that succeeded
    // while the delete failed would leave a secret nothing names.
    tokens
        .secrets
        .delete(id)
        .map_err(|_| ERR_KEYCHAIN_UNAVAILABLE.to_string())?;
    let mut records = store.load_github_token_registry()?;
    records.retain(|r| r.id != id);
    store.save_github_token_registry(records)
}

/// GTS-FR-17: one migration candidate per registry record.
///
/// A candidate is supplied for **every** record, whether or not the vault
/// already holds that record's secret: which secrets are adopted and which
/// legacy entries are deleted is the vault's decision
/// (`ASV-application-secret-vault.md` ASV-FR-21, ASV-FR-25), and this module
/// reads no legacy entry itself.
pub fn migration_candidates(records: &[GithubTokenRecord]) -> Vec<crate::secret_vault::Candidate> {
    records
        .iter()
        .map(|record| crate::secret_vault::Candidate {
            legacy_service: LEGACY_KEYCHAIN_SERVICE.to_string(),
            legacy_account: record.id.clone(),
            path: crate::secret_vault::path(&["github", "tokens", &record.id]),
        })
        .collect()
}

/// GTS-FR-10: how the open project resolves a token right now.
pub fn get_binding_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<ProjectTokenBinding, String> {
    let records = store.load_github_token_registry()?;
    // An empty key is "no project open" (`ProjectState::slot_key`), whose slot
    // is the shared default one — a binding read from there would belong to no
    // project in particular.
    let bound = if project_key.is_empty() {
        None
    } else {
        store.load_github_token_binding(project_key)?
    };
    Ok(resolve_binding(&records, bound.as_deref()))
}

/// GTS-FR-11: point the open project at a stored token.
pub fn set_binding_impl(
    store: &GlobalSettingsStore,
    project_key: &str,
    token_id: &str,
) -> Result<ProjectTokenBinding, String> {
    if project_key.is_empty() {
        return Err(ERR_NO_PROJECT.into());
    }
    let records = store.load_github_token_registry()?;
    if !records.iter().any(|r| r.id == token_id) {
        return Err(ERR_UNKNOWN_TOKEN.into());
    }
    store.save_github_token_binding(project_key, token_id)?;
    Ok(resolve_binding(&records, Some(token_id)))
}

/// GTS-FR-13 / GTS-FR-OBAS: the single read path for a secret in the application.
///
/// Deliberately **not** a `#[tauri::command]`: it is not registered in
/// `generate_handler!`, so no frontend `invoke` can reach it. `GTC-git.md`
/// GTC-FR-09, `GHP-github-publication.md`, and `GPP-github-polling.md` call this
/// when performing an authenticated GitHub operation, and nothing else does.
///
/// The refusals it returns are the same distinction `get_binding_impl` reports,
/// so a caller can route a selection-required failure to the picker
/// (`GIT-git.md` GHA-FR-16) and a missing-token failure to Global settings. A
/// token that belongs to another host than `remote_host` is refused with
/// `github_host_mismatch` **before** the vault is asked for anything.
pub fn resolve_github_token_secret(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
    remote_host: &str,
) -> Result<String, String> {
    let (id, token_host) = resolve_project_token_id(store, project_key)?;
    if token_host != host::stored_host(remote_host) {
        return Err(ERR_HOST_MISMATCH.into());
    }
    read_secret(tokens, &id)
}

/// A token the open project resolves to, with the host it belongs to.
///
/// The secret is held for the length of one operation and never returned across
/// the IPC boundary.
pub struct ProjectToken {
    pub secret: String,
    pub host: String,
}

/// GTS-FR-13: the token the open project resolves to and its host, for an
/// operation that learns the host of its repository only after it has the token
/// (publication, polling). The caller compares the host with the host of its
/// remote before it sends the secret anywhere (GTS-FR-OBAS).
pub fn resolve_project_token(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
) -> Result<ProjectToken, String> {
    let (id, host) = resolve_project_token_id(store, project_key)?;
    let secret = read_secret(tokens, &id)?;
    Ok(ProjectToken { secret, host })
}

/// GTS-FR-10: the id and the host of the token the project resolves to, or the
/// typed refusal.
fn resolve_project_token_id(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<(String, String), String> {
    let records = store.load_github_token_registry()?;
    let bound = if project_key.is_empty() {
        None
    } else {
        store.load_github_token_binding(project_key)?
    };
    let binding = resolve_binding(&records, bound.as_deref());
    let id = match binding.resolution {
        BindingResolution::Bound | BindingResolution::Implicit => binding
            .token_id
            .ok_or_else(|| ERR_SELECTION_REQUIRED.to_string())?,
        BindingResolution::SelectionRequired => return Err(ERR_SELECTION_REQUIRED.into()),
        BindingResolution::NoneStored => return Err(ERR_TOKEN_MISSING.into()),
    };
    let record = records.iter().find(|r| r.id == id).ok_or(ERR_UNKNOWN_TOKEN)?;
    Ok((id, record.effective_host()))
}

fn read_secret(tokens: &GithubTokens, id: &str) -> Result<String, String> {
    match tokens.secrets.get(id) {
        Ok(Some(secret)) => Ok(secret),
        // The record exists but its secret does not: the token is unusable, and
        // saying so beats presenting an empty credential to GitHub.
        Ok(None) | Err(_) => Err(ERR_KEYCHAIN_UNAVAILABLE.into()),
    }
}

/// The hosts of every stored token, for deciding whether a remote host is a
/// GitHub host (GTC-FR-FSLC).
pub fn known_github_hosts(store: &GlobalSettingsStore) -> Vec<String> {
    store
        .load_github_token_registry()
        .map(|records| records.iter().map(GithubTokenRecord::effective_host).collect())
        .unwrap_or_default()
}

/// GTC-FR-FSLC: whether `host` is a GitHub host — `github.com`, a `*.ghe.com`
/// host, or one of the `known` hosts of stored tokens.
pub fn is_github_host(host: &str, known: &[String]) -> bool {
    host::is_github_family_host(host) || known.iter().any(|k| k == host)
}

/// GTC-FR-09 / GTC-FR-FSLC: the secret for a Git remote URL, or `None` when the
/// remote is not one GitHub tokens apply to.
///
/// Only an HTTPS remote on a GitHub host takes a token: `github.com`, a
/// `*.ghe.com` host, or the host of any stored token. A remote on another
/// provider authenticates as it otherwise would. For a GitHub remote the token
/// must belong to the host of the remote (GTS-FR-OBAS).
pub fn resolve_remote_token(
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
    url: &str,
) -> Result<Option<String>, String> {
    let Some(remote_host) = host::https_remote_host(url) else {
        return Ok(None);
    };
    if !is_github_host(&remote_host, &known_github_hosts(store)) {
        return Ok(None);
    }
    resolve_github_token_secret(store, tokens, project_key, &remote_host).map(Some)
}

/// GTS-FR-16: who this machine writes as, for the token the open project
/// resolves to.
///
/// Deliberately **not** a `#[tauri::command]`, like `resolve_github_token_secret`
/// beside it: it is reached only from `CMS-comments-storage.md` (CMS-FR-12),
/// which stamps the result into a comment's `by` itself so no frontend call can
/// claim to be someone else.
///
/// Unlike the secret path it never touches the keychain — the account's facts
/// live in the registry (GTS-FR-03) — so an author whose keychain is locked can
/// still be attributed. It refuses with the same typed distinction
/// `get_binding_impl` reports, so a caller routes a selection-required failure to
/// the picker and a nothing-stored failure to Global settings.
pub fn resolve_github_identity(
    store: &GlobalSettingsStore,
    project_key: &str,
) -> Result<GithubIdentity, String> {
    let records = store.load_github_token_registry()?;
    let bound = if project_key.is_empty() {
        None
    } else {
        store.load_github_token_binding(project_key)?
    };
    let binding = resolve_binding(&records, bound.as_deref());
    let id = match binding.resolution {
        BindingResolution::Bound | BindingResolution::Implicit => binding
            .token_id
            .ok_or_else(|| ERR_SELECTION_REQUIRED.to_string())?,
        BindingResolution::SelectionRequired => return Err(ERR_SELECTION_REQUIRED.into()),
        BindingResolution::NoneStored => return Err(ERR_TOKEN_MISSING.into()),
    };
    let record = records
        .iter()
        .find(|r| r.id == id)
        .ok_or(ERR_UNKNOWN_TOKEN)?;
    // `add_token_impl` verifies before it stores (GTS-FR-04), so a stored record
    // always carries a login. One that does not was hand-edited or predates the
    // field, and there is no honest identity to stamp — say so distinctly rather
    // than reporting a token problem the author does not have.
    let login = record
        .account_login
        .clone()
        .filter(|l| !l.trim().is_empty())
        .ok_or(ERR_IDENTITY_UNRESOLVED)?;
    Ok(GithubIdentity {
        login,
        display_name: record.account_display_name.clone(),
        email: record.account_email.clone(),
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_github_tokens(
    store: State<'_, GlobalSettingsStore>,
    tokens: State<'_, GithubTokens>,
) -> Result<Vec<GithubTokenRecord>, String> {
    list_tokens_impl(&store, &tokens)
}

#[tauri::command]
pub fn add_github_token(
    label: String,
    secret: String,
    host: Option<String>,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    tokens: State<'_, GithubTokens>,
) -> Result<GithubTokenRecord, String> {
    let record = add_token_impl(&store, &tokens, &label, &secret, host.as_deref().unwrap_or(""))
        .inspect_err(|code| log_refusal(&app, "github token add refused", code))?;
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend, Domain::Remote],
        "github token added",
        log_fields! { "token_id" => &record.id, "host" => &record.host },
    );
    emit_tokens_changed(&app);
    Ok(record)
}

#[tauri::command]
pub fn validate_github_token(
    id: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    tokens: State<'_, GithubTokens>,
) -> Result<GithubTokenRecord, String> {
    let record = validate_token_impl(&store, &tokens, &id)
        .inspect_err(|code| log_refusal(&app, "github token verification failed", code))?;
    emit_tokens_changed(&app);
    Ok(record)
}

#[tauri::command]
pub fn rename_github_token(
    id: String,
    label: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    tokens: State<'_, GithubTokens>,
) -> Result<GithubTokenRecord, String> {
    let record = rename_token_impl(&store, &tokens, &id, &label)?;
    emit_tokens_changed(&app);
    Ok(record)
}

#[tauri::command]
pub fn remove_github_token(
    id: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    tokens: State<'_, GithubTokens>,
) -> Result<(), String> {
    remove_token_impl(&store, &tokens, &id)?;
    emit_tokens_changed(&app);
    Ok(())
}

/// GTS-FR-12: hand the OS the token-creation URL of the host and return.
/// Transmits nothing, receives nothing — the token comes back only by the author
/// pasting it.
#[tauri::command]
pub fn open_github_token_creation_page(
    host: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let host = normalize_host(host.as_deref().unwrap_or(""))
        .inspect_err(|code| log_refusal(&app, "github token page refused", code))?;
    app.opener()
        .open_url(token_creation_url(&host), None::<&str>)
        .map_err(|e| {
            log_refusal(&app, "github token page not opened", "browser_unavailable");
            format!("could not open the browser: {e}")
        })
}

/// A refusal of a token command. Only the typed code is logged: the secret, the
/// label, and the host as typed stay out of the record.
fn log_refusal(app: &tauri::AppHandle, message: &str, code: &str) {
    log_warn(
        app,
        &BUFFER,
        &[Domain::Backend, Domain::Remote],
        message,
        log_fields! { "error" => code },
    );
}

#[tauri::command]
pub fn get_project_github_token_binding(
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ProjectTokenBinding, String> {
    get_binding_impl(&store, &project.slot_key())
}

#[tauri::command]
pub fn set_project_github_token_binding(
    token_id: String,
    app: tauri::AppHandle,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<ProjectTokenBinding, String> {
    let binding = set_binding_impl(&store, &project.slot_key(), &token_id)?;
    emit_tokens_changed(&app);
    Ok(binding)
}

#[cfg(test)]
mod tests;
