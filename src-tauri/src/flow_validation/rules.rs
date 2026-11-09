//! The structural rules of `FGV-flow-graph-validation.md`: identity, the
//! hierarchy, what an edge may join, the dependency cycle, and the sync links.
//! Each rule reads the objects `shape.rs` returned and appends its violations.

use std::collections::{HashMap, HashSet};

use super::model::{EdgeType, Hierarchy, Kind, RefKind, WaitGraph};
use super::shape::{RawEdge, RawItem, RawLink};
use super::FlowViolation;

/// FGV-FR-09, FGV-FR-13, FGV-FR-YHFX, FGV-FR-CTFX: the rules on items alone.
pub fn item_rules(items: &[RawItem], out: &mut Vec<FlowViolation>) {
    let mut seen = HashSet::new();
    for item in items {
        if !seen.insert(item.id.as_str()) {
            out.push(FlowViolation::element(
                "duplicate_id",
                &item.id,
                format!("more than one item carries the id `{}`", item.id),
            ));
        }
    }

    let kinds: HashMap<&str, Option<Kind>> =
        items.iter().map(|i| (i.id.as_str(), i.kind)).collect();
    let parent_of: HashMap<&str, Option<&str>> =
        items.iter().map(|i| (i.id.as_str(), i.parent.as_deref())).collect();

    for item in items {
        let parent_kind = match item.parent.as_deref() {
            None => None,
            Some(p) => match kinds.get(p) {
                Some(Some(k)) if k.is_container() => Some(*k),
                Some(None) => continue,
                _ => {
                    out.push(FlowViolation::element(
                        "dangling_parent",
                        &item.id,
                        format!(
                            "`{}` names a parent `{p}` that is no container in this document",
                            item.id
                        ),
                    ));
                    continue;
                }
            },
        };
        if let Some(p) = item.parent.as_deref() {
            if closes_on_itself(&item.id, p, &parent_of) {
                out.push(FlowViolation::element(
                    "containment_cycle",
                    &item.id,
                    format!("`{}` contains itself through its chain of parents", item.id),
                ));
                continue;
            }
        }
        if let Some(k) = item.kind {
            if !k.permitted_parents().contains(&parent_kind) {
                let container = match parent_kind {
                    Some(pk) => format!("a {}", pk.as_str()),
                    None => "the top level".to_string(),
                };
                out.push(FlowViolation::element(
                    "invalid_parent_kind",
                    &item.id,
                    format!("`{}` is a {} and cannot sit in {container}", item.id, k.as_str()),
                ));
            }
        }
    }

    let mut drafts = HashSet::new();
    for item in items {
        if let Some(d) = item.draft_id.as_deref() {
            if !drafts.insert(d) {
                out.push(FlowViolation::element(
                    "duplicate_draft_link",
                    &item.id,
                    format!(
                        "`{}` links the draft `{d}`, which another item already links",
                        item.id
                    ),
                ));
            }
        }
    }
}

/// Walk up from `parent`; arriving back at `id` means the chain closes on
/// itself. Bounded by the item count, so a cycle that excludes `id` ends too.
fn closes_on_itself(id: &str, parent: &str, parent_of: &HashMap<&str, Option<&str>>) -> bool {
    let mut at = Some(parent);
    let mut steps = 0;
    while let Some(current) = at {
        if current == id {
            return true;
        }
        steps += 1;
        if steps > parent_of.len() {
            return false;
        }
        at = parent_of.get(current).copied().flatten();
    }
    false
}

/// FGV-FR-09 to FGV-FR-11, FGV-FR-KBLT, FGV-FR-PMTY: the rules on edges.
pub fn edge_rules(items: &[RawItem], edges: &[RawEdge], out: &mut Vec<FlowViolation>) {
    let ids: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let hierarchy = Hierarchy::from_parts(
        items
            .iter()
            .filter_map(|i| i.kind.map(|k| (i.id.as_str(), k, i.parent.as_deref())))
            .collect(),
    );

    let mut seen_ids = HashSet::new();
    let mut seen_pairs = HashSet::new();
    let mut blocks: Vec<(&str, &str, &str)> = Vec::new();
    for edge in edges {
        let id = edge.id.as_str();
        if !seen_ids.insert(id) {
            out.push(FlowViolation::edge(
                "duplicate_id",
                id,
                format!("more than one edge carries the id `{id}`"),
            ));
        }
        let (from, to) = (edge.from.as_str(), edge.to.as_str());
        let mut sound = true;
        for (which, end) in [("from", from), ("to", to)] {
            if !ids.contains(end) {
                sound = false;
                out.push(FlowViolation::edge(
                    "dangling_edge_endpoint",
                    id,
                    format!("edge `{id}` has a `{which}` naming `{end}`, which is no item of this document"),
                ));
            }
        }
        if from == to {
            out.push(FlowViolation::edge(
                "self_edge",
                id,
                format!("edge `{id}` joins `{from}` to itself"),
            ));
            continue;
        }
        let pair = if from < to { (from, to) } else { (to, from) };
        if !seen_pairs.insert(pair) {
            out.push(FlowViolation::edge(
                "duplicate_edge",
                id,
                format!("more than one edge joins `{from}` and `{to}`"),
            ));
            sound = false;
        }
        if sound && (hierarchy.is_ancestor(from, to) || hierarchy.is_ancestor(to, from)) {
            out.push(FlowViolation::edge(
                "edge_within_lineage",
                id,
                format!("edge `{id}` joins `{from}` and `{to}`, and one contains the other"),
            ));
            sound = false;
        }
        if sound && edge.edge_type == Some(EdgeType::Blocks) {
            blocks.push((id, from, to));
        }
    }

    let cycle = WaitGraph::new(&hierarchy, &blocks).cycle_edges();
    for (id, from, to) in &blocks {
        if cycle.contains(id) {
            out.push(FlowViolation::edge(
                "dependency_cycle",
                *id,
                format!("edge `{id}` (`{from}` blocks `{to}`) is part of a dependency cycle"),
            ));
        }
    }
}

/// FGV-FR-LWPD: the rules on sync links.
pub fn link_rules(items: &[RawItem], links: &[RawLink], out: &mut Vec<FlowViolation>) {
    let kinds: HashMap<&str, Option<Kind>> =
        items.iter().map(|i| (i.id.as_str(), i.kind)).collect();
    let mut seen_items = HashSet::new();
    let mut seen_refs = HashSet::new();
    for link in links {
        let duplicate_item = !seen_items.insert(link.item_id.as_str());
        let duplicate_ref = !seen_refs.insert(link.ref_key.clone());
        if duplicate_item || duplicate_ref {
            out.push(FlowViolation::link(
                "duplicate_tracker_link",
                link.index,
                format!(
                    "sync link {} names an item or a tracker reference that another link names",
                    link.index
                ),
            ));
        }
        if let Some(Some(kind)) = kinds.get(link.item_id.as_str()) {
            let is_milestone = *kind == Kind::Milestone;
            if is_milestone != (link.ref_kind == RefKind::Milestone) {
                out.push(FlowViolation::link(
                    "tracker_kind_mismatch",
                    link.index,
                    format!(
                        "sync link {} joins the {} `{}` to a tracker {}",
                        link.index,
                        kind.as_str(),
                        link.item_id,
                        if link.ref_kind == RefKind::Milestone { "milestone" } else { "issue" }
                    ),
                ));
            }
        }
    }
}
