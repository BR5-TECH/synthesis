//! The secrets of the MCP servers of a project (MCS-FR-SBND to MCS-FR-PXRJ).
//!
//! A secret of a team server is stored in the vault at
//! `["projects", <project key>, "mcp", <server id>, <slot>]`, one of a personal
//! server of the project at `["projects", <project key>, "personal_mcp", ...]`,
//! and one of a personal server of all projects at
//! `["mcp", "servers", <server id>, <slot>]` (MCS-FR-SBND, MCS-FR-VHBK,
//! ASV-FR-FFKB, ASV-FR-FVSH). No value leaves this module except to the
//! connection that sends it, and no value is logged (MCS-FR-MABN).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::config::{McpAuth, McpServer, McpTransport};
use super::scope::McpScope;

/// MCS-FR-MGCJ: the slot that holds the OAuth sign-in state. It is not
/// settable through `set_mcp_server_secret`.
pub const OAUTH_SLOT: &str = "oauth";
/// MCS-FR-QBFA: the slot of the bearer token.
pub const BEARER_SLOT: &str = "bearer";
/// MCS-FR-QBFA: the slot of the client secret of a pre-registered client.
pub const OAUTH_CLIENT_SECRET_SLOT: &str = "oauth_client_secret";

/// The vault could not be read or written. Rendered as `keychain_unavailable`
/// (ASV-FR-31). It holds no path, id, or value (ASV-FR-28).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretsUnavailable;

/// Where the secrets of one server are: the scope of the server and, for the
/// scopes of one project, the project key (MCS-FR-SBND, MCS-FR-VHBK).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SecretOwner {
    /// A team server of the project with this key.
    Team(String),
    /// A personal server of the project with this key.
    Project(String),
    /// A personal server of every project.
    All,
}

impl SecretOwner {
    /// The owner of the secrets of a server of `scope` in the project `key`.
    pub fn of(scope: McpScope, key: &str) -> Self {
        match scope {
            McpScope::Team => SecretOwner::Team(key.to_string()),
            McpScope::PersonalProject => SecretOwner::Project(key.to_string()),
            McpScope::PersonalAll => SecretOwner::All,
        }
    }

    /// The scope of the servers of this owner.
    pub fn scope(&self) -> McpScope {
        match self {
            SecretOwner::Team(_) => McpScope::Team,
            SecretOwner::Project(_) => McpScope::PersonalProject,
            SecretOwner::All => McpScope::PersonalAll,
        }
    }

    /// The vault path of the objects of the servers of this owner.
    fn prefix(&self) -> Vec<String> {
        match self {
            SecretOwner::Team(key) => vec!["projects".into(), key.clone(), "mcp".into()],
            SecretOwner::Project(key) => {
                vec!["projects".into(), key.clone(), "personal_mcp".into()]
            }
            SecretOwner::All => vec!["mcp".into(), "servers".into()],
        }
    }
}

/// The team servers of a project, from its key. A short form for the callers
/// and the tests that work with team servers alone.
impl From<&str> for SecretOwner {
    fn from(key: &str) -> Self {
        SecretOwner::Team(key.to_string())
    }
}

/// The vault, as this module uses it. A seam so the tests run without the
/// operating system keychain.
pub trait McpSecrets: Send + Sync {
    fn get(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
    ) -> Result<Option<String>, SecretsUnavailable>;
    fn set(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
        value: &str,
    ) -> Result<(), SecretsUnavailable>;
    fn remove(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
    ) -> Result<(), SecretsUnavailable>;
    /// MCS-FR-UOIO: remove every secret of one server.
    fn remove_server(&self, owner: &SecretOwner, server: &str) -> Result<(), SecretsUnavailable>;
    fn presence(
        &self,
        owner: &SecretOwner,
        server: &str,
        slots: &[String],
    ) -> Result<HashMap<String, bool>, SecretsUnavailable>;
}

/// The env slot of one variable name.
pub fn env_slot(name: &str) -> String {
    format!("env:{name}")
}

/// The header slot of one header name.
pub fn header_slot(name: &str) -> String {
    format!("header:{name}")
}

/// MCS-FR-QBFA: the settable slots of a record, in a stable order.
pub fn settable_slots(server: &McpServer) -> Vec<String> {
    let mut slots = Vec::new();
    match &server.transport {
        McpTransport::Stdio(stdio) => {
            slots.extend(stdio.secret_env.iter().map(|name| env_slot(name)));
        }
        McpTransport::Http(http) => {
            slots.extend(http.secret_headers.iter().map(|name| header_slot(name)));
            if http.auth == McpAuth::Bearer {
                slots.push(BEARER_SLOT.to_string());
            }
            if http.auth == McpAuth::Oauth2 && http.client_id.is_some() {
                slots.push(OAUTH_CLIENT_SECRET_SLOT.to_string());
            }
        }
    }
    slots
}

/// MCS-FR-PXRJ: the slots that must hold a value before the server connects.
/// The client secret is optional.
pub fn required_slots(server: &McpServer) -> Vec<String> {
    settable_slots(server).into_iter().filter(|slot| slot != OAUTH_CLIENT_SECRET_SLOT).collect()
}

/// MCS-FR-IXHD: whether a value may be stored in a slot. A header value and a
/// bearer token cannot hold a line break.
pub fn value_allowed(slot: &str, value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let one_line = !value.contains(['\r', '\n']);
    if slot == BEARER_SLOT || slot.starts_with("header:") {
        one_line
    } else {
        true
    }
}

/// The production store: the vault of this process.
pub struct VaultMcpSecrets {
    vault: Arc<crate::secret_vault::Vault>,
}

impl VaultMcpSecrets {
    pub fn new(vault: Arc<crate::secret_vault::Vault>) -> Self {
        Self { vault }
    }

    fn path(
        owner: &SecretOwner,
        server: &str,
        slot: Option<&str>,
    ) -> crate::secret_vault::SecretPath {
        let mut path = owner.prefix();
        path.push(server.to_string());
        if let Some(slot) = slot {
            path.push(slot.to_string());
        }
        path
    }
}

impl McpSecrets for VaultMcpSecrets {
    fn get(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
    ) -> Result<Option<String>, SecretsUnavailable> {
        self.vault
            .read_secret(&Self::path(owner, server, Some(slot)))
            .map_err(|_| SecretsUnavailable)
    }

    fn set(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
        value: &str,
    ) -> Result<(), SecretsUnavailable> {
        self.vault
            .apply_secret_mutations(&[crate::secret_vault::Mutation::Set {
                path: Self::path(owner, server, Some(slot)),
                secret: value.to_string(),
            }])
            .map_err(|_| SecretsUnavailable)
    }

    fn remove(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
    ) -> Result<(), SecretsUnavailable> {
        self.vault
            .apply_secret_mutations(&[crate::secret_vault::Mutation::Remove {
                path: Self::path(owner, server, Some(slot)),
            }])
            .map_err(|_| SecretsUnavailable)
    }

    fn remove_server(&self, owner: &SecretOwner, server: &str) -> Result<(), SecretsUnavailable> {
        self.vault
            .apply_secret_mutations(&[crate::secret_vault::Mutation::Remove {
                path: Self::path(owner, server, None),
            }])
            .map_err(|_| SecretsUnavailable)
    }

    fn presence(
        &self,
        owner: &SecretOwner,
        server: &str,
        slots: &[String],
    ) -> Result<HashMap<String, bool>, SecretsUnavailable> {
        let paths: Vec<_> =
            slots.iter().map(|slot| Self::path(owner, server, Some(slot))).collect();
        let answers = self.vault.secret_presence(&paths).map_err(|_| SecretsUnavailable)?;
        Ok(slots
            .iter()
            .zip(paths.iter())
            .map(|(slot, path)| (slot.clone(), answers.get(path).copied().unwrap_or(false)))
            .collect())
    }
}

/// An in-memory store, for the tests of this module and of its callers.
#[derive(Default)]
pub struct MemoryMcpSecrets {
    values: Mutex<HashMap<(SecretOwner, String, String), String>>,
    /// When set, every operation fails as an unavailable keychain does.
    pub unavailable: std::sync::atomic::AtomicBool,
}

impl MemoryMcpSecrets {
    fn check(&self) -> Result<(), SecretsUnavailable> {
        if self.unavailable.load(std::sync::atomic::Ordering::SeqCst) {
            Err(SecretsUnavailable)
        } else {
            Ok(())
        }
    }

    fn key(owner: &SecretOwner, server: &str, slot: &str) -> (SecretOwner, String, String) {
        (owner.clone(), server.to_string(), slot.to_string())
    }
}

impl McpSecrets for MemoryMcpSecrets {
    fn get(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
    ) -> Result<Option<String>, SecretsUnavailable> {
        self.check()?;
        let values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        Ok(values.get(&Self::key(owner, server, slot)).cloned())
    }

    fn set(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
        value: &str,
    ) -> Result<(), SecretsUnavailable> {
        self.check()?;
        let mut values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        values.insert(Self::key(owner, server, slot), value.to_string());
        Ok(())
    }

    fn remove(
        &self,
        owner: &SecretOwner,
        server: &str,
        slot: &str,
    ) -> Result<(), SecretsUnavailable> {
        self.check()?;
        let mut values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        values.remove(&Self::key(owner, server, slot));
        Ok(())
    }

    fn remove_server(&self, owner: &SecretOwner, server: &str) -> Result<(), SecretsUnavailable> {
        self.check()?;
        let mut values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        values.retain(|(o, s, _), _| !(o == owner && s == server));
        Ok(())
    }

    fn presence(
        &self,
        owner: &SecretOwner,
        server: &str,
        slots: &[String],
    ) -> Result<HashMap<String, bool>, SecretsUnavailable> {
        self.check()?;
        let values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        Ok(slots
            .iter()
            .map(|slot| (slot.clone(), values.contains_key(&Self::key(owner, server, slot))))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(owner: &SecretOwner) -> Vec<String> {
        VaultMcpSecrets::path(owner, "docs", Some("bearer"))
    }

    // MCS-FR-SBND, MCS-FR-VHBK, ASV-FR-FFKB, ASV-FR-FVSH: the vault path of a
    // secret of each scope.
    #[test]
    fn each_scope_has_its_vault_path() {
        assert_eq!(
            path(&SecretOwner::Team("k".into())),
            ["projects", "k", "mcp", "docs", "bearer"]
        );
        assert_eq!(
            path(&SecretOwner::Project("k".into())),
            ["projects", "k", "personal_mcp", "docs", "bearer"]
        );
        assert_eq!(path(&SecretOwner::All), ["mcp", "servers", "docs", "bearer"]);
        assert_eq!(
            VaultMcpSecrets::path(&SecretOwner::All, "docs", None),
            ["mcp", "servers", "docs"]
        );
        assert_eq!(SecretOwner::of(McpScope::PersonalAll, "k").scope(), McpScope::PersonalAll);
        assert_eq!(SecretOwner::from("k"), SecretOwner::Team("k".into()));
    }
}
