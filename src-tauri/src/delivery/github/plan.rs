//! The plan of a GitHub read (`GDP-github-delivery-plan.md` GDP-FR-BGIT to
//! GDP-FR-XWES), built without a network.
//!
//! Pure over its inputs: what the read found, the Type mapping of
//! `DSR-delivery-source.md`, the overlay of `DMS-delivery-machine-state.md`,
//! and the two status names of `GPP-github-polling.md`. The same inputs always
//! build the same plan.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::delivery::machine_state::DeliveryOverlay;
use crate::delivery::settings::{DeliverySettings, TypeMapping};
use crate::flow_delivery::{CloseReason, IssueReference, ItemTrackerFacts, TrackerState};
use crate::flow_validation::model::{
    Edge, EdgeType, FlowDocument, Hierarchy, Item, Kind, WaitGraph,
};
use crate::github_polling::client::ProjectShape;
use crate::github_polling::eligibility::{is_eligible, validate_shape, StatusNames};
use crate::github_polling::RepositoryRef;

use super::adapter::{ItemContent, StateReason, TrackerIssue, TrackerMilestone};
use super::cache::TrackerRead;

/// GDP-FR-NFWA: the id of an issue item.
pub fn issue_id(number: u64) -> String {
    format!("issue-{number}")
}

/// GDP-FR-NFWA: the id of a milestone item.
pub fn milestone_id(number: u64) -> String {
    format!("milestone-{number}")
}

/// What a plan item id names on the tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanRef {
    Issue(u64),
    Milestone(u64),
}

/// The tracker item an id of GDP-FR-NFWA names, or `None`.
pub fn parse_item_id(id: &str) -> Option<PlanRef> {
    let number = |rest: &str| {
        (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
            .then(|| rest.parse::<u64>().ok())
            .flatten()
    };
    if let Some(rest) = id.strip_prefix("issue-") {
        return number(rest).map(PlanRef::Issue);
    }
    id.strip_prefix("milestone-").and_then(number).map(PlanRef::Milestone)
}

/// GDP-FR-QGFO: the reference of an item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TrackerRef {
    pub kind: &'static str,
    pub number: u64,
    pub url: String,
}

/// GDP-FR-URZL: an open blocker that is not a plan issue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExternalBlocker {
    pub number: u64,
    pub title: String,
    pub url: String,
}

/// GDP-FR-QGFO: the tracker facts of one item, in the frontend shape
/// `TrackerFacts` of `src/types/delivery.ts`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanTrackerFacts {
    pub item_id: String,
    #[serde(rename = "ref")]
    pub reference: TrackerRef,
    pub state: &'static str,
    pub close_reason: Option<&'static str>,
    pub updated_at: String,
    pub status: Option<String>,
    pub claim_eligible: bool,
    pub external_blockers: Vec<ExternalBlocker>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UnmappedType {
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Misplaced {
    pub item_id: String,
    pub parent_item_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OmitReason {
    Lineage,
    Cycle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OmittedRelation {
    pub blocked_item_id: String,
    pub blocking_item_id: String,
    pub reason: OmitReason,
}

/// GDP-FR-XWES: the notices of a plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notices {
    pub unmapped_types: Vec<UnmappedType>,
    pub misplaced: Vec<Misplaced>,
    pub omitted_relations: Vec<OmittedRelation>,
    pub skipped: usize,
}

/// The plan of one read, before the states are computed.
#[derive(Clone, Debug)]
pub struct BuiltPlan {
    pub document: FlowDocument,
    pub tracker: Vec<PlanTrackerFacts>,
    /// The facts the state rules of `FDL-flow-delivery.md` read.
    pub facts: crate::flow_delivery::TrackerFacts,
    pub notices: Notices,
}

/// The two resolved status names of the read's Project, where the `Status`
/// field resolves them (GPP-FR-VXSB).
fn resolved_statuses(read: &TrackerRead, statuses: &StatusNames) -> Option<(String, String)> {
    let shape = ProjectShape {
        title: read.project.title.clone(),
        status_field: read.project.status_field.as_ref().map(|f| f.as_polling()),
    };
    validate_shape(&shape, statuses).ok().map(|v| (v.ready_name, v.in_progress_name))
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

fn kind_of(kind: crate::delivery::settings::IssueKind) -> Kind {
    Kind::parse(kind.as_str()).unwrap_or(Kind::Task)
}

/// GDP-FR-MYLM, GDP-FR-HNEW, GDP-FR-XJDE: every open milestone and every
/// closed milestone a plan issue names. They follow the milestone order of
/// the overlay (`order`, milestone item ids); the milestones it does not name
/// come after them, by due date with undated ones last, then by number.
fn plan_milestones<'a>(
    read: &'a TrackerRead,
    named: &HashSet<u64>,
    order: &[String],
) -> Vec<&'a TrackerMilestone> {
    let mut seen = HashSet::new();
    let mut out: Vec<&TrackerMilestone> = read
        .milestones
        .iter()
        .filter(|m| m.state == "open" || named.contains(&m.number))
        .filter(|m| seen.insert(m.number))
        .collect();
    let rank: HashMap<u64, usize> = order
        .iter()
        .enumerate()
        .filter_map(|(i, id)| match parse_item_id(id) {
            Some(PlanRef::Milestone(n)) => Some((n, i)),
            _ => None,
        })
        .rev()
        .collect();
    let key = |m: &TrackerMilestone| {
        let ranked = rank.get(&m.number).copied();
        (ranked.is_none(), ranked, m.due.is_none(), m.due.clone(), m.number)
    };
    out.sort_by_key(|m| key(m));
    out
}

/// GDP-FR-SYSZ: whether adding the `blocks` edge `id` to `accepted` closes a
/// dependency cycle (FGV-FR-PMTY).
fn closes_cycle(items: &[Item], accepted: &[Edge], id: &str) -> bool {
    let hierarchy = Hierarchy::new(items);
    let blocks: Vec<(&str, &str, &str)> =
        accepted.iter().map(|e| (e.id.as_str(), e.from.as_str(), e.to.as_str())).collect();
    WaitGraph::new(&hierarchy, &blocks).cycle_edges().contains(id)
}

/// GDP-FR-BGIT to GDP-FR-XWES: the plan of `read`.
pub fn build_plan(
    read: &TrackerRead,
    settings: &DeliverySettings,
    overlay: &DeliveryOverlay,
    statuses: &StatusNames,
) -> BuiltPlan {
    let mut notices = Notices::default();

    // GDP-FR-BGIT: the plan issues, in Project order.
    let mut plan: Vec<(Option<&str>, &TrackerIssue, Kind)> = Vec::new();
    let mut kinds: HashMap<u64, Kind> = HashMap::new();
    for item in &read.project.items {
        let ItemContent::Issue(issue) = &item.content else {
            notices.skipped += 1;
            continue;
        };
        if !read.own(issue) || kinds.contains_key(&issue.number) {
            notices.skipped += 1;
            continue;
        }
        match settings.map_type(issue.issue_type.as_deref()) {
            TypeMapping::Kind(kind) => {
                kinds.insert(issue.number, kind_of(kind));
                plan.push((item.status.as_deref(), issue, kind_of(kind)));
            }
            TypeMapping::Excluded => notices.skipped += 1,
            TypeMapping::Unmapped => {
                notices.skipped += 1;
                match notices.unmapped_types.iter_mut().find(|u| u.type_name == issue.issue_type) {
                    Some(entry) => entry.count += 1,
                    None => notices
                        .unmapped_types
                        .push(UnmappedType { type_name: issue.issue_type.clone(), count: 1 }),
                }
            }
        }
    }

    let named: HashSet<u64> = plan.iter().filter_map(|(_, i, _)| i.milestone).collect();
    let milestones = plan_milestones(read, &named, &overlay.milestone_order);
    let milestone_set: HashSet<u64> = milestones.iter().map(|m| m.number).collect();

    // GDP-FR-MYLM, GDP-FR-NFWA, GDP-FR-KBWK, GDP-FR-JCAR.
    let mut items: Vec<Item> = milestones
        .iter()
        .map(|m| Item {
            id: milestone_id(m.number),
            kind: Kind::Milestone,
            name: m.title.clone(),
            description: non_empty(&m.description),
            artifact_ids: None,
            parent_id: None,
            draft_id: None,
            footprint: None,
            due: m.due.clone(),
        })
        .collect();
    for (_, issue, kind) in &plan {
        let id = issue_id(issue.number);
        let parent = issue
            .parent
            .as_ref()
            .filter(|p| read.repository.is(&p.owner, &p.repo))
            .and_then(|p| kinds.get(&p.number).map(|k| (p.number, *k)));
        let mut parent_id = None;
        if let Some((number, parent_kind)) = parent {
            if kind.may_sit_in(Some(parent_kind)) {
                parent_id = Some(issue_id(number));
            } else {
                notices
                    .misplaced
                    .push(Misplaced { item_id: id.clone(), parent_item_id: issue_id(number) });
            }
        }
        if parent_id.is_none() {
            parent_id = issue.milestone.filter(|m| milestone_set.contains(m)).map(milestone_id);
        }
        items.push(Item {
            id,
            kind: *kind,
            name: issue.title.clone(),
            description: non_empty(&issue.body),
            artifact_ids: None,
            parent_id,
            draft_id: None,
            footprint: None,
            due: None,
        });
    }

    // GDP-FR-CRRO: the overlay of each item. An entry of no plan item is kept
    // in the overlay and is not part of the plan.
    for item in &mut items {
        if let Some(entry) = overlay.items.get(&item.id) {
            item.artifact_ids = entry.artifact_ids.clone().filter(|v| !v.is_empty());
            item.footprint = entry.footprint.clone().filter(|v| !v.is_empty());
            item.draft_id = entry.draft_id.clone();
        }
    }

    // GDP-FR-SYSZ, GDP-FR-URZL: relations in Project order.
    let mut edges: Vec<Edge> = Vec::new();
    let mut external: HashMap<u64, Vec<ExternalBlocker>> = HashMap::new();
    for (_, issue, _) in &plan {
        let blocked = issue_id(issue.number);
        for blocker in &issue.blocked_by {
            let own = read.repository.is(&blocker.owner, &blocker.repo);
            if !(own && kinds.contains_key(&blocker.number)) {
                if blocker.state == "open" {
                    external.entry(issue.number).or_default().push(ExternalBlocker {
                        number: blocker.number,
                        title: blocker.title.clone(),
                        url: blocker.url.clone(),
                    });
                }
                continue;
            }
            let blocking = issue_id(blocker.number);
            let omit = |reason| OmittedRelation {
                blocked_item_id: blocked.clone(),
                blocking_item_id: blocking.clone(),
                reason,
            };
            let hierarchy = Hierarchy::new(&items);
            if blocking == blocked
                || hierarchy.is_ancestor(&blocking, &blocked)
                || hierarchy.is_ancestor(&blocked, &blocking)
            {
                notices.omitted_relations.push(omit(OmitReason::Lineage));
                continue;
            }
            if edges.iter().any(|e| e.from == blocking && e.to == blocked) {
                continue;
            }
            let edge = Edge {
                id: format!("blocks-{blocking}-{blocked}"),
                from: blocking.clone(),
                to: blocked.clone(),
                edge_type: EdgeType::Blocks,
            };
            edges.push(edge);
            let id = edges.last().map(|e| e.id.clone()).unwrap_or_default();
            if closes_cycle(&items, &edges, &id) {
                edges.pop();
                notices.omitted_relations.push(omit(OmitReason::Cycle));
            }
        }
    }

    // GDP-FR-CRRO: the `relates` edges whose two ends are plan items.
    let ids: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let hierarchy = Hierarchy::new(&items);
    let mut relates: Vec<Edge> = Vec::new();
    for e in &overlay.relates {
        let known = ids.contains(e.from.as_str()) && ids.contains(e.to.as_str());
        let lineage = e.from == e.to
            || hierarchy.is_ancestor(&e.from, &e.to)
            || hierarchy.is_ancestor(&e.to, &e.from);
        let id = format!("relates-{}-{}", e.from, e.to);
        if known && !lineage && !relates.iter().any(|r| r.id == id) {
            relates.push(Edge {
                id,
                from: e.from.clone(),
                to: e.to.clone(),
                edge_type: EdgeType::Relates,
            });
        }
    }
    edges.extend(relates);

    // GDP-FR-QGFO: the tracker facts.
    let resolved = resolved_statuses(read, statuses);
    let repository = RepositoryRef {
        host: read.repository.host.clone(),
        owner: read.repository.owner.clone(),
        name: read.repository.name.clone(),
    };
    let mut tracker = Vec::new();
    let mut facts = crate::flow_delivery::TrackerFacts::new();
    for m in &milestones {
        let id = milestone_id(m.number);
        let closed = m.state != "open";
        tracker.push(PlanTrackerFacts {
            item_id: id.clone(),
            reference: TrackerRef { kind: "milestone", number: m.number, url: m.url.clone() },
            state: if closed { "closed" } else { "open" },
            close_reason: None,
            updated_at: m.updated_at.clone(),
            status: None,
            claim_eligible: false,
            external_blockers: Vec::new(),
        });
        let state = if closed { TrackerState::Closed } else { TrackerState::Open };
        facts.insert(id, ItemTrackerFacts { state, ..ItemTrackerFacts::default() });
    }
    for (status, issue, _) in &plan {
        let id = issue_id(issue.number);
        let closed = issue.state != "open";
        let reason = closed.then_some(match issue.state_reason {
            Some(StateReason::NotPlanned) | Some(StateReason::Duplicate) => CloseReason::NotPlanned,
            _ => CloseReason::Completed,
        });
        let blockers = external.remove(&issue.number).unwrap_or_default();
        let eligible = resolved.as_ref().is_some_and(|(ready, _)| {
            is_eligible(
                &issue.state,
                issue.issue_type.as_deref(),
                &issue.owner,
                &issue.repo,
                &repository,
                *status,
                ready,
            )
        });
        let in_progress =
            resolved.as_ref().is_some_and(|(_, progress)| *status == Some(progress.as_str()));
        facts.insert(
            id.clone(),
            ItemTrackerFacts {
                state: if closed { TrackerState::Closed } else { TrackerState::Open },
                close_reason: reason,
                status: status.map(str::to_string),
                status_is_in_progress: in_progress,
                open_external_blockers: blockers.len(),
                issue: Some(IssueReference {
                    host: read.repository.host.clone(),
                    owner: read.repository.owner.clone(),
                    repo: read.repository.name.clone(),
                    number: issue.number,
                }),
            },
        );
        tracker.push(PlanTrackerFacts {
            item_id: id,
            reference: TrackerRef { kind: "issue", number: issue.number, url: issue.url.clone() },
            state: if closed { "closed" } else { "open" },
            close_reason: reason.map(|r| match r {
                CloseReason::Completed => "completed",
                CloseReason::NotPlanned => "not_planned",
            }),
            updated_at: issue.updated_at.clone(),
            status: status.map(str::to_string),
            claim_eligible: eligible,
            external_blockers: blockers,
        });
    }

    let document = FlowDocument {
        version: crate::flow_validation::FLOW_VERSION,
        name: Some(read.project.title.clone()),
        description: non_empty(&read.project.short_description),
        references: (!overlay.references.is_empty()).then(|| overlay.references.clone()),
        items,
        edges,
    };
    BuiltPlan { document, tracker, facts, notices }
}
