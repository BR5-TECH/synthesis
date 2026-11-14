//! The overlay of a GitHub plan (`DMS-delivery-machine-state.md`
//! DMS-FR-WAUS, DMS-FR-QFEA, DMS-FR-VEHN): what GitHub has no field for. For
//! each item, its references, its footprint, and its draft link; and the
//! `relates` edges, the plan's references, and the milestone order.
//!
//! Read and written in-process only, by `GDP-github-delivery-plan.md` and
//! `DSR-delivery-source.md`. No command writes it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::flow_validation::field_rules;
use crate::flow_validation::{FlowViolation, ValidationReport};
use crate::fs::RootFs;
use crate::logging::{log_info, log_warn, Domain, LogBuffer, LogSink};

use super::super::{DeliveryError, ERR_INVALID_OVERLAY};
use super::records::{read_record, record_path, write_record, OVERLAY_DIRNAME};

/// DMS-FR-WAUS: one item's entry. Each field is absent when unset, as in a
/// Flow document (FGV-FR-01).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
}

/// DMS-FR-WAUS: one `relates` edge, by its two item ids.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatesEdge {
    pub from: String,
    pub to: String,
}

/// DMS contract surface `DeliveryOverlay`. The entries are kept by item id in
/// a fixed order, so the same overlay always writes the same file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryOverlay {
    #[serde(default)]
    pub items: BTreeMap<String, OverlayEntry>,
    #[serde(default)]
    pub relates: Vec<RelatesEdge>,
    #[serde(default)]
    pub references: Vec<String>,
    /// DMS-FR-WAUS: the milestone item ids, first to last (per
    /// `FLO-flow.md` FLO-FR-MCFR). An overlay of an earlier version has no
    /// such field and reads as an empty order.
    #[serde(default, rename = "milestoneOrder", skip_serializing_if = "Vec::is_empty")]
    pub milestone_order: Vec<String>,
}

/// DMS-FR-WAUS: a milestone order that names an id more than once.
pub const DUPLICATE_MILESTONE_ORDER: &str = "duplicate_milestone_order";

/// A GitHub node ID names a record file, so it must be one plain path
/// segment. Node IDs are made of letters, digits, `_`, and `-`.
fn node_id_ok(project_node_id: &str) -> bool {
    !project_node_id.is_empty()
        && project_node_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// DMS-FR-QFEA: the violations of `overlay` against the field rules of
/// `FGV-flow-graph-validation.md`: FGV-FR-RQEM for a footprint, FGV-FR-JDGO
/// for references, FGV-FR-CTFX for draft links, and FGV-FR-11 for `relates`
/// edges. Pure. The violations come in the order of FGV-FR-04: the plan's
/// references first, then the items in id order, then the edges.
pub fn validate_overlay(overlay: &DeliveryOverlay) -> ValidationReport {
    let mut plan = Vec::new();
    field_rules::check_references(None, "references", &overlay.references, &mut plan);

    let mut items: Vec<FlowViolation> = Vec::new();
    for (id, entry) in &overlay.items {
        if let Some(ids) = &entry.artifact_ids {
            field_rules::check_references(Some(id), "artifactIds", ids, &mut items);
        }
        if let Some(footprint) = &entry.footprint {
            field_rules::check_footprint(id, footprint, &mut items);
        }
    }
    field_rules::check_draft_links(
        overlay.items.iter().filter_map(|(id, e)| e.draft_id.as_deref().map(|d| (id.as_str(), d))),
        &mut items,
    );
    // FGV-FR-04: within the item group, the order of the items.
    let rank: BTreeMap<&str, usize> =
        overlay.items.keys().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    items.sort_by_key(|v| v.element_id.as_deref().and_then(|id| rank.get(id).copied()));

    let mut edges = Vec::new();
    field_rules::check_edge_ends(
        overlay
            .relates
            .iter()
            .enumerate()
            .map(|(i, e)| (format!("relates edge {i}"), e.from.as_str(), e.to.as_str())),
        &mut edges,
    );

    // DMS-FR-WAUS: each id of the milestone order is unique.
    let mut order = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in &overlay.milestone_order {
        if !seen.insert(id.as_str()) {
            order.push(FlowViolation {
                code: DUPLICATE_MILESTONE_ORDER,
                message: format!("the milestone order names `{id}` more than once"),
                element_id: None,
                edge_id: None,
            });
        }
    }

    let mut violations = plan;
    violations.extend(items);
    violations.extend(edges);
    violations.extend(order);
    ValidationReport { valid: violations.is_empty(), violations }
}

/// DMS contract surface `read_delivery_overlay(project_node_id)`: the overlay
/// of the Project. A Project with no stored overlay, a record that cannot be
/// parsed (DMS-FR-ETLQ), and a node ID that is not one path segment each read
/// as an empty overlay. DMS-FR-EKKY: an entry of an item the plan does not
/// hold is returned as stored; the reader ignores it.
pub fn read_delivery_overlay<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &RootFs,
    project_node_id: &str,
) -> Result<DeliveryOverlay, DeliveryError> {
    if !node_id_ok(project_node_id) {
        log_warn(
            sink,
            buffer,
            &[Domain::Backend],
            "delivery overlay read for a node id that is no path segment",
            crate::log_fields! { "bytes" => project_node_id.len() },
        );
        return Ok(DeliveryOverlay::default());
    }
    let path = record_path(store, OVERLAY_DIRNAME, &format!("{project_node_id}.json"))?;
    Ok(read_record(sink, buffer, store, &path, "overlay")?.unwrap_or_default())
}

/// DMS contract surface `write_delivery_overlay(project_node_id, overlay)`:
/// replace the overlay of the Project with one atomic whole-file write. An
/// overlay that breaks a field rule (DMS-FR-QFEA), and a node ID that is not
/// one path segment, are refused with `invalid_overlay`, and nothing is
/// written.
pub fn write_delivery_overlay<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &RootFs,
    project_node_id: &str,
    overlay: &DeliveryOverlay,
) -> Result<(), DeliveryError> {
    if !node_id_ok(project_node_id) {
        log_warn(
            sink,
            buffer,
            &[Domain::Backend],
            "delivery overlay refused: node id is no path segment",
            crate::log_fields! { "bytes" => project_node_id.len() },
        );
        return Err(DeliveryError::new(
            ERR_INVALID_OVERLAY,
            "The Project node ID is not a plain identifier.",
        ));
    }
    let report = validate_overlay(overlay);
    if !report.valid {
        // DMS-FR-ROMF: the codes and the count, never a path or an id.
        let codes: Vec<&str> = report.violations.iter().map(|v| v.code).collect();
        log_warn(
            sink,
            buffer,
            &[Domain::Backend],
            "delivery overlay refused: invalid overlay",
            crate::log_fields! {
                "violations" => report.violations.len(),
                "codes" => codes.join(","),
            },
        );
        return Err(DeliveryError::new(
            ERR_INVALID_OVERLAY,
            "The overlay breaks a rule of the delivery plan.",
        )
        .with_report(report));
    }
    let path = record_path(store, OVERLAY_DIRNAME, &format!("{project_node_id}.json"))?;
    write_record(store, &path, overlay)?;
    log_info(
        sink,
        buffer,
        &[Domain::Backend],
        "delivery overlay written",
        crate::log_fields! {
            "items" => overlay.items.len(),
            "relates" => overlay.relates.len(),
            "references" => overlay.references.len(),
            "milestone_order" => overlay.milestone_order.len(),
        },
    );
    Ok(())
}
