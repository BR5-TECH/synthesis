//! `FGV-flow-graph-validation.md`: the backend authority on what a Flow document
//! is.
//!
//! A Flow is the project's delivery plan: a hierarchy of deliverables,
//! milestones, epics, features, tasks, and defects, the `blocks` and `relates`
//! edges between them, and the tracker issues it is synced with. This module
//! owns its schema (FGV-FR-01) and the structural rules it obeys, and answers
//! one question about a body of text: is this a Flow, and if not, exactly what
//! is wrong with it.
//!
//! Everything here is pure (FGV-FR-02): no filesystem, no project, no event, no
//! state between calls. That is what lets `save_artifact_contents` call it inside
//! a write (`PST-FR-28`) and the Flow tab call it on a body that has never been
//! on disk (`../ui/FLO-flow.md` FLO-FR-46).

pub mod model;
mod rules;
mod shape;

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::logging::{log_debug, log_warn, Domain, BUFFER};

/// The one document version this build reads (FGV-FR-07).
pub(crate) const FLOW_VERSION: u64 = 2;

/// One thing wrong with a document (FGV-FR-04). `element_id` names the item it
/// sits on, `edge_id` the edge, `link_index` the sync link, and a violation
/// about the document as a whole carries none of them.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowViolation {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_index: Option<usize>,
}

impl FlowViolation {
    fn doc(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), element_id: None, edge_id: None, link_index: None }
    }

    fn element(code: &'static str, id: impl Into<String>, message: impl Into<String>) -> Self {
        Self { element_id: Some(id.into()), ..Self::doc(code, message) }
    }

    fn edge(code: &'static str, id: impl Into<String>, message: impl Into<String>) -> Self {
        Self { edge_id: Some(id.into()), ..Self::doc(code, message) }
    }

    fn link(code: &'static str, index: usize, message: impl Into<String>) -> Self {
        Self { link_index: Some(index), ..Self::doc(code, message) }
    }
}

/// FGV-FR-02: what a validation returns.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub valid: bool,
    pub violations: Vec<FlowViolation>,
}

impl ValidationReport {
    fn ok() -> Self {
        Self { valid: true, violations: Vec::new() }
    }

    fn from(violations: Vec<FlowViolation>) -> Self {
        Self { valid: violations.is_empty(), violations }
    }

    fn single(v: FlowViolation) -> Self {
        Self { valid: false, violations: vec![v] }
    }
}

/// The list in `field`, or an empty one. An absent list is a document holding
/// none of them (FGV-FR-01); anything else that is not an array is `shape`.
fn array<'a>(
    obj: &'a serde_json::Map<String, Value>,
    field: &str,
    out: &mut Vec<FlowViolation>,
) -> &'a [Value] {
    match obj.get(field) {
        None => &[],
        Some(Value::Array(items)) => items,
        Some(_) => {
            out.push(FlowViolation::doc(
                "shape",
                format!("the document has an `{field}` that is not an array"),
            ));
            &[]
        }
    }
}

/// FGV-FR-04: sort a group of violations by where their subject first appears
/// in the document. The sort is stable, so the violations of one subject keep
/// the order the rules found them in.
fn order_by_position(group: &mut [FlowViolation], key: impl Fn(&FlowViolation) -> Option<usize>) {
    group.sort_by_key(|v| key(v).unwrap_or(usize::MAX));
}

/// FGV-FR-02: judge `body` against the Flow schema and the plan's structural
/// rules. Pure — the same body always yields the same report.
pub fn validate(body: &str) -> ValidationReport {
    // FGV-FR-05: an empty or whitespace-only body is the empty plan.
    if body.trim().is_empty() {
        return ValidationReport::ok();
    }

    // FGV-FR-06: nothing structural is evaluated against a document that could
    // not be read, so this violation is reported alone.
    let raw: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return ValidationReport::single(FlowViolation::doc(
                "not_json",
                format!("the file is not valid JSON: {e}"),
            ));
        }
    };

    // FGV-FR-07: likewise alone, version 1 included.
    let obj = match raw.as_object() {
        Some(o) if o.get("version").and_then(Value::as_u64) == Some(FLOW_VERSION) => o,
        _ => {
            let found = raw
                .as_object()
                .and_then(|o| o.get("version"))
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string());
            return ValidationReport::single(FlowViolation::doc(
                "unknown_version",
                format!("expected a Flow document of version {FLOW_VERSION}, found {found}"),
            ));
        }
    };

    let mut doc_level = Vec::new();
    let mut item_level = Vec::new();
    let mut edge_level = Vec::new();
    let mut link_level = Vec::new();

    shape::unknown_fields(obj, shape::DOC_FIELDS, "the document", shape::At::Doc, &mut doc_level);
    shape::optional_string(obj, "name", "the document", shape::At::Doc, &mut doc_level);
    shape::optional_string(obj, "description", "the document", shape::At::Doc, &mut doc_level);
    shape::reference_list(
        obj.get("references"),
        "references",
        "the document",
        shape::At::Doc,
        &mut doc_level,
    );

    let items: Vec<shape::RawItem> = array(obj, "items", &mut doc_level)
        .iter()
        .enumerate()
        .filter_map(|(i, v)| shape::read_item(v, i, &mut item_level))
        .collect();
    let edges: Vec<shape::RawEdge> = array(obj, "edges", &mut doc_level)
        .iter()
        .enumerate()
        .filter_map(|(i, v)| shape::read_edge(v, i, &mut edge_level))
        .collect();
    let links = obj.get("sync").map(|s| shape::read_sync(s, &mut link_level)).unwrap_or_default();

    rules::item_rules(&items, &mut item_level);
    rules::edge_rules(&items, &edges, &mut edge_level);
    rules::link_rules(&items, &links, &mut link_level);

    // FGV-FR-04: within each group, the order the subjects appear in.
    let mut item_rank = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        item_rank.entry(item.id.clone()).or_insert(i);
    }
    let mut edge_rank = HashMap::new();
    for (i, edge) in edges.iter().enumerate() {
        edge_rank.entry(edge.id.clone()).or_insert(i);
    }
    order_by_position(&mut item_level, |v| {
        v.element_id.as_ref().and_then(|id| item_rank.get(id).copied())
    });
    order_by_position(&mut edge_level, |v| {
        v.edge_id.as_ref().and_then(|id| edge_rank.get(id).copied())
    });
    order_by_position(&mut link_level, |v| v.link_index);

    let mut violations = doc_level;
    violations.extend(item_level);
    violations.extend(edge_level);
    violations.extend(link_level);
    ValidationReport::from(violations)
}

/// A refused write's message (`PST-FR-28`, `../ui/FLO-flow.md` FLO-FR-47): every
/// violation the report named, one per line, so the Flow tab surfaces all of
/// them rather than the first.
pub fn describe(report: &ValidationReport) -> String {
    let mut lines = vec![format!(
        "This Flow could not be written: {} problem{} in the plan.",
        report.violations.len(),
        if report.violations.len() == 1 { "" } else { "s" }
    )];
    lines.extend(report.violations.iter().map(|v| format!("• {}", v.message)));
    lines.join("\n")
}

/// FGV-FR-02: `"validate flow document"`.
#[tauri::command]
pub fn validate_flow_document(app: tauri::AppHandle, body: String) -> ValidationReport {
    let report = validate(&body);
    // The body is user content and never reaches a record; its length and the
    // violation codes are what a reader debugging a refused Flow needs.
    if report.valid {
        log_debug(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "flow document validated",
            crate::log_fields! { "bytes" => body.len() },
        );
    } else {
        let codes: Vec<&str> = report.violations.iter().map(|v| v.code).collect();
        log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "flow document is not a valid Flow",
            crate::log_fields! {
                "bytes" => body.len(),
                "violations" => report.violations.len(),
                "codes" => codes.join(","),
            },
        );
    }
    report
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_rules;
