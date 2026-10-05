//! The GitHub publication settings (PSS-FR-HWBG, PSS-FR-RDXE, PSS-FR-VCZM).
//!
//! The settings are project-public: the parent issue Types, the sub-issue
//! Type, and the sub-issue milestone policy are the same for every checkout of
//! the project. They live in a table of their own, apart from the polling
//! settings, so a write of one group carries the other through unchanged.
//!
//! `GHP-github-publication.md` (GHP-FR-KVRH) is the only caller, and no
//! function here is a Tauri command.

use serde::{Deserialize, Serialize};

use super::{load_public, save_public};

/// PSS-FR-HWBG: the project-public table the publication settings live in.
const GITHUB_PUBLICATION_KEY: &str = "githubPublication";
const PARENT_ISSUE_TYPES_KEY: &str = "parentIssueTypes";
const SUB_ISSUE_TYPE_KEY: &str = "subIssueType";
const MILESTONE_POLICY_KEY: &str = "subIssueMilestonePolicy";

/// PSS-FR-HWBG: the parent Type a setting that is unset reads as.
pub const DEFAULT_PARENT_ISSUE_TYPE: &str = "Feature";
/// PSS-FR-HWBG: the sub-issue Type a setting that is unset reads as.
pub const DEFAULT_SUB_ISSUE_TYPE: &str = "Task";

/// GHP-FR-AZPF: how a sub-issue's milestone is resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MilestonePolicy {
    /// The default (PSS-FR-HWBG).
    #[default]
    InheritParent,
    NoMilestone,
    AuthorSelected,
}

/// GHP-FR-KVRH: the three project-public publication settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPublicationSettings {
    pub parent_issue_types: Vec<String>,
    pub sub_issue_type: String,
    pub sub_issue_milestone_policy: MilestonePolicy,
}

impl Default for GithubPublicationSettings {
    fn default() -> Self {
        Self {
            parent_issue_types: vec![DEFAULT_PARENT_ISSUE_TYPE.to_string()],
            sub_issue_type: DEFAULT_SUB_ISSUE_TYPE.to_string(),
            sub_issue_milestone_policy: MilestonePolicy::default(),
        }
    }
}

/// PSS-FR-RDXE: the Types of a stored list that are valid: non-blank strings,
/// trimmed, each once (compared without regard to case). A list that holds none
/// is not valid.
pub fn clean_type_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut clean: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim();
        if !name.is_empty() && !clean.iter().any(|known| known.eq_ignore_ascii_case(name)) {
            clean.push(name.to_string());
        }
    }
    clean
}

/// PSS-FR-HWBG / PSS-FR-RDXE: the stored settings, each value that is unset or
/// not valid read as its default. A malformed `project.toml` is the typed error
/// of PSS-FR-10.
pub fn load_github_publication_settings_from(
    root: &crate::fs::RootFs,
) -> Result<GithubPublicationSettings, String> {
    let table = load_public(root)?;
    let mut settings = GithubPublicationSettings::default();
    let Some(section) = table.get(GITHUB_PUBLICATION_KEY).and_then(|v| v.as_table()) else {
        return Ok(settings);
    };
    let stored_types = clean_type_names(
        section
            .get(PARENT_ISSUE_TYPES_KEY)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string)),
    );
    if !stored_types.is_empty() {
        settings.parent_issue_types = stored_types;
    }
    if let Some(name) = section
        .get(SUB_ISSUE_TYPE_KEY)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        settings.sub_issue_type = name.to_string();
    }
    if let Some(policy) = section
        .get(MILESTONE_POLICY_KEY)
        .cloned()
        .and_then(|v| v.try_into::<MilestonePolicy>().ok())
    {
        settings.sub_issue_milestone_policy = policy;
    }
    Ok(settings)
}

/// PSS-FR-HWBG / PSS-FR-VCZM: persist the settings. Every other project-public
/// section, the polling settings included, is carried through unchanged
/// (PSS-FR-17).
pub fn save_github_publication_settings_to(
    root: &crate::fs::RootFs,
    settings: &GithubPublicationSettings,
) -> Result<(), String> {
    let mut table = load_public(root)?;
    let mut section = toml::Table::new();
    section.insert(
        PARENT_ISSUE_TYPES_KEY.to_string(),
        toml::Value::Array(
            settings
                .parent_issue_types
                .iter()
                .map(|name| toml::Value::String(name.clone()))
                .collect(),
        ),
    );
    section.insert(
        SUB_ISSUE_TYPE_KEY.to_string(),
        toml::Value::String(settings.sub_issue_type.clone()),
    );
    section.insert(
        MILESTONE_POLICY_KEY.to_string(),
        toml::Value::try_from(settings.sub_issue_milestone_policy)
            .map_err(|_| "failed to encode the GitHub publication settings".to_string())?,
    );
    table.insert(GITHUB_PUBLICATION_KEY.to_string(), toml::Value::Table(section));
    save_public(root, &table)
}
