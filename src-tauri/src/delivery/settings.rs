//! The model of the delivery settings (`DSR-delivery-source.md` DSR-FR-HOEW,
//! DSR-FR-ZJMC).
//!
//! The settings hold the source choice and the Type mapping. They are
//! project-public, and `PSS-project-settings-storage.md` (PSS-FR-KZGG) keeps
//! them in `.synthesis/project.toml`; see
//! `crate::project_settings::delivery_settings`. The serde form is the
//! frontend's `DeliverySettings` of `src/types/delivery.ts`: camelCase fields,
//! and snake_case enumeration values.

use serde::{Deserialize, Serialize};

/// DSR-FR-ZJMC: the create Types that an unset value reads as.
pub const DEFAULT_EPIC_TYPE: &str = "Epic";
pub const DEFAULT_FEATURE_TYPE: &str = "Feature";
pub const DEFAULT_TASK_TYPE: &str = "Task";
pub const DEFAULT_DEFECT_TYPE: &str = "Bug";

/// DSR-FR-HOEW: where the project's delivery plan lives. `automatic` is the
/// choice while none is stored (PSS-FR-KZGG).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeliverySourceChoice {
    #[default]
    Automatic,
    Github,
    File {
        /// The artifact id of one `flow`-typed file.
        #[serde(rename = "flowId")]
        flow_id: String,
    },
}

impl DeliverySourceChoice {
    /// The stored name of the choice.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Github => "github",
            Self::File { .. } => "file",
        }
    }
}

/// DSR contract surface `IssueKind`: the kind of an issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    Epic,
    Feature,
    Task,
    Defect,
}

/// DSR-FR-ZJMC: what a read entry or the untyped choice maps an issue to: a
/// kind, or `exclude` (DSR-FR-TZDZ).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappedKind {
    Epic,
    Feature,
    Task,
    Defect,
    Exclude,
}

impl MappedKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Epic => "epic",
            Self::Feature => "feature",
            Self::Task => "task",
            Self::Defect => "defect",
            Self::Exclude => "exclude",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "epic" => Self::Epic,
            "feature" => Self::Feature,
            "task" => Self::Task,
            "defect" => Self::Defect,
            "exclude" => Self::Exclude,
            _ => return None,
        })
    }
}

/// DSR-FR-ZJMC: the create Type of each issue kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreateTypes {
    pub epic: String,
    pub feature: String,
    pub task: String,
    pub defect: String,
}

impl Default for CreateTypes {
    fn default() -> Self {
        Self {
            epic: DEFAULT_EPIC_TYPE.to_string(),
            feature: DEFAULT_FEATURE_TYPE.to_string(),
            task: DEFAULT_TASK_TYPE.to_string(),
            defect: DEFAULT_DEFECT_TYPE.to_string(),
        }
    }
}

impl CreateTypes {
    /// The create Type of `kind`.
    pub fn of(&self, kind: IssueKind) -> &str {
        match kind {
            IssueKind::Epic => &self.epic,
            IssueKind::Feature => &self.feature,
            IssueKind::Task => &self.task,
            IssueKind::Defect => &self.defect,
        }
    }
}

/// DSR-FR-ZJMC: one read entry: a Type name and what it maps to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadEntry {
    #[serde(rename = "type")]
    pub type_name: String,
    pub kind: MappedKind,
}

/// DSR-FR-HOEW, DSR-FR-ZJMC: the delivery settings, with defaults applied.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DeliverySettings {
    pub source: DeliverySourceChoice,
    pub create_types: CreateTypes,
    pub read_entries: Vec<ReadEntry>,
    /// The untyped choice. `None` is unset, which serializes as `null`.
    pub untyped: Option<MappedKind>,
}

/// DSR-FR-MKIG, DSR-FR-TZDZ: what the Type mapping makes of an issue's Type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeMapping {
    Kind(IssueKind),
    /// The issue is not part of the plan, and it is not unmapped.
    Excluded,
    /// The Type maps to nothing, or the issue has no Type and the untyped
    /// choice is unset.
    Unmapped,
}

impl IssueKind {
    pub const ALL: [IssueKind; 4] =
        [IssueKind::Epic, IssueKind::Feature, IssueKind::Task, IssueKind::Defect];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Epic => "epic",
            Self::Feature => "feature",
            Self::Task => "task",
            Self::Defect => "defect",
        }
    }
}

impl MappedKind {
    /// The kind, or `None` for `exclude`.
    pub fn kind(self) -> Option<IssueKind> {
        match self {
            Self::Epic => Some(IssueKind::Epic),
            Self::Feature => Some(IssueKind::Feature),
            Self::Task => Some(IssueKind::Task),
            Self::Defect => Some(IssueKind::Defect),
            Self::Exclude => None,
        }
    }

    fn mapping(self) -> TypeMapping {
        self.kind().map_or(TypeMapping::Excluded, TypeMapping::Kind)
    }
}

impl DeliverySettings {
    /// DSR-FR-MKIG, DSR-FR-TZDZ: the kind of an issue with the Type
    /// `issue_type`. A create Type wins over a read entry, and names compare
    /// exactly.
    pub fn map_type(&self, issue_type: Option<&str>) -> TypeMapping {
        let Some(name) = issue_type else {
            return self.untyped.map_or(TypeMapping::Unmapped, MappedKind::mapping);
        };
        if let Some(kind) = IssueKind::ALL.into_iter().find(|k| self.create_types.of(*k) == name) {
            return TypeMapping::Kind(kind);
        }
        self.read_entries
            .iter()
            .find(|e| e.type_name == name)
            .map_or(TypeMapping::Unmapped, |e| e.kind.mapping())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// DSR-FR-HOEW, DSR-FR-ZJMC, PSS-FR-KZGG: the defaults serialize in the
    /// shape of `src/types/delivery.ts`: the source `automatic`, the four
    /// create Types, no read entry, and an unset untyped choice as `null`.
    #[test]
    fn the_defaults_serialize_in_the_frontend_shape() {
        assert_eq!(
            serde_json::to_value(DeliverySettings::default()).unwrap(),
            json!({
                "source": { "kind": "automatic" },
                "createTypes": { "epic": "Epic", "feature": "Feature", "task": "Task",
                                 "defect": "Bug" },
                "readEntries": [],
                "untyped": null
            })
        );
    }

    /// DSR-FR-HOEW: a `file` choice carries its artifact id as `flowId`, and
    /// every value round-trips through JSON.
    #[test]
    fn a_file_choice_and_a_mapping_round_trip() {
        let value = json!({
            "source": { "kind": "file", "flowId": "plans/beta.flow" },
            "createTypes": { "epic": "Initiative", "feature": "Feature", "task": "Chore",
                             "defect": "Defect" },
            "readEntries": [ { "type": "Story", "kind": "feature" },
                             { "type": "Spike", "kind": "exclude" } ],
            "untyped": "task"
        });
        let settings: DeliverySettings = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            settings.source,
            DeliverySourceChoice::File { flow_id: "plans/beta.flow".into() }
        );
        assert_eq!(settings.create_types.of(IssueKind::Task), "Chore");
        assert_eq!(settings.read_entries[1].kind, MappedKind::Exclude);
        assert_eq!(settings.untyped, Some(MappedKind::Task));
        assert_eq!(serde_json::to_value(&settings).unwrap(), value);

        let github: DeliverySettings =
            serde_json::from_value(json!({ "source": { "kind": "github" } })).unwrap();
        assert_eq!(github.source, DeliverySourceChoice::Github);
        assert_eq!(github.create_types, CreateTypes::default(), "absent fields read as defaults");
    }
}
