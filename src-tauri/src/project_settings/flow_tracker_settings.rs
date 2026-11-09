//! The flow tracker settings (PSS-FR-VGWF, PSS-FR-DNVN).
//!
//! The settings are project-public: the issue Type of each issue kind of a
//! delivery plan is the same for every checkout of the project. They live in a
//! table of their own, so a write of this table carries every other section
//! through unchanged, and a write of another section carries this one through.
//!
//! `FTS-flow-tracker-sync.md` (FTS-FR-LMBE, FTS-FR-ALOJ) is the only caller,
//! and no function here is a Tauri command.

use serde::{Deserialize, Serialize};

use super::{load_public, save_public};

/// PSS-FR-VGWF: the project-public table the flow tracker settings live in.
const FLOW_TRACKER_KEY: &str = "flowTracker";
const DELIVERABLE_TYPE_KEY: &str = "deliverableType";
const EPIC_TYPE_KEY: &str = "epicType";
const FEATURE_TYPE_KEY: &str = "featureType";
const TASK_TYPE_KEY: &str = "taskType";
const DEFECT_TYPE_KEY: &str = "defectType";

/// PSS-FR-VGWF: the Types that an unset value reads as.
pub const DEFAULT_DELIVERABLE_TYPE: &str = "Feature";
pub const DEFAULT_EPIC_TYPE: &str = "Feature";
pub const DEFAULT_FEATURE_TYPE: &str = "Feature";
pub const DEFAULT_TASK_TYPE: &str = "Task";
pub const DEFAULT_DEFECT_TYPE: &str = "Bug";

/// FTS-FR-LMBE: the issue Type of each issue kind of a delivery plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowTrackerSettings {
    pub deliverable_type: String,
    pub epic_type: String,
    pub feature_type: String,
    pub task_type: String,
    pub defect_type: String,
}

impl Default for FlowTrackerSettings {
    fn default() -> Self {
        Self {
            deliverable_type: DEFAULT_DELIVERABLE_TYPE.to_string(),
            epic_type: DEFAULT_EPIC_TYPE.to_string(),
            feature_type: DEFAULT_FEATURE_TYPE.to_string(),
            task_type: DEFAULT_TASK_TYPE.to_string(),
            defect_type: DEFAULT_DEFECT_TYPE.to_string(),
        }
    }
}

/// The five keys and the field of each, in the order the table writes them.
fn fields(settings: &mut FlowTrackerSettings) -> [(&'static str, &mut String); 5] {
    [
        (DELIVERABLE_TYPE_KEY, &mut settings.deliverable_type),
        (EPIC_TYPE_KEY, &mut settings.epic_type),
        (FEATURE_TYPE_KEY, &mut settings.feature_type),
        (TASK_TYPE_KEY, &mut settings.task_type),
        (DEFECT_TYPE_KEY, &mut settings.defect_type),
    ]
}

/// PSS-FR-VGWF: the stored settings. A value that is unset, is not a string, or
/// is blank reads as its default. A malformed `project.toml` is the typed error
/// of PSS-FR-10.
pub fn load_flow_tracker_settings_from(
    root: &crate::fs::RootFs,
) -> Result<FlowTrackerSettings, String> {
    let table = load_public(root)?;
    let mut settings = FlowTrackerSettings::default();
    let Some(section) = table.get(FLOW_TRACKER_KEY).and_then(|v| v.as_table()) else {
        return Ok(settings);
    };
    for (key, field) in fields(&mut settings) {
        if let Some(name) =
            section.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|name| !name.is_empty())
        {
            *field = name.to_string();
        }
    }
    Ok(settings)
}

/// PSS-FR-VGWF / PSS-FR-DNVN: persist the settings in their own table. Every
/// other project-public section is carried through unchanged (PSS-FR-17).
pub fn save_flow_tracker_settings_to(
    root: &crate::fs::RootFs,
    settings: &FlowTrackerSettings,
) -> Result<(), String> {
    let mut table = load_public(root)?;
    let mut section = toml::Table::new();
    let mut copy = settings.clone();
    for (key, field) in fields(&mut copy) {
        section.insert(key.to_string(), toml::Value::String(field.clone()));
    }
    table.insert(FLOW_TRACKER_KEY.to_string(), toml::Value::Table(section));
    save_public(root, &table)
}
