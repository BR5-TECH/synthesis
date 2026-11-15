//! `load_delivery_milestones(refresh)` (`DSR-delivery-source.md` DSR-FR-OJLB
//! to DSR-FR-SKPX): the milestones of the delivery plan, for the Dashboard's
//! Delivery widget. It writes nothing.

use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::delivery::github;
use crate::delivery::DeliveryError;
use crate::flow_delivery::{ItemDeliveryState, ItemState, LeafCounts};
use crate::flow_validation::model::{FlowDocument, Hierarchy, Kind};
use crate::logging::LogLevel;

use super::{log, project_root, refuse, resolve_source, DeliverySource};

/// DSR-FR-FQZD: one milestone of the answer.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryMilestone {
    pub item_id: String,
    pub name: String,
    pub due: Option<String>,
    pub state: ItemState,
    pub leaf_counts: LeafCounts,
    pub blocked_leaves: usize,
    pub current: bool,
}

/// DSR contract surface `DeliveryMilestones`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryMilestones {
    pub source: DeliverySource,
    pub plan_name: Option<String>,
    pub milestones: Vec<DeliveryMilestone>,
    pub read_at: Option<String>,
    pub stale: bool,
}

/// FLO-FR-MCFR: the milestone order of `milestones`, which come in the plan's
/// order. The dated milestones take the places of the dated milestones in that
/// order, sorted by due date; milestones of one date keep their plan order.
/// Each undated milestone keeps its place. Pure.
pub fn milestone_order<T>(milestones: Vec<T>, due: impl Fn(&T) -> Option<&str>) -> Vec<T> {
    let dated_slots: Vec<bool> = milestones.iter().map(|m| due(m).is_some()).collect();
    let (mut dated, mut undated): (Vec<T>, Vec<T>) =
        milestones.into_iter().partition(|m| due(m).is_some());
    // A stable sort keeps the plan order within one date.
    dated.sort_by(|a, b| due(a).cmp(&due(b)));
    let (mut dated, mut undated) = (dated.drain(..), undated.drain(..));
    dated_slots
        .into_iter()
        .filter_map(|is_dated| if is_dated { dated.next() } else { undated.next() })
        .collect()
}

/// DSR-FR-RZUH, FLO-FR-MCFR: put the milestone items of `items` in milestone
/// order. They take the places of the milestone items; every other item keeps
/// its place. Pure.
pub fn order_milestone_items(items: &mut [crate::flow_validation::model::Item]) {
    let slots: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.kind == Kind::Milestone)
        .map(|(n, _)| n)
        .collect();
    let milestones: Vec<_> = slots.iter().map(|n| items[*n].clone()).collect();
    let ordered = milestone_order(milestones, |m| m.due.as_deref());
    for (slot, item) in slots.into_iter().zip(ordered) {
        items[slot] = item;
    }
}

/// DSR-FR-FQZD, DSR-FR-BIYW: the milestones of `doc` with their states, in
/// milestone order (FLO-FR-MCFR) from the order of `doc`: the document order
/// of a Flow file, and the order of GDP-FR-HNEW for a GitHub plan. The first
/// that is neither done nor dropped is `current`. Pure.
pub fn milestones_of(doc: &FlowDocument, states: &[ItemDeliveryState]) -> Vec<DeliveryMilestone> {
    let hierarchy = Hierarchy::new(&doc.items);
    let state_of = |id: &str| states.iter().find(|s| s.item_id == id);
    let out: Vec<DeliveryMilestone> = doc
        .items
        .iter()
        .filter(|i| i.kind == Kind::Milestone)
        .map(|item| {
            let state = state_of(&item.id);
            let blocked_leaves = hierarchy
                .leaves_under(&item.id)
                .iter()
                .filter(|leaf| state_of(leaf).is_some_and(|s| s.blocked))
                .count();
            DeliveryMilestone {
                item_id: item.id.clone(),
                name: item.name.clone(),
                due: item.due.clone(),
                state: state.map(|s| s.state).unwrap_or(ItemState::NotStarted),
                leaf_counts: state.and_then(|s| s.leaf_counts.clone()).unwrap_or_default(),
                blocked_leaves,
                current: false,
            }
        })
        .collect();
    let mut out = milestone_order(out, |m| m.due.as_deref());
    if let Some(first) =
        out.iter_mut().find(|m| !matches!(m.state, ItemState::Done | ItemState::Dropped))
    {
        first.current = true;
    }
    out
}

/// DSR-FR-OJLB to DSR-FR-SKPX: the answer, on the calling thread.
pub fn milestones_impl<R: Runtime>(
    app: &AppHandle<R>,
    refresh: bool,
) -> Result<DeliveryMilestones, DeliveryError> {
    let started = Instant::now();
    let op = "load_delivery_milestones";
    log(
        app,
        LogLevel::Debug,
        false,
        "delivery milestones load started",
        crate::log_fields! { "refresh" => refresh },
    );
    let fail = |e| refuse(app, op, started, e);
    let root = project_root(app).map_err(fail)?;
    let source = resolve_source(app).map_err(fail)?;
    let answer = match &source {
        DeliverySource::None { .. } => DeliveryMilestones {
            source: source.clone(),
            plan_name: None,
            milestones: Vec::new(),
            read_at: None,
            stale: false,
        },
        DeliverySource::Github { .. } => {
            let plan = github::load_plan_impl(app, refresh).map_err(fail)?;
            DeliveryMilestones {
                plan_name: plan.document.name.clone(),
                milestones: milestones_of(&plan.document, &plan.states),
                read_at: Some(plan.read_at.clone()),
                stale: plan.stale,
                source: source.clone(),
            }
        }
        DeliverySource::File { flow_id, .. } => {
            let doc = super::publish::read_valid_flow(&root, flow_id).map_err(fail)?;
            let states = crate::flow_delivery::compute_delivery_state(
                app,
                &root,
                &doc,
                &crate::flow_delivery::TrackerFacts::new(),
            );
            DeliveryMilestones {
                plan_name: doc.name.clone(),
                milestones: milestones_of(&doc, &states.items),
                read_at: None,
                stale: false,
                source: source.clone(),
            }
        }
    };
    log(
        app,
        LogLevel::Info,
        false,
        "delivery milestones loaded",
        crate::log_fields! {
            "milestones" => answer.milestones.len(),
            "stale" => answer.stale,
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(answer)
}

/// DSR contract surface: `"load delivery milestones (refresh)"`.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub async fn load_delivery_milestones(
    app: AppHandle,
    refresh: bool,
) -> Result<DeliveryMilestones, DeliveryError> {
    super::off_thread(move || milestones_impl(&app, refresh)).await
}
