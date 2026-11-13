//! The operations behind the MCP commands (MCS contract surface).
//!
//! Each operation works on a [`Context`] — the open project, its stores, and
//! the manager — so the tests drive the operations without the Tauri runtime.
//! The commands in `commands.rs` only build a context and call these.

use std::path::PathBuf;

use super::config::{
    list_records, remove_record, save_record, validate, ListedRecord, McpAuth, McpServer,
    McpTransport,
};
use super::connect::{ConnectError, ConnectRequest, McpClient, StderrTail};
use super::manager::McpManager;
use super::secrets::{settable_slots, value_allowed, OAUTH_SLOT};
use super::status::{
    gate, McpServerEntry, McpServerStatus, McpTestResult, McpToolInfo, SlotPresence,
};
use super::{config::trust_fingerprint, facts_for, inputs_for, oauth, project_context};
use crate::global_settings::GlobalSettingsStore;

pub const NO_PROJECT_OPEN: &str = "no_project_open";
pub const NOT_FOUND: &str = "not_found";
pub const UNKNOWN_SLOT: &str = "unknown_slot";
pub const INVALID_SECRET: &str = "invalid_secret";
pub const KEYCHAIN_UNAVAILABLE: &str = "keychain_unavailable";
pub const NO_CREDENTIALS: &str = "no_credentials";
pub const RECORD_CHANGED: &str = "record_changed";
pub const UNTRUSTED: &str = "untrusted";
pub const NOT_OAUTH2: &str = "not_oauth2";
pub const SIGN_IN_IN_PROGRESS: &str = "sign_in_in_progress";

/// The code of an error, for a log record. An error that is not one of this
/// module's codes can hold a path, so it is logged as `error` alone.
pub fn error_code(error: &str) -> &str {
    let known = error.chars().all(|c| c.is_ascii_lowercase() || c == '_') && error.len() <= 40;
    if known {
        error
    } else {
        "error"
    }
}

/// The open project and what the operations need of it.
pub struct Context<'a> {
    pub root: crate::fs::RootFs,
    pub root_path: PathBuf,
    pub key: String,
    pub store: &'a GlobalSettingsStore,
    pub manager: &'a McpManager,
}

impl<'a> Context<'a> {
    pub fn from_states(
        project: &crate::project::ProjectState,
        store: &'a GlobalSettingsStore,
        manager: &'a McpManager,
    ) -> Result<Self, String> {
        let root_path = project.root().ok_or_else(|| NO_PROJECT_OPEN.to_string())?;
        let root = project.require_root().map_err(|_| NO_PROJECT_OPEN.to_string())?;
        Ok(Self { root, root_path, key: project.slot_key(), store, manager })
    }

    fn records(&self) -> Result<Vec<ListedRecord>, String> {
        list_records(&self.root)
    }

    pub fn record(&self, id: &str) -> Result<McpServer, String> {
        self.records()?
            .into_iter()
            .find(|listed| listed.id == id)
            .and_then(|listed| listed.record.ok())
            .ok_or_else(|| NOT_FOUND.to_string())
    }

    /// MCS-FR-RYIK: reconcile the manager with the records as they are now.
    pub fn reconcile(&self, restart: Option<&str>) -> Result<(), String> {
        let records = self.records()?;
        let trust = self.store.load_mcp_trust(&self.key)?;
        let secrets = self.manager.secrets();
        let inputs = inputs_for(records, &self.key, &trust, secrets.as_ref());
        self.manager.reconcile(project_context(&self.key, self.root_path.clone()), inputs, restart);
        Ok(())
    }

    fn entry_of(&self, listed: ListedRecord) -> McpServerEntry {
        let (status, tools) =
            self.manager.snapshot(&listed.id).unwrap_or((McpServerStatus::Disabled, Vec::new()));
        match listed.record {
            Ok(record) => {
                let slots = settable_slots(&record);
                let presence = self
                    .manager
                    .secrets()
                    .presence(&self.key, &record.id, &slots)
                    .unwrap_or_default();
                let secrets = slots
                    .into_iter()
                    .map(|slot| SlotPresence { present: presence.get(&slot) == Some(&true), slot })
                    .collect();
                McpServerEntry {
                    id: listed.id,
                    record: Some(record),
                    invalid: None,
                    status,
                    secrets,
                    tools,
                }
            }
            Err(code) => McpServerEntry {
                id: listed.id,
                record: None,
                invalid: Some(code),
                status: McpServerStatus::Invalid,
                secrets: Vec::new(),
                tools: Vec::new(),
            },
        }
    }

    fn entry(&self, id: &str) -> Result<McpServerEntry, String> {
        self.records()?
            .into_iter()
            .find(|listed| listed.id == id)
            .map(|listed| self.entry_of(listed))
            .ok_or_else(|| NOT_FOUND.to_string())
    }

    /// MCS-FR-FIEE: every record, with its status, its secrets, and its tools.
    /// The list reconciles first, so a record changed outside the application
    /// shows what it is now.
    pub fn list(&self) -> Result<Vec<McpServerEntry>, String> {
        self.reconcile(None)?;
        Ok(self.records()?.into_iter().map(|listed| self.entry_of(listed)).collect())
    }

    /// MCS-FR-XCOS, MCS-FR-OCHR, MCS-FR-EUCA.
    pub fn save(&self, server: McpServer) -> Result<McpServerEntry, String> {
        let server = validate(server).map_err(|code| code.code().to_string())?;
        let previous = self.record(&server.id).ok();
        // MCS-FR-EHBW: a record added under an id that the file does not hold
        // starts with no secret, also when the vault still holds some.
        if !self.records()?.iter().any(|listed| listed.id == server.id) {
            self.manager
                .secrets()
                .remove_server(&self.key, &server.id)
                .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())?;
        }
        save_record(&self.root, &server)?;
        // The manager follows the file also when a later step fails.
        let after = self.after_save(previous.as_ref(), &server);
        self.reconcile(None)?;
        after?;
        self.entry(&server.id)
    }

    /// The vault and trust steps of a save, after the record is written.
    fn after_save(&self, previous: Option<&McpServer>, server: &McpServer) -> Result<(), String> {
        let secrets = self.manager.secrets();
        if let Some(previous) = previous {
            let kept = settable_slots(server);
            for slot in settable_slots(previous).into_iter().filter(|slot| !kept.contains(slot)) {
                secrets
                    .remove(&self.key, &server.id, &slot)
                    .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())?;
            }
            // The tokens of one authorization server are never sent to another.
            let url_of = |s: &McpServer| match &s.transport {
                McpTransport::Http(http) => Some(http.url.clone()),
                McpTransport::Stdio(_) => None,
            };
            let moved = url_of(previous) != url_of(server);
            if previous.auth() == McpAuth::Oauth2 && (server.auth() != McpAuth::Oauth2 || moved) {
                secrets
                    .remove(&self.key, &server.id, OAUTH_SLOT)
                    .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())?;
            }
        }
        // MCS-FR-EUCA: a save trusts what the author typed: a new record, or new
        // trusted values. A save that keeps them keeps the trust state.
        if let Some(fingerprint) = trust_fingerprint(server) {
            if previous.and_then(trust_fingerprint).as_deref() != Some(fingerprint.as_str()) {
                self.store.save_mcp_trust(&self.key, &server.id, Some(&fingerprint))?;
            }
        }
        Ok(())
    }

    /// MCS-FR-UOIO.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        if !remove_record(&self.root, id)? {
            return Err(NOT_FOUND.to_string());
        }
        // The session stops even when the vault or the store fails after the
        // record is gone.
        let secrets = self
            .manager
            .secrets()
            .remove_server(&self.key, id)
            .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string());
        let trust = self.store.save_mcp_trust(&self.key, id, None);
        self.reconcile(None)?;
        secrets?;
        trust
    }

    fn settable(&self, id: &str, slot: &str) -> Result<(), String> {
        let record = self.record(id)?;
        if settable_slots(&record).iter().any(|s| s == slot) {
            Ok(())
        } else {
            Err(UNKNOWN_SLOT.to_string())
        }
    }

    /// MCS-FR-IXHD.
    pub fn set_secret(&self, id: &str, slot: &str, value: &str) -> Result<McpServerEntry, String> {
        self.settable(id, slot)?;
        if !value_allowed(slot, value) {
            return Err(INVALID_SECRET.to_string());
        }
        self.manager
            .secrets()
            .set(&self.key, id, slot, value)
            .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())?;
        self.reconcile(Some(id))?;
        self.entry(id)
    }

    /// MCS-FR-WBDV.
    pub fn clear_secret(&self, id: &str, slot: &str) -> Result<McpServerEntry, String> {
        self.settable(id, slot)?;
        self.manager
            .secrets()
            .remove(&self.key, id, slot)
            .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())?;
        self.reconcile(Some(id))?;
        self.entry(id)
    }

    /// MCS-FR-EVRS: trust the record the dialog showed, when it is the record
    /// in the file now.
    pub fn trust(&self, id: &str, shown: &McpServer) -> Result<McpServerEntry, String> {
        let record = self.record(id)?;
        let current = trust_fingerprint(&record).ok_or_else(|| NO_CREDENTIALS.to_string())?;
        if shown.id != id || trust_fingerprint(shown).as_deref() != Some(current.as_str()) {
            return Err(RECORD_CHANGED.to_string());
        }
        // MCS-FR-LXLB: a credential of another destination is not sent to this one.
        let stored = self.store.load_mcp_trust(&self.key)?.get(id).cloned();
        if matches!(record.transport, McpTransport::Http(_))
            && stored.as_deref() != Some(current.as_str())
        {
            let secrets = self.manager.secrets();
            let mut slots = settable_slots(&record);
            slots.push(OAUTH_SLOT.to_string());
            for slot in &slots {
                secrets
                    .remove(&self.key, id, slot)
                    .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())?;
            }
        }
        self.store.save_mcp_trust(&self.key, id, Some(&current))?;
        self.reconcile(None)?;
        self.entry(id)
    }

    /// MCS-FR-HQJQ: a sign-in reads no secret of an untrusted record.
    pub fn signing_in_record(&self, id: &str) -> Result<McpServer, String> {
        let record = self.record(id)?;
        let trust = self.store.load_mcp_trust(&self.key)?;
        let secrets = self.manager.secrets();
        if !facts_for(&record, &self.key, &trust, secrets.as_ref()).trusted {
            return Err(UNTRUSTED.to_string());
        }
        Ok(record)
    }

    /// MCS-FR-KCIH.
    pub fn reconnect(&self, id: &str) -> Result<(), String> {
        self.record(id)?;
        self.reconcile(Some(id))
    }

    /// MCS-FR-QRIB.
    pub fn sign_out(&self, id: &str) -> Result<McpServerEntry, String> {
        let record = self.record(id)?;
        if record.auth() != McpAuth::Oauth2 {
            return Err(NOT_OAUTH2.to_string());
        }
        let secrets = self.manager.secrets();
        oauth::drop_tokens(secrets.as_ref(), &self.key, id)
            .map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())?;
        self.reconcile(None)?;
        self.entry(id)
    }

    /// The request a test or a sign-in needs, prepared before any await.
    pub fn test_plan(&self, id: &str) -> Result<TestPlan, String> {
        let listed = self
            .records()?
            .into_iter()
            .find(|listed| listed.id == id)
            .ok_or_else(|| NOT_FOUND.to_string())?;
        let record = match listed.record {
            Ok(record) => record,
            Err(_) => return Ok(TestPlan::Refused(McpTestResult::refused("invalid", Vec::new()))),
        };
        let trust = self.store.load_mcp_trust(&self.key)?;
        let secrets = self.manager.secrets();
        let facts = facts_for(&record, &self.key, &trust, secrets.as_ref());
        // MCS-FR-ERQP: a disabled server can be tested.
        let mut enabled = record.clone();
        enabled.enabled = true;
        if let Err(status) = gate(&enabled, &facts) {
            let code = match &status {
                McpServerStatus::Failed { failure } => failure.code(),
                other => other.code(),
            };
            return Ok(TestPlan::Refused(McpTestResult::refused(code, Vec::new())));
        }
        Ok(TestPlan::Run { key: self.key.clone(), root: self.root_path.clone(), record })
    }
}

/// What `test_mcp_server` does after its checks.
pub enum TestPlan {
    Refused(McpTestResult),
    Run { key: String, root: PathBuf, record: McpServer },
}

/// MCS-FR-ETIS, MCS-FR-ERQP: open a new connection, list the tools, and close
/// it. Nothing of the manager's state changes.
pub async fn run_test(manager: &McpManager, plan: TestPlan) -> McpTestResult {
    let (key, root, record) = match plan {
        TestPlan::Refused(result) => return result,
        TestPlan::Run { key, root, record } => (key, root, record),
    };
    let stdio = matches!(record.transport, McpTransport::Stdio(_));
    let stderr = StderrTail::default();
    let (client, _events) = McpClient::new();
    let disabled = record.disabled_tools.clone();
    let request = ConnectRequest {
        project_key: key,
        root,
        server: record,
        secrets: manager.secrets(),
        client,
        stderr: stderr.clone(),
    };
    let connector = manager.connector();
    match McpManager::open(connector.as_ref(), request, manager.connect_limit()).await {
        Ok((session, tools)) => {
            let _ = session.cancel().await;
            McpTestResult::passed(
                tools
                    .iter()
                    .map(|tool| McpToolInfo {
                        name: tool.name.to_string(),
                        description: tool.description.as_deref().unwrap_or_default().to_string(),
                        disabled: disabled.iter().any(|d| d == tool.name.as_ref()),
                    })
                    .collect(),
            )
        }
        Err(error) => {
            let code = match error {
                ConnectError::Failure(failure) => failure.code(),
                ConnectError::NeedsSecret => "needs_secret",
                ConnectError::NeedsSignIn => "needs_sign_in",
            };
            let lines = if stdio {
                // The reader of standard error may still hold the last lines.
                stderr.wait_closed(std::time::Duration::from_secs(1)).await;
                stderr.lines()
            } else {
                Vec::new()
            };
            McpTestResult::refused(code, lines)
        }
    }
}
