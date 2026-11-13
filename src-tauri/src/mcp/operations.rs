//! The operations behind the MCP commands (MCS contract surface).
//!
//! Each operation works on a [`Context`] — the open project, its stores, and
//! the manager — so the tests drive the operations without the Tauri runtime.
//! The commands in `commands.rs` only build a context and call these.

use std::path::PathBuf;

use super::config::{validate, McpAuth, McpServer, McpTransport};
use super::connect::{ConnectError, ConnectRequest, McpClient, StderrTail};
use super::manager::McpManager;
use super::scope::{active, enabled_here, McpScope, ScopedRecord, Stores};
use super::secrets::{settable_slots, value_allowed, SecretOwner, OAUTH_SLOT};
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
pub const TRUST_NOT_NEEDED: &str = "trust_not_needed";
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

fn keychain<T, E>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|_| KEYCHAIN_UNAVAILABLE.to_string())
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

    /// A handled error, logged with its code alone (MCS-FR-MABN).
    fn warn(&self, message: &str, id: &str, error: &str) {
        self.manager.log(
            crate::logging::LogLevel::Warn,
            message,
            crate::log_fields! { "server" => id, "error" => error_code(error) },
        );
    }

    fn stores(&self) -> Stores<'_> {
        Stores { root: &self.root, store: self.store, key: &self.key }
    }

    /// MCS-FR-FIEE, MCS-FR-PQHI: the records of every scope.
    pub fn records(&self) -> Result<Vec<ScopedRecord>, String> {
        self.stores().all()
    }

    /// Where the secrets of a server of `scope` are.
    pub fn owner(&self, scope: McpScope) -> SecretOwner {
        SecretOwner::of(scope, &self.key)
    }

    /// MCS-FR-EBJL: the active record of an id, when it is valid.
    pub fn active(&self, id: &str) -> Result<(McpScope, McpServer), String> {
        let records = self.records()?;
        active(&records, id)
            .and_then(|scoped| scoped.record().map(|record| (scoped.scope, record.clone())))
            .ok_or_else(|| NOT_FOUND.to_string())
    }

    pub fn record(&self, id: &str) -> Result<McpServer, String> {
        self.active(id).map(|(_, record)| record)
    }

    /// MCS-FR-RYIK: reconcile the manager with the records as they are now.
    pub fn reconcile(&self, restart: Option<&str>) -> Result<(), String> {
        let records = self.records()?;
        let switches = self.stores().switches()?;
        let trust = self.store.load_mcp_trust(&self.key)?;
        let secrets = self.manager.secrets();
        let inputs = inputs_for(&records, &self.key, &switches, &trust, secrets.as_ref());
        self.manager.reconcile(project_context(&self.key, self.root_path.clone()), inputs, restart);
        Ok(())
    }

    fn entry_of(&self, scoped: &ScopedRecord, records: &[ScopedRecord]) -> McpServerEntry {
        let switches = self.stores().switches().unwrap_or_else(|error| {
            self.warn("mcp switches not read", &scoped.listed.id, &error);
            Default::default()
        });
        let enabled_here = enabled_here(scoped, &switches);
        let is_active = active(records, &scoped.listed.id).map(|a| a.scope) == Some(scoped.scope);
        match (&scoped.listed.record, is_active) {
            (Ok(record), true) => {
                let (status, tools) = self
                    .manager
                    .snapshot(&scoped.listed.id)
                    .unwrap_or((McpServerStatus::Disabled, Vec::new()));
                let slots = settable_slots(record);
                let presence = self
                    .manager
                    .secrets()
                    .presence(&self.owner(scoped.scope), &record.id, &slots)
                    .unwrap_or_else(|_| {
                        self.warn("mcp secret presence not read", &record.id, KEYCHAIN_UNAVAILABLE);
                        Default::default()
                    });
                let secrets = slots
                    .into_iter()
                    .map(|slot| SlotPresence { present: presence.get(&slot) == Some(&true), slot })
                    .collect();
                McpServerEntry {
                    id: scoped.listed.id.clone(),
                    scope: scoped.scope,
                    enabled_here,
                    record: Some(record.clone()),
                    invalid: None,
                    status,
                    secrets,
                    tools,
                }
            }
            (record, _) => McpServerEntry {
                id: scoped.listed.id.clone(),
                scope: scoped.scope,
                enabled_here: false,
                record: None,
                invalid: Some(
                    record
                        .as_ref()
                        .err()
                        .copied()
                        .unwrap_or(super::config::McpConfigError::DuplicateId),
                ),
                status: McpServerStatus::Invalid,
                secrets: Vec::new(),
                tools: Vec::new(),
            },
        }
    }

    fn entry(&self, id: &str) -> Result<McpServerEntry, String> {
        let records = self.records()?;
        active(&records, id)
            .map(|scoped| self.entry_of(scoped, &records))
            .ok_or_else(|| NOT_FOUND.to_string())
    }

    /// MCS-FR-FIEE: every record, with its status, its secrets, and its tools.
    /// The list reconciles first, so a record changed outside the application
    /// shows what it is now.
    pub fn list(&self) -> Result<Vec<McpServerEntry>, String> {
        self.reconcile(None)?;
        let records = self.records()?;
        Ok(records.iter().map(|scoped| self.entry_of(scoped, &records)).collect())
    }

    /// MCS-FR-XCOS, MCS-FR-HWNC, MCS-FR-OCHR, MCS-FR-EUCA.
    pub fn save(&self, server: McpServer, scope: McpScope) -> Result<McpServerEntry, String> {
        let server = validate(server).map_err(|code| code.code().to_string())?;
        let records = self.records()?;
        let same = |r: &&ScopedRecord| r.listed.id == server.id && r.scope == scope;
        // MCS-FR-HWNC: a record that its scope does not hold yet takes no id of
        // another scope. A record that its scope holds can always be saved.
        let held = records.iter().any(|r| same(&r));
        if !held && records.iter().any(|r| r.listed.id == server.id) {
            return Err(super::config::McpConfigError::DuplicateId.code().to_string());
        }
        let previous = records.iter().find(same).and_then(|r| r.record().cloned());
        let owner = self.owner(scope);
        // MCS-FR-EHBW: a record added under an id that its scope does not hold
        // starts with no secret, also when the vault still holds some.
        if !held {
            keychain(self.manager.secrets().remove_server(&owner, &server.id))?;
        }
        self.stores().save(scope, &server)?;
        // The manager follows the stores also when a later step fails.
        let after = self.after_save(previous.as_ref(), &server, scope);
        self.reconcile(None)?;
        after?;
        self.entry(&server.id)
    }

    /// The vault and trust steps of a save, after the record is written.
    fn after_save(
        &self,
        previous: Option<&McpServer>,
        server: &McpServer,
        scope: McpScope,
    ) -> Result<(), String> {
        let secrets = self.manager.secrets();
        let owner = self.owner(scope);
        if let Some(previous) = previous {
            let kept = settable_slots(server);
            for slot in settable_slots(previous).into_iter().filter(|slot| !kept.contains(slot)) {
                keychain(secrets.remove(&owner, &server.id, &slot))?;
            }
            // The tokens of one authorization server are never sent to another.
            let url_of = |s: &McpServer| match &s.transport {
                McpTransport::Http(http) => Some(http.url.clone()),
                McpTransport::Stdio(_) => None,
            };
            let moved = url_of(previous) != url_of(server);
            if previous.auth() == McpAuth::Oauth2 && (server.auth() != McpAuth::Oauth2 || moved) {
                keychain(secrets.remove(&owner, &server.id, OAUTH_SLOT))?;
            }
        }
        // MCS-FR-EUCA: a save trusts what the author typed: a new record, or new
        // trusted values. A save that keeps them keeps the trust state. A
        // personal record needs no trust (MCS-FR-MUVC).
        if scope != McpScope::Team {
            return Ok(());
        }
        if let Some(fingerprint) = trust_fingerprint(server) {
            if previous.and_then(trust_fingerprint).as_deref() != Some(fingerprint.as_str()) {
                self.store.save_mcp_trust(&self.key, &server.id, Some(&fingerprint))?;
            }
        }
        Ok(())
    }

    /// MCS-FR-UOIO.
    pub fn remove(&self, id: &str, scope: McpScope) -> Result<(), String> {
        if !self.stores().remove(scope, id)? {
            return Err(NOT_FOUND.to_string());
        }
        // The session stops even when the vault or the store fails after the
        // record is gone.
        let secrets = keychain(self.manager.secrets().remove_server(&self.owner(scope), id));
        let local = if scope == McpScope::Team {
            self.store
                .save_mcp_trust(&self.key, id, None)
                .and_then(|()| self.store.save_mcp_switch(&self.key, id, None))
        } else {
            Ok(())
        };
        self.reconcile(None)?;
        secrets?;
        local
    }

    /// MCS-FR-MKDH: turn a server on or off on this computer.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<McpServerEntry, String> {
        let (scope, mut record) = self.active(id)?;
        if scope == McpScope::Team {
            let switch = (enabled != record.enabled).then_some(enabled);
            self.store.save_mcp_switch(&self.key, id, switch)?;
        } else {
            record.enabled = enabled;
            self.stores().save(scope, &record)?;
        }
        self.reconcile(None)?;
        self.entry(id)
    }

    fn settable(&self, id: &str, slot: &str) -> Result<SecretOwner, String> {
        let (scope, record) = self.active(id)?;
        if settable_slots(&record).iter().any(|s| s == slot) {
            Ok(self.owner(scope))
        } else {
            Err(UNKNOWN_SLOT.to_string())
        }
    }

    /// MCS-FR-IXHD.
    pub fn set_secret(&self, id: &str, slot: &str, value: &str) -> Result<McpServerEntry, String> {
        let owner = self.settable(id, slot)?;
        if !value_allowed(slot, value) {
            return Err(INVALID_SECRET.to_string());
        }
        keychain(self.manager.secrets().set(&owner, id, slot, value))?;
        self.reconcile(Some(id))?;
        self.entry(id)
    }

    /// MCS-FR-WBDV.
    pub fn clear_secret(&self, id: &str, slot: &str) -> Result<McpServerEntry, String> {
        let owner = self.settable(id, slot)?;
        keychain(self.manager.secrets().remove(&owner, id, slot))?;
        self.reconcile(Some(id))?;
        self.entry(id)
    }

    /// MCS-FR-EVRS: trust the record the dialog showed, when it is the record
    /// in the file now.
    pub fn trust(&self, id: &str, shown: &McpServer) -> Result<McpServerEntry, String> {
        let (scope, record) = self.active(id)?;
        if scope != McpScope::Team {
            return Err(TRUST_NOT_NEEDED.to_string());
        }
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
            let owner = self.owner(scope);
            let mut slots = settable_slots(&record);
            slots.push(OAUTH_SLOT.to_string());
            for slot in &slots {
                keychain(secrets.remove(&owner, id, slot))?;
            }
        }
        self.store.save_mcp_trust(&self.key, id, Some(&current))?;
        self.reconcile(None)?;
        self.entry(id)
    }

    /// MCS-FR-HQJQ: a sign-in reads no secret of an untrusted record.
    pub fn signing_in_record(&self, id: &str) -> Result<(SecretOwner, McpServer), String> {
        let (scope, mut record) = self.active(id)?;
        let trust = self.store.load_mcp_trust(&self.key)?;
        let secrets = self.manager.secrets();
        let owner = self.owner(scope);
        // The sign-in is an act of the author, so it is open also for a server
        // that is off on this computer.
        record.enabled = true;
        if !facts_for(&record, owner.clone(), &trust, secrets.as_ref()).trusted {
            return Err(UNTRUSTED.to_string());
        }
        Ok((owner, record))
    }

    /// MCS-FR-KCIH.
    pub fn reconnect(&self, id: &str) -> Result<(), String> {
        self.record(id)?;
        self.reconcile(Some(id))
    }

    /// MCS-FR-QRIB.
    pub fn sign_out(&self, id: &str) -> Result<McpServerEntry, String> {
        let (scope, record) = self.active(id)?;
        if record.auth() != McpAuth::Oauth2 {
            return Err(NOT_OAUTH2.to_string());
        }
        let secrets = self.manager.secrets();
        keychain(oauth::drop_tokens(secrets.as_ref(), &self.owner(scope), id))?;
        self.reconcile(None)?;
        self.entry(id)
    }

    /// The request a test or a sign-in needs, prepared before any await.
    pub fn test_plan(&self, id: &str) -> Result<TestPlan, String> {
        let records = self.records()?;
        let scoped = active(&records, id).ok_or_else(|| NOT_FOUND.to_string())?;
        let Some(record) = scoped.record().cloned() else {
            return Ok(TestPlan::Refused(McpTestResult::refused("invalid", Vec::new())));
        };
        let trust = self.store.load_mcp_trust(&self.key)?;
        let secrets = self.manager.secrets();
        let owner = self.owner(scoped.scope);
        // MCS-FR-ERQP: a server that is off can be tested.
        let mut enabled = record.clone();
        enabled.enabled = true;
        let facts = facts_for(&enabled, owner.clone(), &trust, secrets.as_ref());
        if let Err(status) = gate(&enabled, &facts) {
            let code = match &status {
                McpServerStatus::Failed { failure } => failure.code(),
                other => other.code(),
            };
            return Ok(TestPlan::Refused(McpTestResult::refused(code, Vec::new())));
        }
        Ok(TestPlan::Run { owner, root: self.root_path.clone(), record: Box::new(record) })
    }
}

/// MCS-FR-ZCPK: the trust fingerprint of the active record of `id` now, from
/// the stores alone. A sign-in compares it with the one it started with.
pub fn current_fingerprint(
    root: &crate::fs::RootFs,
    store: &GlobalSettingsStore,
    key: &str,
    id: &str,
) -> Option<Option<String>> {
    let records = Stores { root, store, key }.all().ok()?;
    let scoped = active(&records, id)?;
    scoped.record().map(trust_fingerprint)
}

/// What `test_mcp_server` does after its checks.
pub enum TestPlan {
    Refused(McpTestResult),
    Run { owner: SecretOwner, root: PathBuf, record: Box<McpServer> },
}

/// MCS-FR-ETIS, MCS-FR-ERQP: open a new connection, list the tools, and close
/// it. Nothing of the manager's state changes.
pub async fn run_test(manager: &McpManager, plan: TestPlan) -> McpTestResult {
    let (owner, root, record) = match plan {
        TestPlan::Refused(result) => return result,
        TestPlan::Run { owner, root, record } => (owner, root, *record),
    };
    let stdio = matches!(record.transport, McpTransport::Stdio(_));
    let stderr = StderrTail::default();
    let (client, _events) = McpClient::new();
    let disabled = record.disabled_tools.clone();
    let request = ConnectRequest {
        owner,
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
