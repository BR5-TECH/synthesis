//! The test scenarios of `specifications/core/GTS-github-token-storage.md`.
//!
//! Both outside dependencies are faked, so every test runs with no keychain and
//! no network. This module holds the fakes and the harness; the topic modules
//! hold the tests.

use super::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

// -- Test doubles ----------------------------------------------------
//
// Both outside dependencies are faked so every test below runs with no
// keychain and no network. That is not only a speed choice: CI runs the
// Rust suite on a headless ubuntu runner with neither available
// (`CIP-ci-pipeline.md`), so a test that reached for either would fail
// there while passing locally.

#[derive(Default)]
struct FakeSecrets {
    entries: StdMutex<HashMap<String, String>>,
    /// When set, every operation reports the store as unavailable.
    broken: bool,
}

impl FakeSecrets {
    fn broken() -> Self {
        Self {
            broken: true,
            ..Default::default()
        }
    }
    fn count(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
    fn contains(&self, id: &str) -> bool {
        self.entries.lock().unwrap().contains_key(id)
    }
    /// Simulate the keychain losing an entry the registry still names.
    fn forget(&self, id: &str) {
        self.entries.lock().unwrap().remove(id);
    }
}

impl SecretStore for FakeSecrets {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable> {
        if self.broken {
            return Err(SecretUnavailable("locked".into()));
        }
        self.entries
            .lock()
            .unwrap()
            .insert(id.to_string(), secret.to_string());
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable> {
        if self.broken {
            return Err(SecretUnavailable("locked".into()));
        }
        Ok(self.entries.lock().unwrap().get(id).cloned())
    }
    fn delete(&self, id: &str) -> Result<(), SecretUnavailable> {
        if self.broken {
            return Err(SecretUnavailable("locked".into()));
        }
        self.entries.lock().unwrap().remove(id);
        Ok(())
    }
}

/// A verifier whose answer is scripted per secret, and can be rescripted
/// mid-test — which is what a token being revoked on GitHub looks like.
struct FakeVerifier {
    good: StdMutex<HashMap<String, VerifiedIdentity>>,
    unreachable: bool,
}

impl FakeVerifier {
    fn accepting(secret: &str, login: &str, scopes: &[&str]) -> Self {
        Self::accepting_account(secret, login, None, None, scopes)
    }

    /// GTS-FR-16: the same, with the account's display name and email
    /// scripted, so the propagation into the record is exercisable.
    fn accepting_account(
        secret: &str,
        login: &str,
        display_name: Option<&str>,
        email: Option<&str>,
        scopes: &[&str],
    ) -> Self {
        let mut good = HashMap::new();
        good.insert(
            secret.to_string(),
            VerifiedIdentity {
                login: login.to_string(),
                display_name: display_name.map(|s| s.to_string()),
                email: email.map(|s| s.to_string()),
                scopes: scopes.iter().map(|s| s.to_string()).collect(),
            },
        );
        Self {
            good: StdMutex::new(good),
            unreachable: false,
        }
    }
    fn rejecting() -> Self {
        Self {
            good: StdMutex::new(HashMap::new()),
            unreachable: false,
        }
    }
    fn offline() -> Self {
        Self {
            good: StdMutex::new(HashMap::new()),
            unreachable: true,
        }
    }
    fn accept(&self, secret: &str, login: &str, scopes: &[&str]) {
        self.good.lock().unwrap().insert(
            secret.to_string(),
            VerifiedIdentity {
                login: login.to_string(),
                display_name: None,
                email: None,
                scopes: scopes.iter().map(|s| s.to_string()).collect(),
            },
        );
    }
    /// The token was revoked on GitHub: it stops being recognised.
    fn revoke(&self, secret: &str) {
        self.good.lock().unwrap().remove(secret);
    }
}

impl GithubVerifier for FakeVerifier {
    fn verify(&self, _host: &str, secret: &str) -> Result<VerifiedIdentity, VerifyError> {
        if self.unreachable {
            return Err(VerifyError::Unreachable("no route to host".into()));
        }
        self.good
            .lock()
            .unwrap()
            .get(secret)
            .cloned()
            .ok_or(VerifyError::Rejected)
    }
}

/// A store plus its collaborators, with both fakes still reachable so a
/// test can assert on what reached the keychain and rescript the verifier.
struct Harness {
    store: GlobalSettingsStore,
    tokens: GithubTokens,
    secrets: Arc<FakeSecrets>,
    verifier: Arc<FakeVerifier>,
}

fn harness(verifier: FakeVerifier) -> Harness {
    let secrets = Arc::new(FakeSecrets::default());
    let verifier = Arc::new(verifier);
    let tokens = GithubTokens::new(
        Box::new(ArcSecrets(secrets.clone())),
        Box::new(ArcVerifier(verifier.clone())),
    );
    Harness {
        store: GlobalSettingsStore::in_memory(),
        tokens,
        secrets,
        verifier,
    }
}

/// Let a test hold the same fake the store is using. The store takes a
/// `Box<dyn _>`, so a shared handle needs this thin forwarding wrapper.
struct ArcSecrets(Arc<FakeSecrets>);
impl SecretStore for ArcSecrets {
    fn set(&self, id: &str, secret: &str) -> Result<(), SecretUnavailable> {
        self.0.set(id, secret)
    }
    fn get(&self, id: &str) -> Result<Option<String>, SecretUnavailable> {
        self.0.get(id)
    }
    fn delete(&self, id: &str) -> Result<(), SecretUnavailable> {
        self.0.delete(id)
    }
}

struct ArcVerifier(Arc<FakeVerifier>);
impl GithubVerifier for ArcVerifier {
    fn verify(&self, host: &str, secret: &str) -> Result<VerifiedIdentity, VerifyError> {
        self.0.verify(host, secret)
    }
}

fn record(id: &str, label: &str) -> GithubTokenRecord {
    GithubTokenRecord {
        id: id.into(),
        label: label.into(),
        ..Default::default()
    }
}

use crate::secret_vault::test_support::FakeKeyring;
use crate::secret_vault::{Vault, VaultSecrets};

/// This module's collaborators wired to a real vault over a fake keyring,
/// exactly as `lib.rs` wires them in production — including the migration
/// candidate source.
fn vault_harness(
    keyring: &FakeKeyring,
    registry: Vec<GithubTokenRecord>,
    verifier: FakeVerifier,
) -> (GlobalSettingsStore, GithubTokens) {
    let store = GlobalSettingsStore::in_memory();
    store.save_github_token_registry(registry.clone()).unwrap();
    let vault = Arc::new(Vault::new(Box::new(keyring.clone())));
    vault.set_candidate_source(Box::new(move || migration_candidates(&registry)));
    let tokens = GithubTokens::new(
        Box::new(VaultSecrets::new(vault, VAULT_NAMESPACE)),
        Box::new(verifier),
    );
    (store, tokens)
}

mod vault_contract;
mod masking;
mod binding_resolution;
mod adding;
mod listing;
mod host_rules;
mod bindings;
mod identity;
mod wire_shapes;
mod verifier_rules;
mod state_and_concurrency;
