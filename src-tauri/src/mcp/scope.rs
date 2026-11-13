//! The scopes of the MCP servers (MCS-FR-ZSFL, MCS-FR-OEDO, MCS-FR-PQHI).
//!
//! A `team` record is in `.synthesis/project.toml`. A `personal_project`
//! record is in the project's user-global slot, and a `personal_all` record is
//! in the user-global store. This module reads the three lists as one list and
//! writes one record into the list of its scope.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use super::config::{
    list_records, records_of_tables, remove_record, save_record, to_toml, ListedRecord,
    McpConfigError, McpServer,
};
use crate::global_settings::GlobalSettingsStore;

/// MCS-FR-ZSFL: where a record is stored and who sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpScope {
    /// A record of `.synthesis/project.toml`, shared with the team.
    #[default]
    Team,
    /// A personal record of this project on this computer.
    PersonalProject,
    /// A personal record of every project on this computer.
    PersonalAll,
}

impl McpScope {
    pub fn code(self) -> &'static str {
        match self {
            McpScope::Team => "team",
            McpScope::PersonalProject => "personal_project",
            McpScope::PersonalAll => "personal_all",
        }
    }
}

/// One listed record and its scope.
#[derive(Clone, Debug)]
pub struct ScopedRecord {
    pub scope: McpScope,
    pub listed: ListedRecord,
}

impl ScopedRecord {
    /// The valid record, when there is one.
    pub fn record(&self) -> Option<&McpServer> {
        self.listed.record.as_ref().ok()
    }
}

/// Where the records of the three scopes are.
pub struct Stores<'a> {
    pub root: &'a crate::fs::RootFs,
    pub store: &'a GlobalSettingsStore,
    pub key: &'a str,
}

impl Stores<'_> {
    fn tables(&self, scope: McpScope) -> Result<Vec<toml::Table>, String> {
        match scope {
            McpScope::Team => unreachable_team(),
            McpScope::PersonalProject => self.store.load_personal_mcp_servers(self.key),
            McpScope::PersonalAll => self.store.load_global_mcp_servers(),
        }
    }

    /// Change the list of a personal scope under one lock of the store, so
    /// two writes at the same time do not lose one of them.
    fn update_tables<R>(
        &self,
        scope: McpScope,
        apply: impl FnOnce(&mut Vec<toml::Table>) -> R,
    ) -> Result<R, String> {
        match scope {
            McpScope::Team => unreachable_team(),
            McpScope::PersonalProject => self.store.update_personal_mcp_servers(self.key, apply),
            McpScope::PersonalAll => self.store.update_global_mcp_servers(apply),
        }
    }

    /// The records of one scope, in the order of their list.
    fn records_of(&self, scope: McpScope) -> Result<Vec<ListedRecord>, String> {
        match scope {
            McpScope::Team => list_records(self.root),
            other => Ok(records_of_tables(&self.tables(other)?)),
        }
    }

    /// MCS-FR-FIEE, MCS-FR-PQHI: the records of every scope, team first. A
    /// record whose id an earlier scope already holds is `duplicate_id`.
    pub fn all(&self) -> Result<Vec<ScopedRecord>, String> {
        let mut out = Vec::new();
        let mut taken: HashSet<String> = HashSet::new();
        for scope in [McpScope::Team, McpScope::PersonalProject, McpScope::PersonalAll] {
            let listed = self.records_of(scope)?;
            let ids: Vec<String> = listed.iter().map(|l| l.id.clone()).collect();
            for mut record in listed {
                if !record.id.is_empty() && taken.contains(&record.id) {
                    record.record = Err(McpConfigError::DuplicateId);
                }
                out.push(ScopedRecord { scope, listed: record });
            }
            taken.extend(ids.into_iter().filter(|id| !id.is_empty()));
        }
        Ok(out)
    }

    /// MCS-FR-XCOS, MCS-FR-JZDF: write one record into the list of its scope.
    pub fn save(&self, scope: McpScope, server: &McpServer) -> Result<(), String> {
        if scope == McpScope::Team {
            return save_record(self.root, server);
        }
        let incoming = to_toml(server);
        self.update_tables(scope, |tables| {
            match tables.iter().position(|t| id_of(t) == Some(server.id.as_str())) {
                Some(index) => tables[index] = incoming,
                None => tables.push(incoming),
            }
        })
    }

    /// MCS-FR-UOIO: remove every record with this id from the list of its
    /// scope. `Ok(false)` when there is none.
    pub fn remove(&self, scope: McpScope, id: &str) -> Result<bool, String> {
        if scope == McpScope::Team {
            return remove_record(self.root, id);
        }
        self.update_tables(scope, |tables| {
            let before = tables.len();
            tables.retain(|t| id_of(t) != Some(id));
            tables.len() != before
        })
    }

    /// GSS-FR-KNUB: the switches of the team servers.
    pub fn switches(&self) -> Result<BTreeMap<String, bool>, String> {
        self.store.load_mcp_switches(self.key)
    }
}

fn id_of(table: &toml::Table) -> Option<&str> {
    table.get("id").and_then(toml::Value::as_str)
}

/// The team list is the project file, which `list_records`, `save_record`, and
/// `remove_record` own.
fn unreachable_team<T>() -> Result<T, String> {
    Err("the team list is the project file".to_string())
}

/// MCS-FR-VKKF: whether a server is on on this computer. A team record follows
/// its switch, and its `enabled` when no switch is stored. A personal record
/// follows its `enabled`. An invalid record is off.
pub fn enabled_here(record: &ScopedRecord, switches: &BTreeMap<String, bool>) -> bool {
    match (record.scope, record.record()) {
        (_, None) => false,
        (McpScope::Team, Some(server)) => {
            switches.get(&server.id).copied().unwrap_or(server.enabled)
        }
        (_, Some(server)) => server.enabled,
    }
}

/// MCS-FR-PQHI: the active record of an id is the first listed record of it.
pub fn active<'a>(records: &'a [ScopedRecord], id: &str) -> Option<&'a ScopedRecord> {
    records.iter().find(|record| record.listed.id == id)
}
