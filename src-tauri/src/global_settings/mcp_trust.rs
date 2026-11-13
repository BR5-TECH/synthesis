//! The MCP trust records of the user-global store (GSS-FR-UTCM).
//!
//! One part of `global_settings.rs`. `MCS-mcp-server-connections.md` reads and
//! writes these (MCS-FR-PPPL); this module persists them and interprets none.

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
}
