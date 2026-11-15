//! `FDL-flow-delivery.md`: how far along a delivery plan is.
//!
//! The state of every item comes from the work itself — the drafts the tasks are
//! worked in, their graduation runs, and, for a plan held on GitHub, the
//! tracker facts of each item — and never from a status the author keeps by
//! hand. This module only reads: a blocked item is reported, and nothing here
//! starts, stops, or refuses a run (FDL intent).
//!
//! A Flow file has no tracker facts, so `load_flow_delivery_state` derives its
//! state from the drafts and the runs alone. The GitHub delivery plan
//! (`GDP-github-delivery-plan.md` GDP-FR-XWES) reads the tracker facts and calls
//! [`compute_delivery_state`] in-process.

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;

use crate::delivery::settings::DeliverySourceChoice;
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

/// GDP-FR-QGFO: whether a tracker item is open or closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackerState {
    #[default]
    Open,
    Closed,
}

/// GDP-FR-QGFO: why a closed tracker item was closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    Completed,
    NotPlanned,
}

/// FDL-FR-TTZZ: the issue of a leaf of a GitHub plan, by the four values that
/// identify it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueReference {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

/// The tracker facts of one item of a GitHub plan that the state rules read
/// (FDL-FR-TTZZ, FDL-FR-YIAZ, FDL-FR-WPSZ, FDL-FR-UEPM). The GitHub delivery
/// plan reads them (GDP-FR-QGFO); this module only uses them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemTrackerFacts {
    pub state: TrackerState,
    /// Present on a closed item alone. An absent reason on a closed item reads
    /// as `completed` (FDL-FR-YIAZ).
    pub close_reason: Option<CloseReason>,
    /// The name of the item's Project `Status` option, if it has one.
    pub status: Option<String>,
    /// Whether `status` is the option that the In Progress status resolves to
    /// (GPP-FR-VXSB). The caller resolves it, so this module reads no setting.
    pub status_is_in_progress: bool,
    /// The count of the item's open blockers that are not items of the plan
    /// (GDP-FR-URZL).
    pub open_external_blockers: usize,
    /// The item's issue. `None` for a milestone.
    pub issue: Option<IssueReference>,
}

/// The tracker facts of a plan, by item id. An item with no entry has no
/// tracker facts, as every item of a Flow file.
pub type TrackerFacts = HashMap<String, ItemTrackerFacts>;

/// FDL-FR-TTZZ: the draft a leaf item is worked in, and whether a `draftId`
/// names no draft (FDL-FR-BRAQ). A leaf of a GitHub plan with no `draftId`
/// takes the GitHub-shadow draft of its issue.
fn leaf_draft<'d>(
    item: &Item,
    facts: Option<&ItemTrackerFacts>,
    drafts: &'d HashMap<&str, &DraftSummary>,
) -> (Option<&'d DraftSummary>, bool) {
    if let Some(id) = item.draft_id.as_deref() {
        return match drafts.get(id) {
            Some(d) => (Some(*d), false),
            None => (None, true),
        };
    }
    let Some(r) = facts.and_then(|f| f.issue.as_ref()) else { return (None, false) };
    // The draft list is a map, so the shadow draft of the issue is looked for
    // in a fixed order. Two shadow drafts of one issue cannot exist
    // (DRS-FR-XDWS), but a fixed order keeps the answer stable if they do.
    let mut shadows: Vec<&&DraftSummary> = drafts
        .values()
        .filter(|d| {
            d.github_issue.as_ref().is_some_and(|g| {
                g.repository_host.eq_ignore_ascii_case(&r.host)
                    && g.repository_owner.eq_ignore_ascii_case(&r.owner)
                    && g.repository_name.eq_ignore_ascii_case(&r.repo)
                    && g.issue_number == r.number
            })
        })
        .collect();
    shadows.sort_by(|a, b| a.id.cmp(&b.id));
    (shadows.first().map(|d| **d), false)
}

/// FDL-FR-YIAZ: the state of a closed tracker item, or `None` for an item that
/// is open or has no tracker facts.
fn closed_state(facts: Option<&ItemTrackerFacts>) -> Option<ItemState> {
    let f = facts.filter(|f| f.state == TrackerState::Closed)?;
    Some(match f.close_reason {
        Some(CloseReason::NotPlanned) => ItemState::Dropped,
        _ => ItemState::Done,
    })
}

/// FDL-FR-VGZI, FDL-FR-YIAZ, FDL-FR-WPSZ: a leaf's state from its tracker
/// facts, its draft, and its run.
fn leaf_state(facts: Option<&ItemTrackerFacts>, draft: Option<&DraftSummary>) -> ItemState {
    if let Some(closed) = closed_state(facts) {
        return closed;
    }
    let by_work = work_state(draft);
    // FDL-FR-WPSZ: an open leaf on the In Progress option of the board is in
    // progress, when its work says no more than that it is drafted.
    if matches!(by_work, ItemState::NotStarted | ItemState::Drafting)
        && facts.is_some_and(|f| f.status_is_in_progress)
    {
        return ItemState::InProgress;
    }
    by_work
}

/// FDL-FR-VGZI: a leaf's state from its draft and its run alone.
fn work_state(draft: Option<&DraftSummary>) -> ItemState {
    let Some(d) = draft else { return ItemState::NotStarted };
    match d.status {
        DraftStatus::Archived => ItemState::Dropped,
        DraftStatus::Graduated => ItemState::Done,
        _ if d.graduation.as_ref().is_some_and(|g| run_is_open(g.state)) => ItemState::InProgress,
        _ => ItemState::Drafting,
    }
}

/// FDL-FR-JYKC, FDL-FR-YIAZ: a container's state from its tracker facts and
/// the leaves under it.
fn container_state(facts: Option<&ItemTrackerFacts>, leaves: &[ItemState]) -> ItemState {
    if let Some(closed) = closed_state(facts) {
        return closed;
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
/// Flow file, in document order. A Flow file has no tracker facts. Pure over
/// its inputs (FDL-FR-PBSD).
pub fn delivery_state(doc: &FlowDocument, drafts: &[DraftSummary]) -> FlowDeliveryState {
    delivery_state_with_tracker(doc, &TrackerFacts::new(), drafts)
}

/// FDL-FR-AZQM to FDL-FR-UEPM: the delivery state of every item of a valid
/// plan, in document order, with the tracker facts of a GitHub plan applied.
/// Pure over its inputs (FDL-FR-PBSD): it reads nothing, writes nothing, and
/// reaches no network.
pub fn delivery_state_with_tracker(
    doc: &FlowDocument,
    tracker: &TrackerFacts,
    drafts: &[DraftSummary],
) -> FlowDeliveryState {
    let by_id: HashMap<&str, &DraftSummary> = drafts.iter().map(|d| (d.id.as_str(), d)).collect();
    let hierarchy = Hierarchy::new(&doc.items);

    let mut leaf_states: HashMap<&str, ItemState> = HashMap::new();
    let mut leaf_drafts: HashMap<&str, (Option<&DraftSummary>, bool)> = HashMap::new();
    for item in doc.items.iter().filter(|i| !i.kind.is_container()) {
        let facts = tracker.get(&item.id);
        let (draft, missing) = leaf_draft(item, facts, &by_id);
        leaf_states.insert(&item.id, leaf_state(facts, draft));
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
        states.insert(&item.id, container_state(tracker.get(&item.id), &leaves));
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
            // FDL-FR-UEPM: an open blocker outside the plan blocks an item that
            // is not finished, and is not listed in `blocked_by`.
            let external = !finished(state)
                && tracker.get(&item.id).is_some_and(|f| f.open_external_blockers > 0);
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
                blocked: external || !blocked_by.is_empty(),
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

/// FDL contract surface: `compute_delivery_state(document, tracker_facts)` for
/// a GitHub plan (GDP-FR-XWES). It reads the drafts of the active worktree and
/// their runs once (FDL-FR-PBSD), and applies FDL-FR-TTZZ, FDL-FR-YIAZ,
/// FDL-FR-WPSZ, and FDL-FR-UEPM. The caller has the document from the plan it
/// built, so no validation is made here.
pub fn compute_delivery_state<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    document: &FlowDocument,
    tracker_facts: &TrackerFacts,
) -> FlowDeliveryState {
    let drafts = drafts_with_runs(app, root);
    delivery_state_with_tracker(document, tracker_facts, &drafts)
}

/// FDL-FR-AZQM: `"load flow delivery state"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn load_flow_delivery_state(
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
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

/// FDL-FR-JLXF: one plan item a draft is worked for. `flow_id` is `None` for
/// an item of a GitHub plan. `delivery` is true where the plan is the
/// project's delivery (DSR-delivery-source.md).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowItemMatch {
    pub flow_id: Option<String>,
    pub flow_name: String,
    pub delivery: bool,
    pub item_id: String,
    pub item_name: String,
    pub kind: &'static str,
}

/// FDL-FR-BDOD, FDL-FR-JLXF: the items of `flows` (each an artifact id and its
/// body, in path order) whose draft is `draft_id`. A Flow that fails
/// validation is skipped. `delivery_flow_id` is the artifact id of the Flow
/// file that is the project's delivery source, if one is.
pub fn items_for_draft(
    flows: &[(String, String)],
    drafts: &[DraftSummary],
    draft_id: &str,
    delivery_flow_id: Option<&str>,
) -> Vec<FlowItemMatch> {
    let by_id: HashMap<&str, &DraftSummary> = drafts.iter().map(|d| (d.id.as_str(), d)).collect();
    let mut out = Vec::new();
    for (flow_id, body) in flows {
        let Ok(doc) = parse_valid(body) else { continue };
        let delivery = delivery_flow_id == Some(flow_id.as_str());
        for item in doc.items.iter().filter(|i| !i.kind.is_container()) {
            if let (Some(d), _) = leaf_draft(item, None, &by_id) {
                if d.id == draft_id {
                    out.push(FlowItemMatch {
                        flow_id: Some(flow_id.clone()),
                        flow_name: doc.name.clone().unwrap_or_default(),
                        delivery,
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

/// FDL-FR-JLUJ, FDL-FR-JLXF: the leaves of the GitHub delivery plan `doc`
/// whose draft (FDL-FR-TTZZ, with the tracker facts) is `draft_id`. A result
/// carries no artifact id, and the Project's title as the Flow name.
pub fn plan_items_for_draft(
    doc: &FlowDocument,
    facts: &TrackerFacts,
    drafts: &[DraftSummary],
    draft_id: &str,
) -> Vec<FlowItemMatch> {
    let by_id: HashMap<&str, &DraftSummary> = drafts.iter().map(|d| (d.id.as_str(), d)).collect();
    doc.items
        .iter()
        .filter(|i| !i.kind.is_container())
        .filter(|i| leaf_draft(i, facts.get(&i.id), &by_id).0.is_some_and(|d| d.id == draft_id))
        .map(|item| FlowItemMatch {
            flow_id: None,
            flow_name: doc.name.clone().unwrap_or_default(),
            delivery: true,
            item_id: item.id.clone(),
            item_name: item.name.clone(),
            kind: item.kind.as_str(),
        })
        .collect()
}

/// FDL-FR-BDOD: `"find flow items for draft"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn find_flow_items_for_draft(
    app: tauri::AppHandle,
    project: crate::project_scope::Scoped<'_, ProjectState>,
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
    Ok(find_items_for_draft_in(&app, &root, &draft_id))
}

/// FDL-FR-BDOD, FDL-FR-JLUJ: the plan items of every Flow file of `root` that
/// name the draft, then those of the plan of the GitHub read cache. It makes
/// no network request.
pub(crate) fn find_items_for_draft_in<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    draft_id: &str,
) -> Vec<FlowItemMatch> {
    let flows = crate::artifacts::flow_bodies(root);
    let drafts = crate::drafts::list_drafts_impl(root).drafts;
    let delivery = delivery_flow_id(app, root);
    let mut matches = items_for_draft(&flows, &drafts, draft_id, delivery.as_deref());
    // FDL-FR-JLUJ: the plan of the GitHub read cache comes after the files,
    // and no request is made.
    let github = crate::delivery::github::cached_delivery_plan(app, root);
    if let Some(plan) = &github {
        matches.extend(plan_items_for_draft(&plan.document, &plan.facts, &drafts, draft_id));
    }
    log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "flow items for draft found",
        crate::log_fields! {
            "flows" => flows.len(),
            "matches" => matches.len(),
            "delivery_file" => delivery.is_some(),
            "github_plan" => github.is_some(),
        },
    );
    matches
}

/// FDL-FR-JLXF: the artifact id of the Flow file that the stored delivery
/// settings choose as the delivery source (DSR-FR-HOEW), or `None`. A store
/// that cannot be read chooses no file, and this function logs why.
fn delivery_flow_id<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
) -> Option<String> {
    match crate::project_settings::load_delivery_settings_from(root) {
        Ok(settings) => match settings.source {
            DeliverySourceChoice::File { flow_id } => Some(flow_id),
            DeliverySourceChoice::Automatic | DeliverySourceChoice::Github => None,
        },
        Err(reason) => {
            log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "flow items for draft: delivery settings unreadable",
                crate::log_fields! { "reason" => reason },
            );
            None
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod tests_tracker;
