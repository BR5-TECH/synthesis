//! `FDL-flow-delivery.md`: how far along a delivery plan is.
//!
//! The state of every item comes from the work itself — the drafts the tasks are
//! worked in, their graduation runs, and the tracker state a sync stored — and
//! never from a status the author keeps by hand. This module only reads: a
//! blocked item is reported, and nothing here starts, stops, or refuses a run
//! (FDL intent).

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;
use tauri::State;

use crate::drafts::{DraftStatus, DraftSummary};
use crate::flow_validation::model::{parse_valid, EdgeType, FlowDocument, Hierarchy, Item};
use crate::flow_validation::ValidationReport;
use crate::graduation::GraduationRunState;
use crate::logging::{log_info, log_warn, Domain, BUFFER};
use crate::project::ProjectState;

/// The typed error of both operations (FDL-FR-VTOQ, FDL-FR-AZQM).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FlowReadError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ValidationReport>,
}

impl FlowReadError {
    pub fn no_project() -> Self {
        Self { code: "no_project_open", message: "No project is open.".into(), report: None }
    }

    pub fn invalid(report: ValidationReport) -> Self {
        Self {
            code: "flow_invalid",
            message: "This Flow is not a valid delivery plan.".into(),
            report: Some(report),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    NotStarted,
    Drafting,
    InProgress,
    Done,
    Dropped,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRef {
    pub id: String,
    pub name: String,
    pub status: DraftStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRef {
    pub run_id: String,
    pub state: GraduationRunState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeafCounts {
    pub not_started: usize,
    pub drafting: usize,
    pub in_progress: usize,
    pub done: usize,
    pub dropped: usize,
}

impl LeafCounts {
    fn add(&mut self, state: ItemState) {
        match state {
            ItemState::NotStarted => self.not_started += 1,
            ItemState::Drafting => self.drafting += 1,
            ItemState::InProgress => self.in_progress += 1,
            ItemState::Done => self.done += 1,
            ItemState::Dropped => self.dropped += 1,
        }
    }
}

/// One item's delivery state (the contract surface's `ItemDeliveryState`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeliveryState {
    pub item_id: String,
    pub state: ItemState,
    pub draft: Option<DraftRef>,
    pub draft_missing: bool,
    pub run: Option<RunRef>,
    pub blocked: bool,
    pub blocked_by: Vec<String>,
    pub leaf_counts: Option<LeafCounts>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDeliveryState {
    pub items: Vec<ItemDeliveryState>,
}

/// A run state that is not over: the draft is in progress (FDL-FR-VGZI).
fn run_is_open(state: GraduationRunState) -> bool {
    !matches!(
        state,
        GraduationRunState::Completed | GraduationRunState::Discarded | GraduationRunState::Failed
    )
}

/// FDL-FR-TTZZ: the draft a leaf item is worked in, and whether a `draftId`
/// names no draft (FDL-FR-BRAQ).
fn leaf_draft<'d>(
    doc: &FlowDocument,
    item: &Item,
    drafts: &'d HashMap<&str, &DraftSummary>,
) -> (Option<&'d DraftSummary>, bool) {
    if let Some(id) = item.draft_id.as_deref() {
        return match drafts.get(id) {
            Some(d) => (Some(*d), false),
            None => (None, true),
        };
    }
    let link = doc.sync.as_ref().and_then(|s| s.links.iter().find(|l| l.item_id == item.id));
    let Some(link) = link else { return (None, false) };
    let r = &link.reference;
    let shadow = drafts.values().find(|d| {
        d.github_issue.as_ref().is_some_and(|g| {
            g.repository_host.eq_ignore_ascii_case(&r.host)
                && g.repository_owner.eq_ignore_ascii_case(&r.owner)
                && g.repository_name.eq_ignore_ascii_case(&r.repo)
                && g.issue_number == r.number
        })
    });
    (shadow.copied(), false)
}

/// FDL-FR-VGZI, FDL-FR-JYKC: whether the tracker state stored at the last sync
/// is `closed`.
fn closed_on_tracker(doc: &FlowDocument, item_id: &str) -> bool {
    doc.sync
        .as_ref()
        .and_then(|s| s.links.iter().find(|l| l.item_id == item_id))
        .is_some_and(|l| l.base.state == "closed")
}

/// FDL-FR-VGZI: a leaf's state from its tracker state, its draft, and its run.
fn leaf_state(closed: bool, draft: Option<&DraftSummary>) -> ItemState {
    if closed {
        return ItemState::Done;
    }
    let Some(d) = draft else { return ItemState::NotStarted };
    match d.status {
        DraftStatus::Archived => ItemState::Dropped,
        DraftStatus::Graduated => ItemState::Done,
        _ if d.graduation.as_ref().is_some_and(|g| run_is_open(g.state)) => ItemState::InProgress,
        _ => ItemState::Drafting,
    }
}

/// FDL-FR-JYKC: a container's state from the leaves under it.
fn container_state(closed: bool, leaves: &[ItemState]) -> ItemState {
    if closed {
        return ItemState::Done;
    }
    if leaves.is_empty() {
        return ItemState::NotStarted;
    }
    if leaves.iter().all(|s| *s == ItemState::Dropped) {
        return ItemState::Dropped;
    }
    if leaves.iter().all(|s| matches!(s, ItemState::Done | ItemState::Dropped)) {
        return ItemState::Done;
    }
    if leaves
        .iter()
        .any(|s| matches!(s, ItemState::Drafting | ItemState::InProgress | ItemState::Done))
    {
        return ItemState::InProgress;
    }
    ItemState::NotStarted
}

fn finished(state: ItemState) -> bool {
    matches!(state, ItemState::Done | ItemState::Dropped)
}

/// FDL-FR-AZQM to FDL-FR-PRED: the delivery state of every item of a valid
/// plan, in document order. Pure over its inputs (FDL-FR-PBSD).
pub fn delivery_state(doc: &FlowDocument, drafts: &[DraftSummary]) -> FlowDeliveryState {
    let by_id: HashMap<&str, &DraftSummary> = drafts.iter().map(|d| (d.id.as_str(), d)).collect();
    let hierarchy = Hierarchy::new(&doc.items);

    let mut leaf_states: HashMap<&str, ItemState> = HashMap::new();
    let mut leaf_drafts: HashMap<&str, (Option<&DraftSummary>, bool)> = HashMap::new();
    for item in doc.items.iter().filter(|i| !i.kind.is_container()) {
        let (draft, missing) = leaf_draft(doc, item, &by_id);
        leaf_states.insert(&item.id, leaf_state(closed_on_tracker(doc, &item.id), draft));
        leaf_drafts.insert(&item.id, (draft, missing));
    }

    let mut states: HashMap<&str, ItemState> = HashMap::new();
    let mut counts: HashMap<&str, LeafCounts> = HashMap::new();
    for item in &doc.items {
        if let Some(s) = leaf_states.get(item.id.as_str()) {
            states.insert(&item.id, *s);
            continue;
        }
        let leaves: Vec<ItemState> = hierarchy
            .leaves_under(&item.id)
            .iter()
            .filter_map(|l| leaf_states.get(l).copied())
            .collect();
        let mut c = LeafCounts::default();
        leaves.iter().for_each(|s| c.add(*s));
        counts.insert(&item.id, c);
        states.insert(&item.id, container_state(closed_on_tracker(doc, &item.id), &leaves));
    }

    let items = doc
        .items
        .iter()
        .map(|item| {
            let state = states[item.id.as_str()];
            // FDL-FR-PXHL: blocks edges into the item or an ancestor whose
            // `from` is not finished. FDL-FR-PRED: relates edges never block.
            let mut targets = hierarchy.ancestors(&item.id);
            targets.push(item.id.as_str());
            let mut blocked_by: Vec<String> = Vec::new();
            if !finished(state) {
                for e in doc.edges.iter().filter(|e| e.edge_type == EdgeType::Blocks) {
                    let open = states.get(e.from.as_str()).is_some_and(|s| !finished(*s));
                    if open && targets.contains(&e.to.as_str()) && !blocked_by.contains(&e.from) {
                        blocked_by.push(e.from.clone());
                    }
                }
            }
            let (draft, missing) =
                leaf_drafts.get(item.id.as_str()).copied().unwrap_or((None, false));
            ItemDeliveryState {
                item_id: item.id.clone(),
                state,
                draft: draft.map(|d| DraftRef {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    status: d.status,
                }),
                draft_missing: missing,
                run: draft
                    .and_then(|d| d.graduation.as_ref())
                    .map(|g| RunRef { run_id: g.run_id.clone(), state: g.state }),
                blocked: !blocked_by.is_empty(),
                blocked_by,
                leaf_counts: counts.remove(item.id.as_str()),
            }
        })
        .collect();
    FlowDeliveryState { items }
}

/// The active worktree's drafts, with their graduation runs attached
/// (FDL-FR-PBSD: read once per call).
fn drafts_with_runs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
) -> Vec<DraftSummary> {
    let mut drafts = crate::drafts::list_drafts_impl(root).drafts;
    crate::drafts::attach_graduation(app, &mut drafts);
    drafts
}

/// FDL-FR-AZQM: `"load flow delivery state"`.
#[tauri::command]
pub fn load_flow_delivery_state(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    body: String,
) -> Result<FlowDeliveryState, FlowReadError> {
    let started = Instant::now();
    let Ok(root) = project.require_root() else {
        log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "flow delivery state refused: no project",
            crate::log_fields! {},
        );
        return Err(FlowReadError::no_project());
    };
    let doc = parse_valid(&body).map_err(|report| {
        log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "flow delivery state refused: invalid flow",
            crate::log_fields! { "violations" => report.violations.len() },
        );
        FlowReadError::invalid(report)
    })?;
    let drafts = drafts_with_runs(&app, &root);
    let state = delivery_state(&doc, &drafts);
    // FDL-FR-VTOQ: counts and a duration, never a name or a description.
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "flow delivery state loaded",
        crate::log_fields! {
            "items" => state.items.len(),
            "drafts" => drafts.len(),
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(state)
}

/// FDL-FR-JLXF: one plan item a draft is worked for.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowItemMatch {
    pub flow_id: String,
    pub flow_name: String,
    pub item_id: String,
    pub item_name: String,
    pub kind: &'static str,
}

/// FDL-FR-BDOD, FDL-FR-JLXF: the items of `flows` (each an artifact id and its
/// body, in path order) whose draft is `draft_id`. A Flow that fails
/// validation is skipped.
pub fn items_for_draft(
    flows: &[(String, String)],
    drafts: &[DraftSummary],
    draft_id: &str,
) -> Vec<FlowItemMatch> {
    let by_id: HashMap<&str, &DraftSummary> = drafts.iter().map(|d| (d.id.as_str(), d)).collect();
    let mut out = Vec::new();
    for (flow_id, body) in flows {
        let Ok(doc) = parse_valid(body) else { continue };
        for item in doc.items.iter().filter(|i| !i.kind.is_container()) {
            if let (Some(d), _) = leaf_draft(&doc, item, &by_id) {
                if d.id == draft_id {
                    out.push(FlowItemMatch {
                        flow_id: flow_id.clone(),
                        flow_name: doc.name.clone().unwrap_or_default(),
                        item_id: item.id.clone(),
                        item_name: item.name.clone(),
                        kind: item.kind.as_str(),
                    });
                }
            }
        }
    }
    out
}

/// FDL-FR-BDOD: `"find flow items for draft"`.
#[tauri::command]
pub fn find_flow_items_for_draft(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
    draft_id: String,
) -> Result<Vec<FlowItemMatch>, FlowReadError> {
    let Ok(root) = project.require_root() else {
        log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "flow items for draft refused: no project",
            crate::log_fields! {},
        );
        return Err(FlowReadError::no_project());
    };
    let flows = crate::artifacts::flow_bodies(&root);
    let drafts = crate::drafts::list_drafts_impl(&root).drafts;
    let matches = items_for_draft(&flows, &drafts, &draft_id);
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "flow items for draft found",
        crate::log_fields! { "flows" => flows.len(), "matches" => matches.len() },
    );
    Ok(matches)
}

#[cfg(test)]
mod tests;
