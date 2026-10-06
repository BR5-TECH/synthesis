//! Vault tests — `specifications/core/ASV-application-secret-vault.md`
//! ASV-FR-01, ASV-FR-02, ASV-FR-03, ASV-FR-04 … ASV-FR-33.
//!
//! Every one drives the module through the `VaultBackend` seam, so the suite
//! runs on a CI machine with no keychain. The fake keyring records every call
//! it was handed, which is what lets a scenario assert *how many* accesses an
//! operation made and *in what order* a write and a deletion happened — both of
//! which are requirements here rather than incidental behaviour.

use std::collections::HashMap;
use std::sync::Arc;

use super::test_support::{Call, FakeKeyring};
use super::*;

/// A vault over a fresh fake keyring, with migration already accounted for so a
/// scenario that is not about migration does not pay for one.
fn vault() -> (Vault, FakeKeyring) {
    let keyring = FakeKeyring::default();
    let vault = Vault::new(Box::new(keyring.clone()));
    vault.mark_migrated();
    (vault, keyring)
}

/// A vault whose migration has *not* run, for the scenarios that drive it.
fn unmigrated_vault() -> (Vault, FakeKeyring) {
    let keyring = FakeKeyring::default();
    let vault = Vault::new(Box::new(keyring.clone()));
    (vault, keyring)
}

fn p(segments: &[&str]) -> SecretPath {
    path(segments)
}

fn set(vault: &Vault, segments: &[&str], secret: &str) -> Result<(), VaultError> {
    vault.apply_secret_mutations(&[Mutation::Set {
        path: p(segments),
        secret: secret.to_string(),
    }])
}

fn candidate(service: &str, account: &str, segments: &[&str]) -> Candidate {
    Candidate {
        legacy_service: service.to_string(),
        legacy_account: account.to_string(),
        path: p(segments),
    }
}

const GITHUB_LEGACY: &str = "com.synthesis.github-token";
const AI_API_LEGACY: &str = "com.synthesis.ai-api-provider";
const AGENTIC_LEGACY: &str = "com.synthesis.agentic-integration";

mod adapter;
mod cache;
mod entry;
mod migration;
mod retention;
mod states;
