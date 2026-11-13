//! The secrets of the MCP servers of a project (MCS-FR-SBND to MCS-FR-PXRJ).
//!
//! A secret is stored in the vault at
//! `["projects", <project key>, "mcp", <server id>, <slot>]` (ASV-FR-FFKB). No
//! value leaves this module except to the connection that sends it, and no
//! value is logged (MCS-FR-MABN).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::config::{McpAuth, McpServer, McpTransport};

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

/// The vault, as this module uses it. A seam so the tests run without the
/// operating system keychain.
pub trait McpSecrets: Send + Sync {
    fn get(
        &self,
        project: &str,
        server: &str,
        slot: &str,
    ) -> Result<Option<String>, SecretsUnavailable>;
    fn set(
        &self,
        project: &str,
        server: &str,
        slot: &str,
        value: &str,
    ) -> Result<(), SecretsUnavailable>;
    fn remove(&self, project: &str, server: &str, slot: &str) -> Result<(), SecretsUnavailable>;
    /// MCS-FR-UOIO: remove every secret of one server.
    fn remove_server(&self, project: &str, server: &str) -> Result<(), SecretsUnavailable>;
    fn presence(
        &self,
        project: &str,
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

    fn path(project: &str, server: &str, slot: Option<&str>) -> crate::secret_vault::SecretPath {
        let mut path = vec![
            "projects".to_string(),
            project.to_string(),
            "mcp".to_string(),
            server.to_string(),
        ];
        if let Some(slot) = slot {
            path.push(slot.to_string());
        }
        path
    }
}

impl McpSecrets for VaultMcpSecrets {
    fn get(
        &self,
        project: &str,
        server: &str,
        slot: &str,
    ) -> Result<Option<String>, SecretsUnavailable> {
        self.vault
            .read_secret(&Self::path(project, server, Some(slot)))
            .map_err(|_| SecretsUnavailable)
    }

    fn set(
        &self,
        project: &str,
        server: &str,
        slot: &str,
        value: &str,
    ) -> Result<(), SecretsUnavailable> {
        self.vault
            .apply_secret_mutations(&[crate::secret_vault::Mutation::Set {
                path: Self::path(project, server, Some(slot)),
                secret: value.to_string(),
            }])
            .map_err(|_| SecretsUnavailable)
    }

    fn remove(&self, project: &str, server: &str, slot: &str) -> Result<(), SecretsUnavailable> {
        self.vault
            .apply_secret_mutations(&[crate::secret_vault::Mutation::Remove {
                path: Self::path(project, server, Some(slot)),
            }])
            .map_err(|_| SecretsUnavailable)
    }

    fn remove_server(&self, project: &str, server: &str) -> Result<(), SecretsUnavailable> {
        self.vault
            .apply_secret_mutations(&[crate::secret_vault::Mutation::Remove {
                path: Self::path(project, server, None),
            }])
            .map_err(|_| SecretsUnavailable)
    }

    fn presence(
        &self,
        project: &str,
        server: &str,
        slots: &[String],
    ) -> Result<HashMap<String, bool>, SecretsUnavailable> {
        let paths: Vec<_> =
            slots.iter().map(|slot| Self::path(project, server, Some(slot))).collect();
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
    values: Mutex<HashMap<(String, String, String), String>>,
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

    fn key(project: &str, server: &str, slot: &str) -> (String, String, String) {
        (project.to_string(), server.to_string(), slot.to_string())
    }
}

impl McpSecrets for MemoryMcpSecrets {
    fn get(
        &self,
        project: &str,
        server: &str,
        slot: &str,
    ) -> Result<Option<String>, SecretsUnavailable> {
        self.check()?;
        let values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        Ok(values.get(&Self::key(project, server, slot)).cloned())
    }

    fn set(
        &self,
        project: &str,
        server: &str,
        slot: &str,
        value: &str,
    ) -> Result<(), SecretsUnavailable> {
        self.check()?;
        let mut values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        values.insert(Self::key(project, server, slot), value.to_string());
        Ok(())
    }

    fn remove(&self, project: &str, server: &str, slot: &str) -> Result<(), SecretsUnavailable> {
        self.check()?;
        let mut values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        values.remove(&Self::key(project, server, slot));
        Ok(())
    }

    fn remove_server(&self, project: &str, server: &str) -> Result<(), SecretsUnavailable> {
        self.check()?;
        let mut values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        values.retain(|(p, s, _), _| !(p == project && s == server));
        Ok(())
    }

    fn presence(
        &self,
        project: &str,
        server: &str,
        slots: &[String],
    ) -> Result<HashMap<String, bool>, SecretsUnavailable> {
        self.check()?;
        let values = self.values.lock().map_err(|_| SecretsUnavailable)?;
        Ok(slots
            .iter()
            .map(|slot| (slot.clone(), values.contains_key(&Self::key(project, server, slot))))
            .collect())
    }
}
