//! The typed shapes of the read seam, and how CLI output becomes them
//! (`OSM-openspec-mode.md` OSM-FR-JTOQ, OSM-FR-JAIC, OSM-FR-OPZF,
//! OSM-FR-FCMY, OSM-FR-WWWK).
//!
//! Each `parse_*` function is pure: it takes the bytes a CLI process printed
//! and gives the shape, or `cli_output_unreadable`. A field the expected shape
//! does not name is ignored (OSM-FR-WWWK).

use serde::{Deserialize, Serialize};

use super::OpenSpecReadError;
use crate::fs::{EntryKind, RootFs};

/// One capability of `openspec/specs/`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub id: String,
    pub requirement_count: u64,
}

/// One capability, read whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySpec {
    pub id: String,
    pub title: String,
    pub overview: String,
    pub requirements: Vec<Requirement>,
}

/// One requirement, under the name OpenSpec matches deltas on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Requirement {
    pub name: String,
    pub text: String,
    pub scenarios: Vec<Scenario>,
}

/// One scenario of a requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Scenario {
    pub name: String,
    pub text: String,
}

/// One active change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSummary {
    pub name: String,
    pub schema: String,
    pub artifacts: Vec<ArtifactState>,
    pub is_complete: bool,
    pub completed_tasks: u64,
    pub total_tasks: u64,
    pub capabilities: Vec<String>,
    pub last_modified: String,
}

/// The state of one artifact of a change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArtifactState {
    pub id: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// The CLI fields read (OSM contract surface, "CLI fields read")
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SpecList {
    specs: Vec<SpecEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecEntry {
    id: String,
    requirement_count: u64,
}

#[derive(Deserialize)]
struct SpecShow {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    overview: String,
    #[serde(default)]
    requirements: Vec<RequirementShow>,
}

#[derive(Deserialize)]
struct RequirementShow {
    name: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    scenarios: Vec<ScenarioShow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioShow {
    #[serde(default)]
    name: String,
    #[serde(default)]
    raw_text: String,
}

#[derive(Deserialize)]
struct ChangeList {
    changes: Vec<ChangeEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeEntry {
    name: String,
    completed_tasks: u64,
    total_tasks: u64,
    last_modified: String,
}

#[derive(Deserialize)]
struct StatusAll {
    #[serde(default)]
    changes: Vec<StatusEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusEntry {
    change_name: String,
    #[serde(default)]
    schema_name: String,
    #[serde(default)]
    is_complete: bool,
    #[serde(default)]
    artifacts: Vec<ArtifactShow>,
}

#[derive(Deserialize)]
struct ArtifactShow {
    id: String,
    status: String,
}

fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, OpenSpecReadError> {
    serde_json::from_slice(bytes).map_err(|_| OpenSpecReadError::CliOutputUnreadable)
}

/// OSM-FR-JTOQ: one `Capability` for each entry of `list --specs --json`,
/// ordered by id.
pub fn parse_capabilities(bytes: &[u8]) -> Result<Vec<Capability>, OpenSpecReadError> {
    let list: SpecList = decode(bytes)?;
    let mut out: Vec<Capability> = list
        .specs
        .into_iter()
        .map(|s| Capability { id: s.id, requirement_count: s.requirement_count })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// OSM-FR-SIBT: the `CapabilitySpec` of `show <id> --type spec --json`, with
/// its requirements in the order of the spec file.
pub fn parse_capability(bytes: &[u8]) -> Result<CapabilitySpec, OpenSpecReadError> {
    let show: SpecShow = decode(bytes)?;
    Ok(CapabilitySpec {
        id: show.id,
        title: show.title,
        overview: show.overview,
        requirements: show
            .requirements
            .into_iter()
            .map(|r| Requirement {
                name: r.name,
                text: r.text,
                scenarios: r
                    .scenarios
                    .into_iter()
                    .map(|s| Scenario { name: s.name, text: s.raw_text })
                    .collect(),
            })
            .collect(),
    })
}

/// OSM-FR-OPZF: join `list --changes --json` and `status --all --json` by
/// name. The list holds active changes alone. `capabilities` is filled by the
/// caller from the directory listing (OSM-FR-FCMY). A change that the status
/// output does not name keeps an empty schema and no artifacts.
pub fn parse_changes(list: &[u8], status: &[u8]) -> Result<Vec<ChangeSummary>, OpenSpecReadError> {
    let list: ChangeList = decode(list)?;
    let status: StatusAll = decode(status)?;
    Ok(list
        .changes
        .into_iter()
        // `list --changes` lists no archived change. A folder named `archive`
        // is the archive itself and never a change.
        .filter(|c| c.name != "archive")
        .map(|c| {
            let state = status.changes.iter().find(|s| s.change_name == c.name);
            ChangeSummary {
                schema: state.map(|s| s.schema_name.clone()).unwrap_or_default(),
                artifacts: state
                    .map(|s| {
                        s.artifacts
                            .iter()
                            .map(|a| ArtifactState { id: a.id.clone(), status: a.status.clone() })
                            .collect()
                    })
                    .unwrap_or_default(),
                is_complete: state.is_some_and(|s| s.is_complete),
                completed_tasks: c.completed_tasks,
                total_tasks: c.total_tasks,
                capabilities: Vec::new(),
                last_modified: c.last_modified,
                name: c.name,
            }
        })
        .collect())
}

/// OSM-FR-JAIC: an id the seam may hand to the CLI. Not empty, not absolute,
/// no `..` segment, and only `A-Z`, `a-z`, `0-9`, `.`, `_`, `-`, and `/`.
pub fn valid_capability_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('/')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
        && id.split('/').all(|seg| !seg.is_empty() && seg != ".." && seg != ".")
}

/// OSM-FR-FCMY: each folder path below `openspec/changes/<name>/specs/` that
/// holds a `spec.md`, ordered by id, from the directory listing alone.
pub fn delta_capabilities(root: &RootFs, change: &str) -> Vec<String> {
    let base = std::path::PathBuf::from("openspec/changes").join(change).join("specs");
    let mut found = Vec::new();
    let mut pending: Vec<String> = vec![String::new()];
    while let Some(rel) = pending.pop() {
        let dir = if rel.is_empty() { base.clone() } else { base.join(&rel) };
        let Ok(entries) = root.list_dir(root.path().join(&dir)) else {
            continue;
        };
        for entry in entries {
            match entry.kind {
                EntryKind::File if entry.name == super::detection::SPEC_FILE && !rel.is_empty() => {
                    found.push(rel.clone());
                }
                EntryKind::Dir => pending.push(if rel.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{rel}/{}", entry.name)
                }),
                _ => {}
            }
        }
    }
    found.sort();
    found.dedup();
    found
}
