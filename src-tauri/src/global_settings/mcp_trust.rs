//! The MCP facts of the user-global store: the trust records (GSS-FR-UTCM),
//! the switches (GSS-FR-KNUB), and the personal server records (GSS-FR-EAJV,
//! GSS-FR-RPBB).
//!
//! One part of `global_settings.rs`. `MCS-mcp-server-connections.md` reads and
//! writes these (MCS-FR-PPPL, MCS-FR-KKDO, MCS-FR-OEDO); this module persists
//! them and interprets none.

use std::collections::BTreeMap;

use super::GlobalSettingsStore;

impl GlobalSettingsStore {
    /// GSS-FR-UTCM: the trusted fingerprint of each MCP server of the project,
    /// keyed by server id. Empty until a server is trusted.
    pub fn load_mcp_trust(&self, key: &str) -> Result<BTreeMap<String, String>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).mcp_trust)
    }

    /// GSS-FR-UTCM: record the fingerprint the author trusted for one server, or
    /// remove the record of that server with `None`.
    pub fn save_mcp_trust(
        &self,
        key: &str,
        server_id: &str,
        fingerprint: Option<&str>,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| match fingerprint {
            Some(fingerprint) => {
                slot.mcp_trust.insert(server_id.to_string(), fingerprint.to_string());
            }
            None => {
                slot.mcp_trust.remove(server_id);
            }
        })
    }
}

impl GlobalSettingsStore {
    /// GSS-FR-KNUB: the switch on this computer of each team MCP server of the
    /// project, keyed by server id. Empty until the author sets one.
    pub fn load_mcp_switches(&self, key: &str) -> Result<BTreeMap<String, bool>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).mcp_switches)
    }

    /// GSS-FR-KNUB: store the switch of one server, or remove it with `None`.
    pub fn save_mcp_switch(
        &self,
        key: &str,
        server_id: &str,
        enabled: Option<bool>,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| match enabled {
            Some(enabled) => {
                slot.mcp_switches.insert(server_id.to_string(), enabled);
            }
            None => {
                slot.mcp_switches.remove(server_id);
            }
        })
    }

    /// GSS-FR-EAJV: the personal MCP server records of the project, as tables.
    pub fn load_personal_mcp_servers(&self, key: &str) -> Result<Vec<toml::Table>, String> {
        let state = self.lock()?;
        Ok(Self::slot(&state, key).personal_mcp_servers)
    }

    /// GSS-FR-EAJV: replace the personal MCP server records of the project.
    pub fn save_personal_mcp_servers(
        &self,
        key: &str,
        servers: Vec<toml::Table>,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| slot.personal_mcp_servers = servers)
    }

    /// GSS-FR-EAJV: change the personal MCP server records of the project
    /// under one lock. The store is written also when `apply` changed nothing.
    pub fn update_personal_mcp_servers<R>(
        &self,
        key: &str,
        apply: impl FnOnce(&mut Vec<toml::Table>) -> R,
    ) -> Result<R, String> {
        let mut out = None;
        self.update_slot(key, |slot| out = Some(apply(&mut slot.personal_mcp_servers)))?;
        out.ok_or_else(|| "personal mcp servers not updated".to_string())
    }

    /// GSS-FR-RPBB: change the personal MCP server records of all projects
    /// under one lock.
    pub fn update_global_mcp_servers<R>(
        &self,
        apply: impl FnOnce(&mut Vec<toml::Table>) -> R,
    ) -> Result<R, String> {
        let mut state = self.lock()?;
        let out = apply(&mut state.personal_mcp_servers);
        self.persist(&state)?;
        Ok(out)
    }

    /// GSS-FR-RPBB: the personal MCP server records of all projects, as tables.
    pub fn load_global_mcp_servers(&self) -> Result<Vec<toml::Table>, String> {
        Ok(self.lock()?.personal_mcp_servers.clone())
    }

    /// GSS-FR-RPBB: replace the personal MCP server records of all projects.
    pub fn save_global_mcp_servers(&self, servers: Vec<toml::Table>) -> Result<(), String> {
        let mut state = self.lock()?;
        state.personal_mcp_servers = servers;
        self.persist(&state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GSS-FR-UTCM: the trust records start empty, survive a reload of
    // `synthesis.toml`, are keyed by project, and one is removed by `None`.
    #[test]
    fn mcp_trust_persists_per_project_and_is_removed_by_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert!(store.load_mcp_trust("/repo").unwrap().is_empty());
        store.save_mcp_trust("/repo", "docs", Some("abc")).unwrap();
        store.save_mcp_trust("/repo", "tracker", Some("def")).unwrap();
        let reloaded = GlobalSettingsStore::with_path(path);
        let trust = reloaded.load_mcp_trust("/repo").unwrap();
        assert_eq!(trust.get("docs").map(String::as_str), Some("abc"));
        assert_eq!(trust.get("tracker").map(String::as_str), Some("def"));
        assert!(reloaded.load_mcp_trust("/other").unwrap().is_empty());
        reloaded.save_mcp_trust("/repo", "docs", None).unwrap();
        assert!(!reloaded.load_mcp_trust("/repo").unwrap().contains_key("docs"));
    }

    fn table(id: &str) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert("id".into(), toml::Value::String(id.into()));
        table.insert("transport".into(), toml::Value::String("future".into()));
        table
    }

    // GSS-FR-KNUB: the switches start empty, survive a reload, are keyed by
    // project, and one is removed by `None`.
    #[test]
    fn mcp_switches_persist_per_project_and_are_removed_by_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert!(store.load_mcp_switches("/repo").unwrap().is_empty());
        store.save_mcp_switch("/repo", "docs", Some(false)).unwrap();
        store.save_mcp_switch("/repo", "tracker", Some(true)).unwrap();
        let reloaded = GlobalSettingsStore::with_path(path);
        let switches = reloaded.load_mcp_switches("/repo").unwrap();
        assert_eq!(switches.get("docs"), Some(&false));
        assert_eq!(switches.get("tracker"), Some(&true));
        assert!(reloaded.load_mcp_switches("/other").unwrap().is_empty());
        reloaded.save_mcp_switch("/repo", "docs", None).unwrap();
        assert!(!reloaded.load_mcp_switches("/repo").unwrap().contains_key("docs"));
    }

    // GSS-FR-EAJV, GSS-FR-RPBB: the personal records of one project and of all
    // projects survive a reload in their order, and a table that no build can
    // decode is kept as it was.
    #[test]
    fn personal_mcp_servers_persist_per_project_and_for_all_projects() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert!(store.load_personal_mcp_servers("/repo").unwrap().is_empty());
        assert!(store.load_global_mcp_servers().unwrap().is_empty());
        store.save_personal_mcp_servers("/repo", vec![table("b"), table("a")]).unwrap();
        store.save_global_mcp_servers(vec![table("all")]).unwrap();
        store.save_mcp_trust("/repo", "docs", Some("abc")).unwrap();
        let reloaded = GlobalSettingsStore::with_path(path);
        assert_eq!(
            reloaded.load_personal_mcp_servers("/repo").unwrap(),
            vec![table("b"), table("a")]
        );
        assert!(reloaded.load_personal_mcp_servers("/other").unwrap().is_empty());
        assert_eq!(reloaded.load_global_mcp_servers().unwrap(), vec![table("all")]);
        assert_eq!(reloaded.load_mcp_trust("/repo").unwrap().len(), 1);
    }
}
