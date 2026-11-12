//! The pure half of the spec index: from the reads to the index
//! (`OSI-openspec-spec-index.md` OSI-FR-XUJG through OSI-FR-GRCV).
//!
//! Nothing here runs a process or reads a file, so every rule of the shape of
//! the index is testable on fixed reads.

use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// OSI-FR-WFJU: the id and the label of the root group.
pub const ROOT_GROUP_ID: &str = "group:/";
pub const ROOT_GROUP_LABEL: &str = "specs";

/// What one location of OSI-FR-QELX gave: its active changes, or `None` where
/// `list_changes` failed (OSI-FR-DENT).
#[derive(Clone, Debug)]
pub struct LocationRead {
    pub location: ChipLocation,
    pub changes: Option<Vec<crate::openspec::ChangeSummary>>,
    /// OSI-FR-GRCV: the validation ran in this location's working copy.
    pub validated: bool,
}

/// OSI-FR-MAUQ: the segment of the group of a capability id, or `None` for
/// the root group.
fn group_of(id: &str) -> Option<&str> {
    id.split_once('/').map(|(segment, _)| segment)
}

/// OSI-FR-DVLQ: the position, from 0, of the requirement an issue path names,
/// where it starts with `requirements[<i>]`.
fn requirement_index(path: &str) -> Option<usize> {
    let rest = path.strip_prefix("requirements[")?;
    let (digits, _) = rest.split_once(']')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn map_issue(issue: &crate::openspec::ValidationIssue) -> MapIssue {
    MapIssue { level: issue.level.clone(), message: issue.message.clone() }
}

/// OSI-FR-OJRE: an issue that makes a requirement a gap.
fn is_gap_level(issue: &MapIssue) -> bool {
    issue.level == "ERROR" || issue.level == "WARNING"
}

/// OSI-FR-ALQI: the order of the chips of one node: by change name, then the
/// active worktree first, then by stream name. A stream id breaks a tie of
/// two stream names, so the order is the same at each load.
fn chip_order(a: &ChangeChip, b: &ChangeChip) -> std::cmp::Ordering {
    let rank = |chip: &ChangeChip| match &chip.location {
        ChipLocation::ActiveWorktree => (0, String::new(), String::new()),
        ChipLocation::Stream { stream_id, stream_name } => {
            (1, stream_name.clone(), stream_id.clone())
        }
    };
    a.change.cmp(&b.change).then_with(|| rank(a).cmp(&rank(b)))
}

/// OSI-FR-XUJG through OSI-FR-GRCV: the index of the live capabilities, the
/// validation, and the changes of each location.
///
/// `capabilities` holds each live capability read whole. `validation` is
/// `None` where the validation failed (OSI-FR-OIOB).
pub fn build_index(
    capabilities: &[crate::openspec::CapabilitySpec],
    validation: Option<&crate::openspec::ProjectValidation>,
    locations: &[LocationRead],
) -> OpenSpecMapIndex {
    // OSI-FR-DVLQ, OSI-FR-SBVF: the issues of each live capability and of
    // each of its requirements, in the order the CLI gives them. A spec item
    // that names no live capability gives no issue.
    let live: BTreeSet<&str> = capabilities.iter().map(|c| c.id.as_str()).collect();
    let mut capability_issues: BTreeMap<&str, Vec<MapIssue>> = BTreeMap::new();
    let mut requirement_issues: BTreeMap<(&str, usize), Vec<MapIssue>> = BTreeMap::new();
    let mut change_issues: BTreeMap<&str, Vec<MapIssue>> = BTreeMap::new();
    if let Some(validation) = validation {
        for item in &validation.items {
            if item.kind == "change" {
                // OSI-FR-GRCV: for the chips of the location the validation ran in.
                change_issues
                    .entry(item.id.as_str())
                    .or_default()
                    .extend(item.issues.iter().map(map_issue));
                continue;
            }
            let Some(id) = live.get(item.id.as_str()).copied() else {
                continue;
            };
            for issue in &item.issues {
                let count =
                    capabilities.iter().find(|c| c.id == id).map_or(0, |c| c.requirements.len());
                match requirement_index(&issue.path).filter(|i| *i < count) {
                    Some(i) => {
                        requirement_issues.entry((id, i)).or_default().push(map_issue(issue))
                    }
                    None => capability_issues.entry(id).or_default().push(map_issue(issue)),
                }
            }
        }
    }

    // OSI-FR-ALQI, OSI-FR-VOOS, OSI-FR-GRCV: the chips of each capability id
    // that a change names, live or planned.
    let mut chips: BTreeMap<String, Vec<ChangeChip>> = BTreeMap::new();
    for read in locations {
        let Some(changes) = &read.changes else {
            continue;
        };
        for change in changes {
            for capability in &change.capabilities {
                let issues = if read.validated {
                    change_issues.get(change.name.as_str()).cloned().unwrap_or_default()
                } else {
                    Vec::new()
                };
                chips.entry(capability.clone()).or_default().push(ChangeChip {
                    change: change.name.clone(),
                    location: read.location.clone(),
                    issues,
                });
            }
        }
    }
    for list in chips.values_mut() {
        list.sort_by(chip_order);
    }

    // OSI-FR-OHJE, OSI-FR-SEYB, OSI-FR-CWLE, OSI-FR-OJRE: the capability
    // nodes of the live capabilities.
    let mut nodes: Vec<(String, IndexNode)> = Vec::new();
    for capability in capabilities {
        let id = capability.id.as_str();
        let own = capability_issues.remove(id).unwrap_or_default();
        let capability_gap = own.iter().any(is_gap_level);
        let path = format!("openspec/specs/{id}/spec.md");
        let requirements: Vec<SpecNode> = capability
            .requirements
            .iter()
            .enumerate()
            .map(|(i, requirement)| {
                let issues = requirement_issues.remove(&(id, i)).unwrap_or_default();
                let gap = capability_gap || issues.iter().any(is_gap_level);
                SpecNode {
                    code: format!("{id}#{}", i + 1),
                    badge: format!("R{}", i + 1),
                    path: path.clone(),
                    label: requirement.name.clone(),
                    summary: requirement.text.clone(),
                    requirements: 1,
                    scenarios: requirement.scenarios.len() as u64,
                    state: if gap { "gap" } else { "built" },
                    issues,
                }
            })
            .collect();
        let label = if capability.title.trim().is_empty() {
            id.to_string()
        } else {
            capability.title.clone()
        };
        nodes.push((
            id.to_string(),
            IndexNode {
                id: format!("capability:{id}"),
                label,
                summary: capability.overview.clone(),
                children: IndexChildren::Specs(requirements),
                planned: false,
                issues: own,
                changes: chips.remove(id).unwrap_or_default(),
            },
        ));
    }
    // OSI-FR-VOOS: a capability id of a change that names no live capability.
    for (id, list) in std::mem::take(&mut chips) {
        nodes.push((
            id.clone(),
            IndexNode {
                id: format!("capability:{id}"),
                label: id,
                summary: String::new(),
                children: IndexChildren::Specs(Vec::new()),
                planned: true,
                issues: Vec::new(),
                changes: list,
            },
        ));
    }
    nodes.sort_by(|a, b| a.0.cmp(&b.0));

    // OSI-FR-MAUQ, OSI-FR-WFJU: the groups, the root group first.
    let mut root: Vec<IndexNode> = Vec::new();
    let mut groups: BTreeMap<String, Vec<IndexNode>> = BTreeMap::new();
    for (id, node) in nodes {
        match group_of(&id) {
            None => root.push(node),
            Some(segment) => groups.entry(segment.to_string()).or_default().push(node),
        }
    }
    let mut roots: Vec<IndexNode> = Vec::new();
    if !root.is_empty() {
        roots.push(group(ROOT_GROUP_ID.to_string(), ROOT_GROUP_LABEL.to_string(), root));
    }
    for (segment, children) in groups {
        roots.push(group(format!("group:{segment}"), segment, children));
    }

    OpenSpecMapIndex {
        levels: levels(),
        roots,
        dependencies: Vec::new(),
        validation: match validation {
            None => "unavailable",
            Some(v) if v.valid => "valid",
            Some(_) => "invalid",
        },
        unread_locations: locations.iter().filter(|read| read.changes.is_none()).count() as u64,
    }
}

fn group(id: String, label: String, children: Vec<IndexNode>) -> IndexNode {
    IndexNode {
        id,
        label,
        summary: String::new(),
        children: IndexChildren::Nodes(children),
        planned: false,
        issues: Vec::new(),
        changes: Vec::new(),
    }
}

/// OSI-FR-XUJG: the three levels.
fn levels() -> Vec<LevelName> {
    [("groups", "group"), ("capabilities", "capability"), ("requirements", "requirement")]
        .into_iter()
        .map(|(plural, singular)| LevelName {
            plural: plural.to_string(),
            singular: singular.to_string(),
        })
        .collect()
}

/// OSI-FR-YVZW: the counts a load record carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexCounts {
    pub groups: usize,
    pub capabilities: usize,
    pub requirements: usize,
    pub chips: usize,
    pub issues: usize,
}

impl OpenSpecMapIndex {
    pub fn counts(&self) -> IndexCounts {
        let mut counts = IndexCounts { groups: self.roots.len(), ..IndexCounts::default() };
        for group in &self.roots {
            let IndexChildren::Nodes(capabilities) = &group.children else {
                continue;
            };
            for capability in capabilities {
                counts.capabilities += 1;
                counts.chips += capability.changes.len();
                counts.issues += capability.issues.len();
                counts.issues += capability.changes.iter().map(|c| c.issues.len()).sum::<usize>();
                if let IndexChildren::Specs(specs) = &capability.children {
                    counts.requirements += specs.len();
                    counts.issues += specs.iter().map(|s| s.issues.len()).sum::<usize>();
                }
            }
        }
        counts
    }
}
