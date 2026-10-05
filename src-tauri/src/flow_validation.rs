//! `FGV-flow-graph-validation.md`: the backend authority on what a Flow document
//! is.
//!
//! This module owns the Flow document schema (FGV-FR-01) and the structural
//! rules its graph obeys, and answers one question about a body of text: is this
//! a Flow, and if not, exactly what is wrong with it. A Flow is a plain JSON file
//! committed to the repository — an agent writes one, a merge resolves one, a
//! person edits one by hand — and the canvas that enforces these rules while an
//! author draws is present for none of that.
//!
//! Everything here is pure (FGV-FR-02): no filesystem, no project, no event, no
//! state between calls. That is what lets `save_artifact_contents` call it inside
//! a write (`PST-FR-28`) and the Flow tab call it on a body that has never been
//! on disk (`../ui/FLO-flow.md` FLO-FR-46).

use serde::Serialize;
use serde_json::Value;

use crate::logging::{log_debug, log_warn, Domain, BUFFER};

/// The one document version this build reads (FGV-FR-07).
const FLOW_VERSION: u64 = 1;

/// FGV-FR-08: the fields the schema defines, per object kind. Anything else in
/// one of these positions is an `unknown_field` — a field nothing reads is more
/// often a misspelling of one that matters than an extension of the format.
const DOC_FIELDS: &[&str] = &["version", "name", "description", "loops", "nodes", "edges"];
/// `artifactId` is the singular form a node carried before it could reference
/// more than one artifact. It is part of this schema's history rather than a
/// field nobody defined, so a document still using it validates and the editor
/// migrates it to `artifactIds` on the next write.
const NODE_FIELDS: &[&str] = &[
    "id",
    "name",
    "artifactIds",
    "artifactId",
    "prompt",
    "parentId",
    "position",
];
/// A loop carries no `prompt` and no `artifactId`: what it is run under is the
/// artifact it references (`../ui/FLO-flow.md` FLO-FR-37), and no loop was ever
/// written with the singular form a node kept, so accepting it here would be
/// inventing history rather than keeping it.
const LOOP_FIELDS: &[&str] = &[
    "id",
    "name",
    "artifactIds",
    "maxPasses",
    "parentId",
    "position",
    "size",
];
const EDGE_FIELDS: &[&str] = &["id", "from", "to", "label"];
const POSITION_FIELDS: &[&str] = &["x", "y"];
const SIZE_FIELDS: &[&str] = &["width", "height"];

/// One thing wrong with a document (FGV-FR-04). `element_id` names the node or
/// loop it sits on, `edge_id` names the edge, and a violation about the document
/// as a whole carries neither.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowViolation {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
}

impl FlowViolation {
    fn doc(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), element_id: None, edge_id: None }
    }

    fn element(code: &'static str, id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            element_id: Some(id.into()),
            edge_id: None,
        }
    }

    fn edge(code: &'static str, id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            element_id: None,
            edge_id: Some(id.into()),
        }
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

/// One element (node or loop) as the validator reads it, once its own shape has
/// been checked. Kept flat because every structural rule below is about ids and
/// parentage rather than about what kind of element carries them.
struct Element {
    id: String,
    parent: Option<String>,
    is_loop: bool,
}

/// Whether `v` is an object, for the "is this the right shape" checks.
fn as_object(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    v.as_object()
}

/// FGV-FR-08: flag every field of `obj` the schema does not define. `at` names
/// the position in the document a reader would look for it in.
fn unknown_fields(
    obj: &serde_json::Map<String, Value>,
    known: &[&str],
    at: &str,
    element_id: Option<&str>,
    out: &mut Vec<FlowViolation>,
) {
    for key in obj.keys() {
        if known.contains(&key.as_str()) {
            continue;
        }
        let message = format!("{at} carries a field `{key}` the Flow schema does not define");
        out.push(match element_id {
            Some(id) => FlowViolation::element("unknown_field", id, message),
            None => FlowViolation::doc("unknown_field", message),
        });
    }
}

/// A `shape` violation, which is the same statement wherever it is raised: this
/// field is missing, or is not the type the schema gives it.
fn shape(at: &str, what: &str, element_id: Option<&str>) -> FlowViolation {
    let message = format!("{at} {what}");
    match element_id {
        Some(id) => FlowViolation::element("shape", id, message),
        None => FlowViolation::doc("shape", message),
    }
}

/// FGV-FR-08 / FGV-FR-15: a `{ x, y }` or `{ width, height }` block. Its numbers
/// are checked for being numbers and for nothing else — where an element sits is
/// the canvas's business and never this module's.
fn check_number_pair(
    value: Option<&Value>,
    field: &str,
    keys: &[&str],
    at: &str,
    element_id: &str,
    required: bool,
    out: &mut Vec<FlowViolation>,
) {
    let Some(value) = value else {
        if required {
            out.push(shape(at, &format!("has no `{field}`"), Some(element_id)));
        }
        return;
    };
    let Some(obj) = as_object(value) else {
        out.push(shape(at, &format!("`{field}` is not an object"), Some(element_id)));
        return;
    };
    unknown_fields(obj, keys, &format!("{at} `{field}`"), Some(element_id), out);
    for key in keys {
        match obj.get(*key) {
            None => out.push(shape(
                at,
                &format!("`{field}` has no `{key}`"),
                Some(element_id),
            )),
            Some(v) if !v.is_f64() && !v.is_i64() && !v.is_u64() => out.push(shape(
                at,
                &format!("`{field}.{key}` is not a number"),
                Some(element_id),
            )),
            Some(_) => {}
        }
    }
}

/// An optional string field: absent is fine, present-and-not-a-string is not.
fn check_optional_string(
    obj: &serde_json::Map<String, Value>,
    field: &str,
    at: &str,
    element_id: Option<&str>,
    out: &mut Vec<FlowViolation>,
) {
    match obj.get(field) {
        None | Some(Value::Null) => {}
        Some(v) if v.is_string() => {}
        Some(_) => out.push(shape(at, &format!("`{field}` is not a string"), element_id)),
    }
}

/// A required string field, returning it when it is one so the caller can go on
/// using it as an identity.
fn required_string(
    obj: &serde_json::Map<String, Value>,
    field: &str,
    at: &str,
    element_id: Option<&str>,
    out: &mut Vec<FlowViolation>,
) -> Option<String> {
    match obj.get(field).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        Some(_) => {
            out.push(shape(at, &format!("`{field}` is empty"), element_id));
            None
        }
        None => {
            out.push(shape(at, &format!("has no `{field}`"), element_id));
            None
        }
    }
}

/// FGV-FR-08: an element's `artifactIds` — a loop's as much as a node's
/// (`../ui/FLO-flow.md` FLO-FR-37) — which must be a list of strings when
/// present. Its *contents* are not checked against the project — an id naming a
/// deleted artifact is a Flow to be repaired in the editor, not a document to be
/// refused (FGV-FR-15).
fn check_artifact_ids(
    obj: &serde_json::Map<String, Value>,
    at: &str,
    element_id: &str,
    out: &mut Vec<FlowViolation>,
) {
    match obj.get("artifactIds") {
        None | Some(Value::Null) => {}
        Some(Value::Array(items)) => {
            // An empty entry is refused alongside a non-string one: an artifact
            // id is a path-derived node key (ASC-FR-13) and `""` names nothing,
            // so a document carrying one is malformed rather than merely
            // pointing at a deleted artifact (which FGV-FR-15 permits).
            if items.iter().any(|v| v.as_str().is_none_or(str::is_empty)) {
                out.push(shape(
                    at,
                    "has an `artifactIds` entry that is not a non-empty string",
                    Some(element_id),
                ));
            }
        }
        Some(_) => out.push(shape(at, "`artifactIds` is not an array", Some(element_id))),
    }
}

/// FGV-FR-14: a pass bound, when present, is a whole number greater than zero.
/// Its absence is not a violation — a loop bounded by nothing but the prompt it
/// references is an ordinary loop.
fn check_max_passes(
    obj: &serde_json::Map<String, Value>,
    element_id: &str,
    out: &mut Vec<FlowViolation>,
) {
    match obj.get("maxPasses") {
        None | Some(Value::Null) => {}
        Some(v) => {
            let ok = v.as_u64().is_some_and(|n| n > 0);
            if !ok {
                out.push(FlowViolation::element(
                    "invalid_max_passes",
                    element_id,
                    format!(
                        "loop `{element_id}` has a `maxPasses` that is not a whole number greater than zero"
                    ),
                ));
            }
        }
    }
}

/// Read one element's shape, appending every violation it carries and returning
/// what the structural rules below need from it.
fn read_element(
    value: &Value,
    index: usize,
    is_loop: bool,
    out: &mut Vec<FlowViolation>,
) -> Option<Element> {
    let kind = if is_loop { "loop" } else { "node" };
    let Some(obj) = as_object(value) else {
        out.push(FlowViolation::doc(
            "shape",
            format!("{kind} {index} is not an object"),
        ));
        return None;
    };
    // The id is read first and used to attribute every other violation on this
    // element, so a report about a node names the node rather than its index.
    let id = required_string(obj, "id", &format!("{kind} {index}"), None, out)?;
    let at = format!("{kind} `{id}`");

    unknown_fields(
        obj,
        if is_loop { LOOP_FIELDS } else { NODE_FIELDS },
        &at,
        Some(&id),
        out,
    );
    match obj.get("name") {
        Some(v) if v.is_string() => {}
        Some(_) => out.push(shape(&at, "`name` is not a string", Some(&id))),
        None => out.push(shape(&at, "has no `name`", Some(&id))),
    }
    check_optional_string(obj, "parentId", &at, Some(&id), out);
    check_number_pair(
        obj.get("position"),
        "position",
        POSITION_FIELDS,
        &at,
        &id,
        true,
        out,
    );
    // FLO-FR-10: artifact references are an element's, node and loop alike.
    check_artifact_ids(obj, &at, &id, out);
    if is_loop {
        check_max_passes(obj, &id, out);
        check_number_pair(obj.get("size"), "size", SIZE_FIELDS, &at, &id, true, out);
    } else {
        check_optional_string(obj, "prompt", &at, Some(&id), out);
        check_optional_string(obj, "artifactId", &at, Some(&id), out);
    }

    Some(Element {
        id,
        parent: obj.get("parentId").and_then(Value::as_str).map(str::to_string),
        is_loop,
    })
}

/// FGV-FR-02: judge `body` against the Flow schema and the graph's structural
/// rules. Pure — the same body always yields the same report.
pub fn validate(body: &str) -> ValidationReport {
    // FGV-FR-05: an empty or whitespace-only body is the empty graph, so a Flow
    // created by New Artifact, New File, or an external `touch` validates.
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
            ))
        }
    };

    // FGV-FR-07: likewise alone — rules written for version 1 say nothing useful
    // about a document that does not claim to be one.
    let obj = match as_object(&raw) {
        Some(o) if o.get("version").and_then(Value::as_u64) == Some(FLOW_VERSION) => o,
        _ => {
            let found = as_object(&raw)
                .and_then(|o| o.get("version"))
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".to_string());
            return ValidationReport::single(FlowViolation::doc(
                "unknown_version",
                format!("expected a Flow document of version {FLOW_VERSION}, found {found}"),
            ));
        }
    };

    // FGV-FR-04: document-level violations first, then element, then edge.
    let mut doc_level: Vec<FlowViolation> = Vec::new();
    let mut element_level: Vec<FlowViolation> = Vec::new();
    let mut edge_level: Vec<FlowViolation> = Vec::new();

    unknown_fields(obj, DOC_FIELDS, "the document", None, &mut doc_level);
    check_optional_string(obj, "name", "the document", None, &mut doc_level);
    check_optional_string(obj, "description", "the document", None, &mut doc_level);

    let array = |field: &str, required: bool, out: &mut Vec<FlowViolation>| match obj.get(field) {
        None | Some(Value::Null) if !required => Vec::new(),
        Some(Value::Array(items)) => items.clone(),
        None | Some(Value::Null) => {
            out.push(shape("the document", &format!("has no `{field}`"), None));
            Vec::new()
        }
        Some(_) => {
            out.push(shape("the document", &format!("`{field}` is not an array"), None));
            Vec::new()
        }
    };

    // `loops` may be absent on a document holding none; `nodes` and `edges` are
    // required, so a document missing one is malformed rather than empty.
    let loops = array("loops", false, &mut doc_level);
    let nodes = array("nodes", true, &mut doc_level);
    let edges = array("edges", true, &mut doc_level);

    // Elements in document order, loops before nodes — the order the schema
    // lists them, which is what makes the report's order stable (FGV-FR-04).
    let mut elements: Vec<Element> = Vec::new();
    for (i, value) in loops.iter().enumerate() {
        if let Some(e) = read_element(value, i, true, &mut element_level) {
            elements.push(e);
        }
    }
    for (i, value) in nodes.iter().enumerate() {
        if let Some(e) = read_element(value, i, false, &mut element_level) {
            elements.push(e);
        }
    }

    // FGV-FR-09: nodes and loops share one identifier space.
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for e in &elements {
        if !seen_ids.insert(e.id.as_str()) {
            element_level.push(FlowViolation::element(
                "duplicate_id",
                &e.id,
                format!("more than one element carries the id `{}`", e.id),
            ));
        }
    }

    let loop_ids: std::collections::HashSet<&str> = elements
        .iter()
        .filter(|e| e.is_loop)
        .map(|e| e.id.as_str())
        .collect();
    let parent_of: std::collections::HashMap<&str, Option<&str>> = elements
        .iter()
        .map(|e| (e.id.as_str(), e.parent.as_deref()))
        .collect();

    // FGV-FR-13: a `parentId` names a loop, and containment is acyclic.
    for e in &elements {
        let Some(parent) = e.parent.as_deref() else { continue };
        if !loop_ids.contains(parent) {
            element_level.push(FlowViolation::element(
                "dangling_parent",
                &e.id,
                format!("`{}` names a parent `{parent}` that is not a loop in this document", e.id),
            ));
            continue;
        }
        // Walk up from the parent; arriving back at this element means the chain
        // closes on itself. Bounded by the element count, so a cycle terminates.
        let mut at = Some(parent);
        let mut steps = 0usize;
        while let Some(current) = at {
            if current == e.id {
                element_level.push(FlowViolation::element(
                    "containment_cycle",
                    &e.id,
                    format!("loop `{}` contains itself through its chain of parents", e.id),
                ));
                break;
            }
            steps += 1;
            if steps > elements.len() {
                break;
            }
            at = parent_of.get(current).copied().flatten();
        }
    }

    // Edges. Their ids share the uniqueness requirement among themselves
    // (FGV-FR-09), and the three rules on what they may join follow.
    let element_ids: std::collections::HashSet<&str> =
        elements.iter().map(|e| e.id.as_str()).collect();
    let mut seen_edge_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_pairs: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    for (i, value) in edges.iter().enumerate() {
        let Some(eobj) = as_object(value) else {
            edge_level.push(FlowViolation::doc(
                "shape",
                format!("edge {i} is not an object"),
            ));
            continue;
        };
        let Some(id) = required_string(eobj, "id", &format!("edge {i}"), None, &mut edge_level)
        else {
            continue;
        };
        let at = format!("edge `{id}`");
        unknown_fields(eobj, EDGE_FIELDS, &at, None, &mut edge_level);
        check_optional_string(eobj, "label", &at, None, &mut edge_level);

        if !seen_edge_ids.insert(id.clone()) {
            edge_level.push(FlowViolation::edge(
                "duplicate_id",
                &id,
                format!("more than one edge carries the id `{id}`"),
            ));
        }

        let from = eobj.get("from").and_then(Value::as_str);
        let to = eobj.get("to").and_then(Value::as_str);
        let (Some(from), Some(to)) = (from, to) else {
            edge_level.push(FlowViolation::edge(
                "shape",
                &id,
                format!("edge `{id}` has no `from` and `to` pair of strings"),
            ));
            continue;
        };

        // FGV-FR-10: an endpoint may name a node or a loop; both are elements
        // the graph connects.
        let mut dangling = false;
        for (which, endpoint) in [("from", from), ("to", to)] {
            if !element_ids.contains(endpoint) {
                dangling = true;
                edge_level.push(FlowViolation::edge(
                    "dangling_edge_endpoint",
                    &id,
                    format!("edge `{id}` has a `{which}` naming `{endpoint}`, which is no element of this document"),
                ));
            }
        }

        // FGV-FR-11: no self-edge, and at most one edge per ordered pair.
        if from == to {
            edge_level.push(FlowViolation::edge(
                "self_edge",
                &id,
                format!("edge `{id}` joins `{from}` to itself"),
            ));
        } else if !seen_pairs.insert((from.to_string(), to.to_string())) {
            edge_level.push(FlowViolation::edge(
                "duplicate_edge",
                &id,
                format!("more than one edge runs from `{from}` to `{to}`"),
            ));
        }

        // FGV-FR-12: both endpoints share a container. Skipped when either
        // endpoint does not resolve — "it crosses a boundary" says nothing
        // useful about an edge one end of which names nothing.
        if !dangling && from != to {
            let a = parent_of.get(from).copied().flatten();
            let b = parent_of.get(to).copied().flatten();
            if a != b {
                let name = |c: Option<&str>| match c {
                    Some(loop_id) => format!("loop `{loop_id}`"),
                    None => "the top level".to_string(),
                };
                edge_level.push(FlowViolation::edge(
                    "cross_container_edge",
                    &id,
                    format!(
                        "edge `{id}` joins `{from}` in {} to `{to}` in {}",
                        name(a),
                        name(b)
                    ),
                ));
            }
        }
    }

    let mut violations = doc_level;
    violations.extend(element_level);
    violations.extend(edge_level);
    ValidationReport::from(violations)
}

/// A refused write's message (`PST-FR-28`, `../ui/FLO-flow.md` FLO-FR-47): every
/// violation the report named, one per line, so the Flow tab surfaces all of
/// them rather than the first.
pub fn describe(report: &ValidationReport) -> String {
    let mut lines = vec![format!(
        "This Flow could not be written: {} problem{} in the graph.",
        report.violations.len(),
        if report.violations.len() == 1 { "" } else { "s" }
    )];
    lines.extend(report.violations.iter().map(|v| format!("• {}", v.message)));
    lines.join("\n")
}

/// FGV-FR-02: `"validate flow document"`.
///
/// Takes the document's text and returns the report. It reads no file and opens
/// no project, so it can be called on a body that has never been on disk and on
/// one about to replace one.
#[tauri::command]
pub fn validate_flow_document(app: tauri::AppHandle, body: String) -> ValidationReport {
    let report = validate(&body);
    // The body itself is user content and never reaches a record; its length and
    // the violation codes are what a reader debugging a refused Flow needs.
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
