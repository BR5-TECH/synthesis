//! The agent group settings: the switch and the join timeout (PSS-FR-CQML).

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::global_settings::GlobalSettingsStore;
use crate::logging::{log_info, Domain, BUFFER};

/// PSS-FR-CQML: the shortest join timeout, in seconds.
pub const MIN_JOIN_TIMEOUT_SECS: u32 = 5;
/// PSS-FR-CQML: the longest join timeout, in seconds.
pub const MAX_JOIN_TIMEOUT_SECS: u32 = 600;
/// PSS-FR-CQML: the typed refusal of a timeout outside the range.
pub const ERR_JOIN_TIMEOUT_OUT_OF_RANGE: &str = "join_timeout_out_of_range";

/// PSS-FR-CQML: off by default, and 60 seconds.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentGroupSettings {
    pub enabled: bool,
    pub join_timeout_secs: u32,
}

impl Default for AgentGroupSettings {
    fn default() -> Self {
        Self { enabled: false, join_timeout_secs: 60 }
    }
}

impl AgentGroupSettings {
    /// PSS-FR-CQML: the range check of a save.
    pub fn validate(&self) -> Result<(), String> {
        if (MIN_JOIN_TIMEOUT_SECS..=MAX_JOIN_TIMEOUT_SECS).contains(&self.join_timeout_secs) {
            Ok(())
        } else {
            Err(ERR_JOIN_TIMEOUT_OUT_OF_RANGE.to_string())
        }
    }
}

/// `"load agent group settings"` (PSS-FR-CQML).
#[tauri::command]
pub fn load_agent_group_settings(
    project: tauri::State<'_, crate::project::ProjectState>,
    store: tauri::State<'_, GlobalSettingsStore>,
) -> Result<AgentGroupSettings, String> {
    let key = project.anchor().ok_or_else(|| crate::agents::ERR_NO_PROJECT_OPEN.to_string())?;
    store.load_agent_group_settings(&key)
}

/// `"save agent group settings"` (PSS-FR-CQML).
#[tauri::command]
pub fn save_agent_group_settings<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    settings: AgentGroupSettings,
) -> Result<AgentGroupSettings, String> {
    settings.validate()?;
    let project = app.state::<crate::project::ProjectState>();
    let key = project.anchor().ok_or_else(|| crate::agents::ERR_NO_PROJECT_OPEN.to_string())?;
    app.state::<GlobalSettingsStore>().save_agent_group_settings(&key, &settings)?;
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "agent group settings saved",
        crate::log_fields! {
            "enabled" => settings.enabled,
            "joinTimeoutSecs" => settings.join_timeout_secs,
        },
    );
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    // PSS-FR-CQML: the defaults.
    #[test]
    fn defaults_are_off_and_sixty_seconds() {
        let settings = AgentGroupSettings::default();
        assert!(!settings.enabled);
        assert_eq!(settings.join_timeout_secs, 60);
        assert!(settings.validate().is_ok());
    }

    // PSS-FR-CQML: the range is 5 to 600 seconds and both ends are valid.
    #[test]
    fn a_timeout_outside_the_range_is_refused() {
        for secs in [MIN_JOIN_TIMEOUT_SECS, 60, MAX_JOIN_TIMEOUT_SECS] {
            assert!(AgentGroupSettings { enabled: true, join_timeout_secs: secs }
                .validate()
                .is_ok());
        }
        for secs in [0, MIN_JOIN_TIMEOUT_SECS - 1, MAX_JOIN_TIMEOUT_SECS + 1] {
            assert_eq!(
                AgentGroupSettings { enabled: true, join_timeout_secs: secs }.validate(),
                Err(ERR_JOIN_TIMEOUT_OUT_OF_RANGE.to_string())
            );
        }
    }

    // PSS-FR-CQML: the wire shape is camelCase.
    #[test]
    fn the_wire_shape_is_camel_case() {
        let json = serde_json::to_value(AgentGroupSettings::default()).unwrap();
        assert_eq!(json, serde_json::json!({ "enabled": false, "joinTimeoutSecs": 60 }));
    }
}
