//! The adapter the owning modules hold (`specifications/core/ASV-application-secret-vault.md`
//! ASV-FR-04, ASV-FR-31).

use std::collections::HashMap;
use std::sync::Arc;

use super::{Mutation, SecretPath, Vault, VaultError};

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
