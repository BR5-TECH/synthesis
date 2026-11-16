//! The edits of a GitHub delivery plan (`GDP-github-delivery-plan.md`
//! GDP-FR-VNNU to GDP-FR-IRDD).
//!
//! A tracker edit goes to GitHub at once, through the tracker adapter. Before
//! it writes, it reads each tracker item it changes and refuses where the item
//! changed on the tracker (GDP-FR-GBDF). After it writes, it reads each changed
//! item again and updates the read cache (GDP-FR-VNNU). An overlay edit writes
//! the per-machine overlay alone and makes no GitHub request (GDP-FR-YBHD).

use crate::project_scope::ProjectStores;
use std::time::Instant;

use serde::Deserialize;
use tauri::{AppHandle, Runtime};

use crate::delivery::machine_state::{
    read_delivery_overlay, write_delivery_overlay, DeliveryOverlay, RelatesEdge,
};
use crate::delivery::settings::{DeliverySettings, IssueKind, TypeMapping};
use crate::delivery::DeliveryError;
use crate::flow_validation::model::{EdgeType, Hierarchy, Kind};
use crate::logging::LogLevel;

use super::adapter::*;
use super::cache::TrackerRead;
use super::plan::{build_plan, issue_id, milestone_id, parse_item_id, BuiltPlan, PlanRef};
use super::*;

mod extra;
mod rows;

pub(crate) use rows::{apply_row, IssueChangeRow, NewIssueRow, RowWrite};

/// GDP-FR-MVAA: the reason `close_item` closes an issue with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    Completed,
    NotPlanned,
}

/// GDP-FR-WAKG: how `remove_item` removes an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoveMode {
    RemoveFromBoard,
    CloseNotPlanned,
}

/// GDP contract surface `GithubDeliveryEdit`, in the frontend shape of
/// `src/types/delivery.ts`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum GithubDeliveryEdit {
    CreateItem {
        item_kind: Kind,
        name: String,
        container_id: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        due: Option<String>,
        /// GDP-FR-RMNF: the parent issue item. Where given, it replaces the
        /// container rule for the parent.
        #[serde(default)]
        parent_id: Option<String>,
        /// GDP-FR-RMNF: the milestone item. Where given, it replaces the
        /// container rule for the milestone.
        #[serde(default)]
        milestone_id: Option<String>,
        /// GDP-FR-RMNF: names of existing repository labels.
        #[serde(default)]
        labels: Vec<String>,
    },
    Rename {
        item_id: String,
        name: String,
        updated_at: String,
    },
    SetDescription {
        item_id: String,
        description: String,
        updated_at: String,
    },
    SetDue {
        item_id: String,
        due: Option<String>,
        updated_at: String,
    },
    SetKind {
        item_id: String,
        item_kind: Kind,
        updated_at: String,
    },
    Move {
        item_id: String,
        container_id: Option<String>,
        updated_at: String,
    },
    AddEdge {
        from: String,
        to: String,
        #[serde(rename = "type")]
        edge_type: EdgeType,
        updated_at: String,
    },
    RemoveEdge {
        from: String,
        to: String,
        #[serde(rename = "type")]
        edge_type: EdgeType,
        updated_at: String,
    },
    RemoveItem {
        item_id: String,
        mode: RemoveMode,
        updated_at: String,
    },
    AddExistingIssue {
        number: u64,
        container_id: Option<String>,
    },
    /// GDP-FR-ICDW.
    SetLabels {
        item_id: String,
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
        updated_at: String,
    },
    /// GDP-FR-MVAA.
    CloseItem {
        item_id: String,
        reason: CloseReason,
        updated_at: String,
    },
    /// GDP-FR-KOWL.
    SetStatus {
        item_id: String,
        status: String,
        updated_at: String,
    },
    /// GDP-FR-RMNF: `parent_id` is an issue item id, or `None` to remove the
    /// parent.
    SetParent {
        item_id: String,
        parent_id: Option<String>,
        updated_at: String,
    },
    /// GDP-FR-RMNF: `milestone_id` is a milestone item id, or `None` to remove
    /// the milestone.
    SetMilestone {
        item_id: String,
        milestone_id: Option<String>,
        updated_at: String,
    },
    SetReferences {
        item_id: String,
        artifact_ids: Vec<String>,
    },
    SetFootprint {
        item_id: String,
        footprint: Vec<String>,
    },
    LinkDraft {
        item_id: String,
        draft_id: String,
    },
    UnlinkDraft {
        item_id: String,
    },
    SetPlanReferences {
        references: Vec<String>,
    },
    /// GDP-FR-XJDE, GDP-FR-YBHD: the milestone item ids, first to last.
    SetMilestoneOrder {
        milestone_ids: Vec<String>,
    },
}

impl GithubDeliveryEdit {
    /// The name of the edit for a log record.
    pub fn name(&self) -> &'static str {
        match self {
            Self::CreateItem { .. } => "create_item",
            Self::Rename { .. } => "rename",
            Self::SetDescription { .. } => "set_description",
            Self::SetDue { .. } => "set_due",
            Self::SetKind { .. } => "set_kind",
            Self::Move { .. } => "move",
            Self::AddEdge { .. } => "add_edge",
            Self::RemoveEdge { .. } => "remove_edge",
            Self::RemoveItem { .. } => "remove_item",
            Self::AddExistingIssue { .. } => "add_existing_issue",
            Self::SetLabels { .. } => "set_labels",
            Self::CloseItem { .. } => "close_item",
            Self::SetStatus { .. } => "set_status",
            Self::SetParent { .. } => "set_parent",
            Self::SetMilestone { .. } => "set_milestone",
            Self::SetReferences { .. } => "set_references",
            Self::SetFootprint { .. } => "set_footprint",
            Self::LinkDraft { .. } => "link_draft",
            Self::UnlinkDraft { .. } => "unlink_draft",
            Self::SetPlanReferences { .. } => "set_plan_references",
            Self::SetMilestoneOrder { .. } => "set_milestone_order",
        }
    }

    /// GDP-FR-YBHD: whether the edit writes the overlay alone.
    pub fn is_overlay(&self) -> bool {
        match self {
            Self::AddEdge { edge_type, .. } | Self::RemoveEdge { edge_type, .. } => {
                *edge_type == EdgeType::Relates
            }
            Self::SetReferences { .. }
            | Self::SetFootprint { .. }
            | Self::LinkDraft { .. }
            | Self::UnlinkDraft { .. }
            | Self::SetPlanReferences { .. }
            | Self::SetMilestoneOrder { .. } => true,
            _ => false,
        }
    }
}

fn refusal(code: &str, message: &str) -> DeliveryError {
    DeliveryError::new(code, message)
}

fn not_found() -> DeliveryError {
    refusal(ERR_TRACKER_ITEM_NOT_FOUND, "The tracker has no such item.")
}

fn changed_on_tracker() -> DeliveryError {
    refusal(ERR_CHANGED_ON_TRACKER, "The item changed on the tracker. Look again and retry.")
}

fn kind_unsupported() -> DeliveryError {
    refusal(ERR_KIND_CHANGE_UNSUPPORTED, "This change is not possible for this kind of item.")
}

/// GDP-FR-YBHD: apply an overlay edit to `overlay`. Pure.
pub fn apply_overlay_edit(overlay: &mut DeliveryOverlay, edit: &GithubDeliveryEdit) {
    let list = |v: &Vec<String>| (!v.is_empty()).then(|| v.clone());
    match edit {
        GithubDeliveryEdit::SetReferences { item_id, artifact_ids } => {
            overlay.items.entry(item_id.clone()).or_default().artifact_ids = list(artifact_ids);
        }
        GithubDeliveryEdit::SetFootprint { item_id, footprint } => {
            overlay.items.entry(item_id.clone()).or_default().footprint = list(footprint);
        }
        GithubDeliveryEdit::LinkDraft { item_id, draft_id } => {
            overlay.items.entry(item_id.clone()).or_default().draft_id = Some(draft_id.clone());
        }
        GithubDeliveryEdit::UnlinkDraft { item_id } => {
            if let Some(entry) = overlay.items.get_mut(item_id) {
                entry.draft_id = None;
            }
        }
        GithubDeliveryEdit::AddEdge { from, to, .. }
            if !overlay.relates.iter().any(|e| e.from == *from && e.to == *to) =>
        {
            overlay.relates.push(RelatesEdge { from: from.clone(), to: to.clone() });
        }
        GithubDeliveryEdit::RemoveEdge { from, to, .. } => {
            overlay.relates.retain(|e| !(e.from == *from && e.to == *to));
        }
        GithubDeliveryEdit::SetPlanReferences { references } => {
            overlay.references = references.clone();
        }
        GithubDeliveryEdit::SetMilestoneOrder { milestone_ids } => {
            overlay.milestone_order = milestone_ids.clone();
        }
        _ => {}
    }
    overlay
        .items
        .retain(|_, e| e.artifact_ids.is_some() || e.footprint.is_some() || e.draft_id.is_some());
}

/// GDP-FR-MSVS: the writes of one edit. A failure after a write is
/// `partial_edit`; a failure of the first write is its own error.
#[derive(Default)]
pub struct Writes {
    pub made: usize,
}

impl Writes {
    pub fn run<T>(&mut self, result: Result<T, String>) -> Result<T, DeliveryError> {
        match result {
            Ok(value) => {
                self.made += 1;
                Ok(value)
            }
            Err(code) if self.made == 0 => Err(github_error(&code)),
            Err(code) => Err(DeliveryError::new(
                ERR_PARTIAL_EDIT,
                format!(
                    "The edit stopped after {} of its writes: {}",
                    self.made,
                    crate::github_polling::records::error_text(&code)
                ),
            )),
        }
    }
}

/// One edit against the tracker, with what it reads and writes.
pub struct TrackerEdit<'a> {
    pub adapter: &'a dyn TrackerAdapter,
    pub project_id: &'a str,
    pub settings: &'a DeliverySettings,
    pub read: &'a mut TrackerRead,
    pub plan: &'a BuiltPlan,
}

/// GDP-FR-NJJZ: the nearest container of `container_id`, or of an ancestor,
/// that is an issue, and the nearest that is a milestone.
fn containers(
    plan: &BuiltPlan,
    container_id: Option<&str>,
) -> Result<(Option<u64>, Option<u64>), DeliveryError> {
    let Some(start) = container_id else { return Ok((None, None)) };
    let hierarchy = Hierarchy::new(&plan.document.items);
    if hierarchy.kind(start).is_none() {
        return Err(not_found());
    }
    let mut chain = vec![start];
    chain.extend(hierarchy.ancestors(start));
    let (mut parent, mut milestone) = (None, None);
    for id in chain {
        match parse_item_id(id) {
            Some(PlanRef::Issue(n)) if parent.is_none() => parent = Some(n),
            Some(PlanRef::Milestone(m)) if milestone.is_none() => milestone = Some(m),
            _ => {}
        }
    }
    Ok((parent, milestone))
}

fn issue_kind(kind: Kind) -> Option<IssueKind> {
    match kind {
        Kind::Epic => Some(IssueKind::Epic),
        Kind::Feature => Some(IssueKind::Feature),
        Kind::Task => Some(IssueKind::Task),
        Kind::Defect => Some(IssueKind::Defect),
        Kind::Milestone => None,
    }
}

/// The values of a `create_item` edit.
struct CreateArgs<'a> {
    kind: Kind,
    name: &'a str,
    container_id: Option<&'a str>,
    description: &'a Option<String>,
    due: &'a Option<String>,
    parent_id: Option<&'a str>,
    milestone_id: Option<&'a str>,
    labels: &'a [String],
}

impl TrackerEdit<'_> {
    /// The plan item `id` names, or `tracker_item_not_found`.
    fn item(&self, id: &str) -> Result<PlanRef, DeliveryError> {
        let known = self.plan.document.items.iter().any(|i| i.id == id);
        parse_item_id(id).filter(|_| known).ok_or_else(not_found)
    }

    /// GDP-FR-GBDF: read the item and refuse where its `updated_at` is not the
    /// value the edit names. The cache takes the values read either way.
    fn check_fresh(&mut self, item: PlanRef, updated_at: &str) -> Result<(), DeliveryError> {
        let stamp = match item {
            PlanRef::Issue(n) => {
                let fresh = self
                    .adapter
                    .read_issue(n)
                    .map_err(|c| github_error(&c))?
                    .ok_or_else(not_found)?;
                let stamp = fresh.updated_at.clone();
                self.read.replace_issue(fresh);
                stamp
            }
            PlanRef::Milestone(m) => {
                let fresh = self
                    .adapter
                    .read_milestone(m)
                    .map_err(|c| github_error(&c))?
                    .ok_or_else(not_found)?;
                let stamp = fresh.updated_at.clone();
                self.read.replace_milestone(fresh);
                stamp
            }
        };
        if stamp != updated_at {
            return Err(changed_on_tracker());
        }
        Ok(())
    }

    /// GDP-FR-VNNU: read changed items again. A read that fails leaves the
    /// cache as it is: the writes are already on GitHub.
    fn refresh(&mut self, items: &[PlanRef]) {
        for item in items {
            match *item {
                PlanRef::Issue(n) => {
                    if let Ok(Some(fresh)) = self.adapter.read_issue(n) {
                        self.read.replace_issue(fresh);
                    }
                }
                PlanRef::Milestone(m) => {
                    if let Ok(Some(fresh)) = self.adapter.read_milestone(m) {
                        self.read.replace_milestone(fresh);
                    }
                }
            }
        }
    }

    /// Run the writes of `body`, then read the changed items again, also
    /// where a write failed (GDP-FR-MSVS).
    fn with_refresh<T>(
        &mut self,
        changed: &[PlanRef],
        body: impl FnOnce(&mut Self, &mut Writes) -> Result<T, DeliveryError>,
    ) -> Result<T, DeliveryError> {
        let mut writes = Writes::default();
        let result = body(self, &mut writes);
        self.refresh(changed);
        result
    }

    /// GDP-FR-UDOL to GDP-FR-HULQ: apply one tracker edit. It returns the id of
    /// a new item.
    pub fn apply(&mut self, edit: &GithubDeliveryEdit) -> Result<Option<String>, DeliveryError> {
        match edit {
            GithubDeliveryEdit::CreateItem {
                item_kind,
                name,
                container_id,
                description,
                due,
                parent_id,
                milestone_id,
                labels,
            } => self
                .create(CreateArgs {
                    kind: *item_kind,
                    name,
                    container_id: container_id.as_deref(),
                    description,
                    due,
                    parent_id: parent_id.as_deref(),
                    milestone_id: milestone_id.as_deref(),
                    labels,
                })
                .map(Some),
            GithubDeliveryEdit::SetLabels { item_id, add, remove, updated_at } => {
                self.set_labels(item_id, add, remove, updated_at).map(|_| None)
            }
            GithubDeliveryEdit::CloseItem { item_id, reason, updated_at } => {
                self.close_item(item_id, *reason, updated_at).map(|_| None)
            }
            GithubDeliveryEdit::SetStatus { item_id, status, updated_at } => {
                self.set_status(item_id, status, updated_at).map(|_| None)
            }
            GithubDeliveryEdit::SetParent { item_id, parent_id, updated_at } => {
                self.set_parent_of(item_id, parent_id.as_deref(), updated_at).map(|_| None)
            }
            GithubDeliveryEdit::SetMilestone { item_id, milestone_id, updated_at } => {
                self.set_milestone_of(item_id, milestone_id.as_deref(), updated_at).map(|_| None)
            }
            GithubDeliveryEdit::Rename { item_id, name, updated_at } => {
                let item = self.item(item_id)?;
                self.check_fresh(item, updated_at)?;
                self.with_refresh(&[item], |e, w| match item {
                    PlanRef::Issue(n) => w.run(e.adapter.update_issue(
                        n,
                        &IssueChange { title: Some(name.clone()), ..Default::default() },
                    )),
                    PlanRef::Milestone(m) => w.run(e.adapter.update_milestone(
                        m,
                        &MilestoneChange { title: Some(name.clone()), ..Default::default() },
                    )),
                })?;
                Ok(None)
            }
            GithubDeliveryEdit::SetDescription { item_id, description, updated_at } => {
                let item = self.item(item_id)?;
                self.check_fresh(item, updated_at)?;
                self.with_refresh(&[item], |e, w| match item {
                    PlanRef::Issue(n) => w.run(e.adapter.update_issue(
                        n,
                        &IssueChange { body: Some(description.clone()), ..Default::default() },
                    )),
                    PlanRef::Milestone(m) => w.run(e.adapter.update_milestone(
                        m,
                        &MilestoneChange {
                            description: Some(description.clone()),
                            ..Default::default()
                        },
                    )),
                })?;
                Ok(None)
            }
            GithubDeliveryEdit::SetDue { item_id, due, updated_at } => {
                let PlanRef::Milestone(m) = self.item(item_id)? else {
                    return Err(kind_unsupported());
                };
                self.check_fresh(PlanRef::Milestone(m), updated_at)?;
                self.with_refresh(&[PlanRef::Milestone(m)], |e, w| {
                    w.run(e.adapter.update_milestone(
                        m,
                        &MilestoneChange { due: Some(due.clone()), ..Default::default() },
                    ))
                })?;
                Ok(None)
            }
            GithubDeliveryEdit::SetKind { item_id, item_kind, updated_at } => {
                // GDP-FR-EJTS: a change to or from `milestone` is refused.
                let PlanRef::Issue(n) = self.item(item_id)? else { return Err(kind_unsupported()) };
                let Some(kind) = issue_kind(*item_kind) else { return Err(kind_unsupported()) };
                self.check_fresh(PlanRef::Issue(n), updated_at)?;
                let type_name = self.settings.create_types.of(kind).to_string();
                self.with_refresh(&[PlanRef::Issue(n)], |e, w| {
                    w.run(e.adapter.update_issue(
                        n,
                        &IssueChange { issue_type: Some(type_name), ..Default::default() },
                    ))
                })?;
                Ok(None)
            }
            GithubDeliveryEdit::Move { item_id, container_id, updated_at } => {
                let PlanRef::Issue(n) = self.item(item_id)? else { return Err(kind_unsupported()) };
                let hierarchy = Hierarchy::new(&self.plan.document.items);
                if container_id
                    .as_deref()
                    .is_some_and(|c| c == item_id || hierarchy.is_ancestor(item_id, c))
                {
                    // An item cannot sit in itself or in one of its members.
                    return Err(kind_unsupported());
                }
                let (parent, milestone) = containers(self.plan, container_id.as_deref())?;
                self.check_fresh(PlanRef::Issue(n), updated_at)?;
                let old_parent =
                    self.read.issue(n).and_then(|i| i.parent.as_ref()).map(|p| p.number);
                let mut changed = vec![PlanRef::Issue(n)];
                changed.extend(parent.into_iter().chain(old_parent).map(PlanRef::Issue));
                self.with_refresh(&changed, |e, w| e.place(n, parent, milestone, w))?;
                Ok(None)
            }
            GithubDeliveryEdit::AddEdge { from, to, updated_at, .. }
            | GithubDeliveryEdit::RemoveEdge { from, to, updated_at, .. } => {
                let (blocking, blocked) = (self.item(from)?, self.item(to)?);
                let (PlanRef::Issue(blocking), PlanRef::Issue(blocked)) = (blocking, blocked)
                else {
                    return Err(refusal(
                        ERR_MILESTONE_EDGE_UNSUPPORTED,
                        "A milestone cannot block or be blocked.",
                    ));
                };
                self.check_fresh(PlanRef::Issue(blocked), updated_at)?;
                let add = matches!(edit, GithubDeliveryEdit::AddEdge { .. });
                self.with_refresh(&[PlanRef::Issue(blocked)], |e, w| match add {
                    true => w.run(e.adapter.add_blocked_by(blocked, blocking)),
                    false => w.run(e.adapter.remove_blocked_by(blocked, blocking)),
                })?;
                Ok(None)
            }
            GithubDeliveryEdit::RemoveItem { item_id, mode, updated_at } => {
                let item = self.item(item_id)?;
                self.check_fresh(item, updated_at)?;
                self.remove(item_id, item, *mode)?;
                Ok(None)
            }
            GithubDeliveryEdit::AddExistingIssue { number, container_id } => {
                self.add_existing(*number, container_id.as_deref()).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// GDP-FR-NJJZ: set the issue's parent and milestone to the ones given,
    /// where they differ from the cache.
    fn place(
        &mut self,
        number: u64,
        parent: Option<u64>,
        milestone: Option<u64>,
        writes: &mut Writes,
    ) -> Result<(), DeliveryError> {
        let issue = self.read.issue(number).cloned().ok_or_else(not_found)?;
        let own_parent = issue
            .parent
            .as_ref()
            .filter(|p| self.read.repository.is(&p.owner, &p.repo))
            .map(|p| p.number);
        if own_parent != parent || (parent.is_none() && issue.parent.is_some()) {
            writes.run(self.adapter.set_parent(number, issue.parent.as_ref(), parent))?;
        }
        if issue.milestone != milestone {
            writes.run(self.adapter.set_milestone(number, milestone))?;
        }
        Ok(())
    }

    /// GDP-FR-UDOL, GDP-FR-CNVO, GDP-FR-RMNF: create an item and return its id.
    fn create(&mut self, args: CreateArgs<'_>) -> Result<String, DeliveryError> {
        let CreateArgs {
            kind,
            name,
            container_id,
            description,
            due,
            parent_id,
            milestone_id: milestone_item,
            labels,
        } = args;
        let Some(issue_kind) = issue_kind(kind) else {
            if container_id.is_some()
                || parent_id.is_some()
                || milestone_item.is_some()
                || !labels.is_empty()
            {
                return Err(kind_unsupported());
            }
            let created = self
                .adapter
                .create_milestone(&NewMilestone {
                    title: name.to_string(),
                    description: description.clone().unwrap_or_default(),
                    due: due.clone(),
                })
                .map_err(|c| github_error(&c))?;
            let id = milestone_id(created.number);
            self.read.replace_milestone(created);
            return Ok(id);
        };
        let types = self.adapter.list_issue_types().map_err(|c| github_error(&c))?;
        let wanted = self.settings.create_types.of(issue_kind).to_string();
        let issue_type = if types.is_empty() {
            // A repository with no Types takes an untyped issue, for the kind of
            // the untyped choice alone.
            if self.settings.untyped.and_then(|u| u.kind()) != Some(issue_kind) {
                return Err(refusal(
                    ERR_ISSUE_TYPE_UNAVAILABLE,
                    "The repository offers no issue Type for this kind.",
                ));
            }
            None
        } else if types.contains(&wanted) {
            Some(wanted)
        } else {
            return Err(refusal(
                ERR_ISSUE_TYPE_UNAVAILABLE,
                "The repository does not offer the create Type of this kind.",
            ));
        };
        let (parent, milestone) = self.places(container_id, parent_id, milestone_item)?;
        let labels = self.canonical_labels(labels)?;
        self.write_new_issue(
            NewIssue {
                title: name.to_string(),
                body: description.clone().unwrap_or_default(),
                issue_type,
                milestone,
                labels,
            },
            parent,
        )
        .map(|(number, _)| issue_id(number))
    }

    /// GDP-FR-UDOL, GDP-FR-MSVS: create the issue, add it to the Project, and set
    /// its parent. It returns the number and the URL of the issue.
    fn write_new_issue(
        &mut self,
        new: NewIssue,
        parent: Option<u64>,
    ) -> Result<(u64, String), DeliveryError> {
        let mut writes = Writes::default();
        let created = writes.run(self.adapter.create_issue(&new))?;
        let number = created.number;
        let node_id = created.node_id.clone();
        let url = created.url.clone();
        let result = (|| {
            let item_id = writes.run(self.adapter.add_project_item(self.project_id, &node_id))?;
            self.read.push_issue(item_id, created);
            if let Some(parent) = parent {
                writes.run(self.adapter.set_parent(number, None, Some(parent)))?;
            }
            Ok(())
        })();
        let mut changed = vec![PlanRef::Issue(number)];
        changed.extend(parent.map(PlanRef::Issue));
        self.refresh(&changed);
        // GDP-FR-MSVS, DSR-FR-ULON: a partial create names the issue it made,
        // so a publish journals it and a retry does not make it twice.
        result.map(|_| (number, url.clone())).map_err(|mut e: DeliveryError| {
            e.created_urls = Some(vec![url]);
            e
        })
    }

    /// GDP-FR-WAKG: remove an item and each plan issue under it.
    fn remove(
        &mut self,
        item_id: &str,
        item: PlanRef,
        mode: RemoveMode,
    ) -> Result<(), DeliveryError> {
        let hierarchy = Hierarchy::new(&self.plan.document.items);
        let mut members: Vec<(usize, usize, u64)> = self
            .plan
            .document
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.id == item_id || hierarchy.is_ancestor(item_id, &i.id))
            .filter_map(|(at, i)| match parse_item_id(&i.id) {
                Some(PlanRef::Issue(n)) => Some((hierarchy.ancestors(&i.id).len(), at, n)),
                _ => None,
            })
            .collect();
        // Members first, so a parent never goes before its members.
        members.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        let issues: Vec<u64> = members.into_iter().map(|(_, _, n)| n).collect();
        // GDP-FR-GBDF: the edit also changes each member, so each member is
        // read before the first write. A member whose `updated_at` is not the
        // value of the plan the edit was made against refuses the edit, and
        // nothing is written. The cache takes the values read either way.
        for n in issues.iter().filter(|n| PlanRef::Issue(**n) != item) {
            let known = self.read.issue(*n).map(|i| i.updated_at.clone());
            let fresh =
                self.adapter.read_issue(*n).map_err(|c| github_error(&c))?.ok_or_else(not_found)?;
            let stamp = fresh.updated_at.clone();
            self.read.replace_issue(fresh);
            if known.as_deref() != Some(stamp.as_str()) {
                return Err(changed_on_tracker());
            }
        }
        let mut writes = Writes::default();
        let mut changed: Vec<PlanRef> = Vec::new();
        let result = (|| {
            for n in &issues {
                match mode {
                    RemoveMode::RemoveFromBoard => {
                        let Some(project_item) = self.read.item_id_of(*n).map(str::to_string)
                        else {
                            continue;
                        };
                        writes.run(
                            self.adapter.remove_project_item(self.project_id, &project_item),
                        )?;
                        self.read.remove_issue(*n);
                    }
                    RemoveMode::CloseNotPlanned => {
                        writes.run(self.adapter.close_issue(*n, CloseWith::NotPlanned))?;
                        changed.push(PlanRef::Issue(*n));
                    }
                }
            }
            if let PlanRef::Milestone(m) = item {
                writes.run(self.adapter.close_milestone(m))?;
                changed.push(item);
            }
            Ok(())
        })();
        self.refresh(&changed);
        result
    }

    /// GDP-FR-HULQ: add an issue of the repository to the Project, then move it
    /// to the container.
    fn add_existing(
        &mut self,
        number: u64,
        container_id: Option<&str>,
    ) -> Result<String, DeliveryError> {
        let issue =
            self.adapter.read_issue(number).map_err(|c| github_error(&c))?.ok_or_else(not_found)?;
        if !self.read.repository.is(&issue.owner, &issue.repo) {
            return Err(not_found());
        }
        let id = issue_id(number);
        if self.plan.document.items.iter().any(|i| i.id == id) {
            return Err(refusal(ERR_ALREADY_IN_PLAN, "The issue is already part of the plan."));
        }
        if !matches!(self.settings.map_type(issue.issue_type.as_deref()), TypeMapping::Kind(_)) {
            return Err(refusal(
                ERR_TYPE_UNMAPPED,
                "The Type of the issue maps to no kind of the plan.",
            ));
        }
        let (parent, milestone) = containers(self.plan, container_id)?;
        let mut writes = Writes::default();
        let result = (|| {
            if self.read.item_id_of(number).is_none() {
                let item_id =
                    writes.run(self.adapter.add_project_item(self.project_id, &issue.node_id))?;
                self.read.push_issue(item_id, issue.clone());
            }
            self.place(number, parent, milestone, &mut writes)
        })();
        self.refresh(&[PlanRef::Issue(number)]);
        result.map(|_| id)
    }
}

/// GDP-FR-VNNU, GDP-FR-YBHD, GDP-FR-IRDD: apply one edit for the target. The
/// caller holds the turn of the edit queue. It returns the plan and the id of
/// a new item.
pub(crate) fn apply_for_target<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    edit: &GithubDeliveryEdit,
) -> Result<(GithubDeliveryPlan, Option<String>), DeliveryError> {
    let store = machine_store(app)?;
    if edit.is_overlay() {
        let mut overlay =
            read_delivery_overlay(app, buffer_of(app), &store, &target.project_node_id)?;
        apply_overlay_edit(&mut overlay, edit);
        write_delivery_overlay(app, buffer_of(app), &store, &target.project_node_id, &overlay)?;
        return Ok((load_target(app, target, false)?, None));
    }
    let mut read = match cached(app, target)? {
        Some(read) => read,
        None => shared_read(app, target)?,
    };
    let overlay = read_delivery_overlay(app, buffer_of(app), &store, &target.project_node_id)?;
    let plan = build_plan(&read, &target.settings, &overlay, &target.statuses);
    let adapter = connect(app)?;
    let before = read.clone();
    let result = TrackerEdit {
        adapter: adapter.as_ref(),
        project_id: &target.project_node_id,
        settings: &target.settings,
        read: &mut read,
        plan: &plan,
    }
    .apply(edit);
    if read != before {
        store_edit(app, target, &read)?;
    }
    let new_id = result?;
    Ok((answer(app, target, &read, false, None)?, new_id))
}

/// GDP-FR-VNNU, GDP-FR-QHQR: `apply_github_delivery_edit(edit)` on the calling
/// thread.
pub fn apply_edit_impl<R: Runtime>(
    app: &AppHandle<R>,
    edit: GithubDeliveryEdit,
) -> Result<GithubDeliveryPlan, DeliveryError> {
    let started = Instant::now();
    let name = edit.name();
    log(
        app,
        LogLevel::Debug,
        "github delivery edit started",
        crate::log_fields! { "edit" => name },
    );
    let result = delivery_target(app).and_then(|target| {
        let state = app.project_store::<GithubDeliveryState>();
        let _turn = state.edits.enter();
        apply_for_target(app, &target, &edit)
    });
    match &result {
        Ok(_) => log(
            app,
            LogLevel::Info,
            "github delivery edit applied",
            crate::log_fields! { "edit" => name, "ms" => started.elapsed().as_millis() as u64 },
        ),
        Err(e) => log(
            app,
            LogLevel::Warn,
            "github delivery edit refused",
            crate::log_fields! { "edit" => name, "code" => &e.code, "ms" => started.elapsed().as_millis() as u64 },
        ),
    }
    result.map(|(plan, _)| plan)
}
