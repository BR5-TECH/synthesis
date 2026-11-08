//! The agent group settings of the user-global store (GSS-FR-ARIP).
//!
//! One part of `global_settings.rs`. `PSS-project-settings-storage.md` reads and
//! writes these; this module persists them and interprets neither.

use super::GlobalSettingsStore;
use crate::agent_groups::AgentGroupSettings;

impl GlobalSettingsStore {
    /// GSS-FR-ARIP: the project's agent group settings, off and 60 seconds
    /// where nothing was written.
    pub fn load_agent_group_settings(&self, key: &str) -> Result<AgentGroupSettings, String> {
        let state = self.lock()?;
        let slot = Self::slot(&state, key);
        let defaults = AgentGroupSettings::default();
        Ok(AgentGroupSettings {
            enabled: slot.agent_group_enabled.unwrap_or(defaults.enabled),
            join_timeout_secs: slot
                .agent_group_join_timeout_secs
                .unwrap_or(defaults.join_timeout_secs),
        })
    }

    /// GSS-FR-ARIP: persist the project's agent group settings in its slot,
    /// keyed by project (GSS-FR-18). The values are stored as given; the range of
    /// the timeout is checked by the caller (PSS-FR-CQML).
    pub fn save_agent_group_settings(
        &self,
        key: &str,
        settings: &AgentGroupSettings,
    ) -> Result<(), String> {
        self.update_slot(key, |slot| {
            slot.agent_group_enabled = Some(settings.enabled);
            slot.agent_group_join_timeout_secs = Some(settings.join_timeout_secs);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GSS-FR-ARIP, PSS-FR-PARJ: the defaults, and both values survive a reload of
    // `synthesis.toml`, keyed by project. Nothing is written to a file of the project.
    #[test]
    fn agent_group_settings_default_persist_and_are_keyed_by_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthesis.toml");
        let store = GlobalSettingsStore::with_path(path.clone());
        assert_eq!(
            store.load_agent_group_settings("/repo").unwrap(),
            AgentGroupSettings { enabled: false, join_timeout_secs: 60 }
        );
        store
            .save_agent_group_settings(
                "/repo",
                &AgentGroupSettings { enabled: true, join_timeout_secs: 90 },
            )
            .unwrap();
        let reloaded = GlobalSettingsStore::with_path(path);
        assert_eq!(
            reloaded.load_agent_group_settings("/repo").unwrap(),
            AgentGroupSettings { enabled: true, join_timeout_secs: 90 }
        );
        assert_eq!(
            reloaded.load_agent_group_settings("/other").unwrap(),
            AgentGroupSettings::default()
        );
    }
}
