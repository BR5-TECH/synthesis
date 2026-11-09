//! The sync itself, pure over the tracker adapter (FTS-FR-ARMD).
//!
//! One call reads the tracker, compares each link field by field with its base
//! (FTS-FR-AIJM), pulls the sub-issues the Flow does not hold (FTS-FR-EGJL),
//! makes every tracker write, and gives back the new document. The caller
//! writes the document once (FTS-FR-VFNM). Nothing here touches a file, a
//! secret, or the network except through the adapter.

use std::collections::{HashMap, HashSet};

use crate::flow_validation::model::{FlowDocument, Item, RefKind, Sync as FlowSync, SyncLink};
use crate::project_settings::FlowTrackerSettings;

use super::adapter::{
    IssueChange, MilestoneChange, TrackerAdapter, TrackerIssue, TrackerMilestone,
};
use super::records::*;
use super::values::{self, Repo};

/// The most numbers one read asks for. The GitHub adapter splits further
/// where its query needs it.
const READ_BATCH: usize = 50;

/// What a sync or a resolve is asked to do.
pub struct SyncRequest<'a> {
    pub flow_id: &'a str,
    pub doc: FlowDocument,
    pub settings: &'a FlowTrackerSettings,
    /// FTS-FR-ARHM: empty for a sync.
    pub resolutions: &'a [ConflictResolution],
}

/// What a sync or a resolve did.
#[derive(Debug)]
pub struct SyncOutcome {
    pub doc: FlowDocument,
    /// FTS-FR-VFNM: whether the document differs from the one read.
    pub doc_changed: bool,
    pub result: SyncResult,
    /// FTS-FR-AIDI: the URL of each tracker item the call created.
    pub created_urls: Vec<String>,
    /// The code of the tracker write that failed. The writes after it were not
    /// made, and the document holds the links of the items created before it.
    pub write_error: Option<String>,
}

/// One planned tracker write, and the base field it brings up to date once it
/// is made.
#[derive(Clone, Debug)]
pub(super) enum Write {
    Issue {
        item_id: String,
        number: u64,
        change: IssueChange,
    },
    Parent {
        item_id: String,
        number: u64,
        current: Option<u64>,
        parent: Option<crate::flow_validation::model::TrackerRef>,
    },
    Milestone {
        item_id: String,
        number: u64,
        milestone: Option<u64>,
    },
    AddBlocker {
        item_id: String,
        number: u64,
        blocker: crate::flow_validation::model::TrackerRef,
    },
    RemoveBlocker {
        item_id: String,
        number: u64,
        blocker: crate::flow_validation::model::TrackerRef,
    },
    MilestoneFields {
        item_id: String,
        number: u64,
        change: MilestoneChange,
    },
}

/// The working state of one call.
pub(super) struct Sync<'a> {
    pub adapter: &'a dyn TrackerAdapter,
    pub flow_id: &'a str,
    pub settings: &'a FlowTrackerSettings,
    pub repo: Repo,
    pub original: FlowDocument,
    pub doc: FlowDocument,
    pub resolutions: HashMap<String, Choice>,
    pub used: HashSet<String>,
    pub result: SyncResult,
    pub issues: HashMap<u64, TrackerIssue>,
    pub milestones: HashMap<u64, TrackerMilestone>,
    /// The numbers the read found no item for.
    pub missing_issues: HashSet<u64>,
    pub missing_milestones: HashSet<u64>,
    pub writes: Vec<Write>,
    /// Items the call changed on the Flow side.
    pub changed_items: HashSet<String>,
    /// Items the call added to the Flow by a pull or a restore.
    pub added_items: HashSet<String>,
    /// Items whose link the call removed: no issue is created for them in the
    /// same call.
    pub no_create: HashSet<String>,
    /// Items with a tracker write that was made, other than a creation.
    pub updated_items: HashSet<String>,
    pub created_items: Vec<String>,
    pub created_urls: Vec<String>,
    pub issue_types: Option<Vec<String>>,
    /// Issue numbers whose link the call removed: no pull of the same call
    /// brings them back.
    pub released: HashSet<u64>,
    /// The issues of links that wait for the author to decide a removal
    /// (FTS-FR-BMWI, FTS-FR-VXMN): no relation to them changes meanwhile.
    pub pending: HashSet<values::RefId>,
}

/// FTS-FR-LTMW to FTS-FR-LVTU: run one sync, or one resolve where `request`
/// carries resolutions. A read that fails returns its code before any write.
pub fn run(
    adapter: &dyn TrackerAdapter,
    request: SyncRequest<'_>,
) -> Result<SyncOutcome, FlowSyncError> {
    if request.doc.sync.is_none() {
        return Err(FlowSyncError::of(ERR_SYNC_NOT_ENABLED));
    }
    let (host, owner, repo) = adapter.repository();
    let repo = Repo { host: host.to_string(), owner: owner.to_string(), repo: repo.to_string() };
    let mut sync = Sync {
        adapter,
        flow_id: request.flow_id,
        settings: request.settings,
        repo,
        original: request.doc.clone(),
        doc: request.doc,
        resolutions: request
            .resolutions
            .iter()
            .map(|r| (r.conflict_id.clone(), r.choice))
            .collect(),
        used: HashSet::new(),
        result: SyncResult::default(),
        issues: HashMap::new(),
        milestones: HashMap::new(),
        missing_issues: HashSet::new(),
        missing_milestones: HashSet::new(),
        writes: Vec::new(),
        changed_items: HashSet::new(),
        added_items: HashSet::new(),
        no_create: HashSet::new(),
        updated_items: HashSet::new(),
        created_items: Vec::new(),
        created_urls: Vec::new(),
        issue_types: None,
        released: HashSet::new(),
        pending: HashSet::new(),
    };
    sync.read_linked().map_err(FlowSyncError::of)?;
    sync.removed_pass();
    sync.pending = sync.pending_links();
    sync.pull_all().map_err(FlowSyncError::of)?;
    sync.merge_pass();
    if sync.needs_issue_types() {
        sync.issue_types = Some(sync.adapter.list_issue_types().map_err(FlowSyncError::of)?);
    }
    // A guard: the plan the merge made must still be a plan before anything
    // is written to the tracker (PST-FR-28 refuses it later otherwise).
    let report = crate::flow_validation::validate(&super::serialize::write_flow(&sync.doc));
    if !report.valid {
        return Err(FlowSyncError::invalid(report));
    }
    let write_error = sync.write_all().err();
    Ok(sync.finish(write_error))
}

impl Sync<'_> {
    /// FTS-FR-PNMA: read every linked item of the publication repository, in
    /// batches (FTS NFR).
    fn read_linked(&mut self) -> Result<(), String> {
        let mut issue_numbers = Vec::new();
        let mut milestone_numbers = Vec::new();
        for link in self.links() {
            if !self.repo.holds(&link.reference) {
                continue;
            }
            match link.reference.kind {
                RefKind::Issue => issue_numbers.push(link.reference.number),
                RefKind::Milestone => milestone_numbers.push(link.reference.number),
            }
        }
        self.read_issues(&issue_numbers)?;
        for chunk in milestone_numbers.chunks(READ_BATCH) {
            for (number, found) in self.adapter.read_milestones(chunk)? {
                match found {
                    Some(m) => {
                        self.milestones.insert(number, m);
                    }
                    None => {
                        self.missing_milestones.insert(number);
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn read_issues(&mut self, numbers: &[u64]) -> Result<(), String> {
        for chunk in numbers.chunks(READ_BATCH) {
            for (number, found) in self.adapter.read_issues(chunk)? {
                // FTS-FR-VXMN: an issue moved to another repository is gone.
                match found.filter(|i| {
                    i.owner.eq_ignore_ascii_case(&self.repo.owner)
                        && i.repo.eq_ignore_ascii_case(&self.repo.repo)
                }) {
                    Some(i) => {
                        self.issues.insert(number, i);
                    }
                    None => {
                        self.missing_issues.insert(number);
                    }
                }
            }
        }
        Ok(())
    }

    /// The links of the working document.
    pub(super) fn links(&self) -> &[SyncLink] {
        self.doc.sync.as_ref().map(|s| s.links.as_slice()).unwrap_or(&[])
    }

    pub(super) fn sync_mut(&mut self) -> &mut FlowSync {
        self.doc.sync.get_or_insert_with(|| FlowSync { tracker: "github".into(), links: vec![] })
    }

    pub(super) fn link(&self, item_id: &str) -> Option<&SyncLink> {
        self.links().iter().find(|l| l.item_id == item_id)
    }

    pub(super) fn link_mut(&mut self, item_id: &str) -> Option<&mut SyncLink> {
        self.sync_mut().links.iter_mut().find(|l| l.item_id == item_id)
    }

    pub(super) fn remove_link(&mut self, item_id: &str) {
        let released: Vec<u64> = self
            .links()
            .iter()
            .filter(|l| l.item_id == item_id && l.reference.kind == RefKind::Issue)
            .map(|l| l.reference.number)
            .collect();
        self.released.extend(released);
        self.sync_mut().links.retain(|l| l.item_id != item_id);
    }

    /// The own issue links whose item is gone from the Flow or from the
    /// tracker, after the removals of this call.
    fn pending_links(&self) -> HashSet<values::RefId> {
        self.links()
            .iter()
            .filter(|l| l.reference.kind == RefKind::Issue && self.repo.holds(&l.reference))
            .filter(|l| {
                self.item(&l.item_id).is_none() || self.missing_issues.contains(&l.reference.number)
            })
            .map(|l| values::ref_id(&l.reference))
            .collect()
    }

    pub(super) fn item(&self, id: &str) -> Option<&Item> {
        self.doc.items.iter().find(|i| i.id == id)
    }

    pub(super) fn item_mut(&mut self, id: &str) -> Option<&mut Item> {
        self.doc.items.iter_mut().find(|i| i.id == id)
    }

    /// The item an own link of `kind` with `number` names.
    pub(super) fn item_linked_to(&self, kind: RefKind, number: u64) -> Option<String> {
        self.links()
            .iter()
            .find(|l| {
                l.reference.kind == kind
                    && l.reference.number == number
                    && self.repo.holds(&l.reference)
                    && self.item(&l.item_id).is_some()
            })
            .map(|l| l.item_id.clone())
    }

    /// FTS-FR-VEEG: the id of a conflict, the same in every sync for the same
    /// item, field, and values.
    pub(super) fn conflict_id(
        &self,
        item_id: &str,
        kind: ConflictKind,
        field: &str,
        flow: &str,
        tracker: &str,
    ) -> String {
        let text = [self.flow_id, item_id, kind.as_str(), field, flow, tracker].join("\u{1f}");
        let hash = crate::fs::sha256_bytes(text.as_bytes());
        format!("fsc_{}", &hash[..16])
    }

    /// The choice the author made for a conflict of this call, where the
    /// conflict offers it. A choice is used once it matches.
    pub(super) fn choice_for(&mut self, conflict: &SyncConflict) -> Option<Choice> {
        let choice = *self.resolutions.get(&conflict.id)?;
        if !conflict.choices.contains(&choice) {
            return None;
        }
        self.used.insert(conflict.id.clone());
        Some(choice)
    }

    pub(super) fn skip(
        &mut self,
        number: Option<u64>,
        title: &str,
        reason: &'static str,
        item_id: Option<&str>,
    ) {
        let entry = SkippedIssue {
            number,
            title: title.to_string(),
            reason,
            item_id: item_id.map(str::to_string),
        };
        if !self.result.skipped.contains(&entry) {
            self.result.skipped.push(entry);
        }
    }

    pub(super) fn mark_changed(&mut self, item_id: &str) {
        if !self.added_items.contains(item_id) {
            self.changed_items.insert(item_id.to_string());
        }
    }

    /// Whether a planned write or a creation names an issue Type, so the
    /// Types the repository offers must be known (FTS-FR-XITX).
    fn needs_issue_types(&self) -> bool {
        let typed_write = self.writes.iter().any(
            |w| matches!(w, Write::Issue { change, .. } if matches!(change.issue_type, Some(Some(_)))),
        );
        typed_write
            || self.creation_order().iter().any(|id| {
                self.item(id)
                    .is_some_and(|i| i.kind != crate::flow_validation::model::Kind::Milestone)
            })
    }

    /// The spelling the repository uses for `name`, where it offers it.
    pub(super) fn offered_type(&self, name: &str) -> Option<String> {
        self.issue_types.as_ref()?.iter().find(|t| t.eq_ignore_ascii_case(name.trim())).cloned()
    }

    fn finish(mut self, write_error: Option<String>) -> SyncOutcome {
        self.result.created = self.created_items.len();
        self.result.updated =
            self.updated_items.iter().filter(|id| !self.created_items.contains(id)).count();
        self.result.added = self.added_items.len();
        self.result.changed = self.changed_items.len();
        let mut stale: Vec<String> =
            self.resolutions.keys().filter(|id| !self.used.contains(*id)).cloned().collect();
        stale.sort();
        self.result.stale = stale;
        let doc_changed = self.doc != self.original;
        SyncOutcome {
            doc: self.doc,
            doc_changed,
            result: self.result,
            created_urls: self.created_urls,
            write_error,
        }
    }
}

/// FTS-FR-VRRJ: an item's description as the Flow stores it: absent when
/// empty.
pub(super) fn flow_description(text: &str) -> Option<String> {
    let text = values::normalize_description(text);
    (!text.is_empty()).then_some(text)
}
