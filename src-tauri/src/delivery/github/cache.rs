//! The read cache of a GitHub delivery plan (`GDP-github-delivery-plan.md`
//! GDP-FR-YOSF, GDP-FR-LKXA, GDP-FR-GCJO): what the last GitHub read of one
//! Project said, and nothing derived from it.
//!
//! The record is `delivery/github/<project-node-id>.json` in the repository
//! store (RMS-FR-OCPB). Each write replaces the whole record atomically
//! (RMS-FR-PFOB). The cache is never the source of a write to GitHub.

use serde::{Deserialize, Serialize};

use crate::delivery::machine_state::records::{read_record, record_path, write_record};
use crate::delivery::DeliveryError;
use crate::fs::RootFs;
use crate::logging::{LogBuffer, LogSink};

use super::adapter::{
    ItemContent, ProjectItemRead, ProjectRead, TrackerIssue, TrackerMilestone, TrackerRepository,
};

/// The folder of `delivery/` that holds the read caches.
const CACHE_DIRNAME: &str = "github";

/// GDP-FR-YOSF: what one GitHub read found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerRead {
    /// GDP-FR-WEFV: the instant of the read, RFC 3339.
    pub read_at: String,
    pub repository: TrackerRepository,
    pub project: ProjectRead,
    pub milestones: Vec<TrackerMilestone>,
}

/// A node ID names a record file, so it must be one plain path segment.
pub fn node_id_ok(project_node_id: &str) -> bool {
    !project_node_id.is_empty()
        && project_node_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn cache_path(store: &RootFs, project_node_id: &str) -> Result<std::path::PathBuf, DeliveryError> {
    if !node_id_ok(project_node_id) {
        return Err(DeliveryError::store_unavailable());
    }
    record_path(store, CACHE_DIRNAME, &format!("{project_node_id}.json"))
}

/// GDP-FR-LKXA: the read cache of the Project, or `None`. A record that cannot
/// be parsed, and a record of another Project, read as no cache.
pub fn read_cache<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &RootFs,
    project_node_id: &str,
) -> Result<Option<TrackerRead>, DeliveryError> {
    if !node_id_ok(project_node_id) {
        return Ok(None);
    }
    let path = cache_path(store, project_node_id)?;
    let record: Option<TrackerRead> = read_record(sink, buffer, store, &path, "github_cache")?;
    Ok(record.filter(|r| r.project.node_id == project_node_id))
}

/// GDP-FR-YOSF: replace the read cache of the Project.
pub fn write_cache(
    store: &RootFs,
    project_node_id: &str,
    read: &TrackerRead,
) -> Result<(), DeliveryError> {
    let path = cache_path(store, project_node_id)?;
    write_record(store, &path, read)
}

impl TrackerRead {
    /// The issue of the Project item that holds issue `number` of the
    /// repository.
    pub fn issue(&self, number: u64) -> Option<&TrackerIssue> {
        self.project.items.iter().find_map(|item| match &item.content {
            ItemContent::Issue(i) if i.number == number && self.own(i) => Some(i),
            _ => None,
        })
    }

    /// The Project item ID of issue `number` of the repository.
    pub fn item_id_of(&self, number: u64) -> Option<&str> {
        self.project.items.iter().find_map(|item| match &item.content {
            ItemContent::Issue(i) if i.number == number && self.own(i) => {
                Some(item.item_id.as_str())
            }
            _ => None,
        })
    }

    pub fn milestone(&self, number: u64) -> Option<&TrackerMilestone> {
        self.milestones.iter().find(|m| m.number == number)
    }

    /// Whether the issue is of the bound repository.
    pub fn own(&self, issue: &TrackerIssue) -> bool {
        self.repository.is(&issue.owner, &issue.repo)
    }

    /// Take the values a later read of one issue found.
    pub fn replace_issue(&mut self, issue: TrackerIssue) {
        let repository = self.repository.clone();
        for item in &mut self.project.items {
            if let ItemContent::Issue(i) = &mut item.content {
                if i.number == issue.number && repository.is(&i.owner, &i.repo) {
                    *i = issue.clone();
                }
            }
        }
    }

    /// Add a new Project item that holds `issue`, at the end of the Project.
    pub fn push_issue(&mut self, item_id: String, issue: TrackerIssue) {
        self.project.items.push(ProjectItemRead {
            item_id,
            status: None,
            content: ItemContent::Issue(issue),
        });
    }

    /// GDP-FR-KOWL: take the `Status` a write set on the Project item that
    /// holds issue `number`.
    pub fn set_status(&mut self, number: u64, status: Option<String>) {
        let repository = self.repository.clone();
        for item in &mut self.project.items {
            if let ItemContent::Issue(i) = &item.content {
                if i.number == number && repository.is(&i.owner, &i.repo) {
                    item.status = status.clone();
                }
            }
        }
    }

    /// Remove the Project item that holds issue `number`.
    pub fn remove_issue(&mut self, number: u64) {
        let repository = self.repository.clone();
        self.project.items.retain(|item| match &item.content {
            ItemContent::Issue(i) => !(i.number == number && repository.is(&i.owner, &i.repo)),
            _ => true,
        });
    }

    /// Take the values a later read of one milestone found, or add it.
    pub fn replace_milestone(&mut self, milestone: TrackerMilestone) {
        match self.milestones.iter_mut().find(|m| m.number == milestone.number) {
            Some(m) => *m = milestone,
            None => self.milestones.push(milestone),
        }
    }
}
