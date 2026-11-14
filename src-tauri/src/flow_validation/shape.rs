//! FGV-FR-08 and the per-field rules of `FGV-flow-graph-validation.md`: what
//! each object of a Flow document must carry, and what each field may hold.
//! Every reader appends the violations it finds and returns what the structural
//! rules of `rules.rs` need from the object.

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::field_rules::footprint_entry_ok;
use super::model::{EdgeType, Kind};
use super::FlowViolation;

pub const DOC_FIELDS: &[&str] = &["version", "name", "description", "references", "items", "edges"];
const ITEM_FIELDS: &[&str] = &[
    "id",
    "kind",
    "name",
    "description",
    "artifactIds",
    "parentId",
    "draftId",
    "footprint",
    "due",
];
const EDGE_FIELDS: &[&str] = &["id", "from", "to", "type"];

/// Where a violation is attributed: the document, an item, or an edge.
#[derive(Clone, Copy)]
pub enum At<'a> {
    Doc,
    Item(&'a str),
    Edge(&'a str),
}

impl At<'_> {
    pub fn violation(self, code: &'static str, message: String) -> FlowViolation {
        match self {
            At::Doc => FlowViolation::doc(code, message),
            At::Item(id) => FlowViolation::element(code, id, message),
            At::Edge(id) => FlowViolation::edge(code, id, message),
        }
    }
}

fn shape(at: At, place: &str, what: &str) -> FlowViolation {
    at.violation("shape", format!("{place} {what}"))
}

/// FGV-FR-08: flag every field of `obj` that the schema does not define.
pub fn unknown_fields(
    obj: &Map<String, Value>,
    known: &[&str],
    place: &str,
    at: At,
    out: &mut Vec<FlowViolation>,
) {
    for key in obj.keys() {
        if !known.contains(&key.as_str()) {
            out.push(at.violation(
                "unknown_field",
                format!("{place} carries a field `{key}` the Flow schema does not define"),
            ));
        }
    }
}

pub fn optional_string(
    obj: &Map<String, Value>,
    field: &str,
    place: &str,
    at: At,
    out: &mut Vec<FlowViolation>,
) -> Option<String> {
    match obj.get(field) {
        None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => {
            out.push(shape(at, place, &format!("has a `{field}` that is not a string")));
            None
        }
    }
}

/// A required, non-empty string, returned so the caller can use it as an id.
pub fn required_id(
    obj: &Map<String, Value>,
    field: &str,
    place: &str,
    at: At,
    out: &mut Vec<FlowViolation>,
) -> Option<String> {
    match obj.get(field).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        Some(_) => {
            out.push(shape(at, place, &format!("has an empty `{field}`")));
            None
        }
        None => {
            out.push(shape(at, place, &format!("has no `{field}` string")));
            None
        }
    }
}

/// FGV-FR-08 / FGV-FR-JDGO: a list of artifact ids. Each entry is a non-empty
/// string, and no entry repeats.
pub fn reference_list(
    value: Option<&Value>,
    field: &str,
    place: &str,
    at: At,
    out: &mut Vec<FlowViolation>,
) {
    let Some(value) = value else { return };
    let Some(items) = value.as_array() else {
        out.push(shape(at, place, &format!("has a `{field}` that is not an array")));
        return;
    };
    let mut seen = HashSet::new();
    for v in items {
        match v.as_str() {
            Some(s) if !s.is_empty() => {
                if !seen.insert(s) {
                    out.push(at.violation(
                        "duplicate_reference",
                        format!("{place} references `{s}` more than once in `{field}`"),
                    ));
                }
            }
            _ => out.push(shape(
                at,
                place,
                &format!("has a `{field}` entry that is not a non-empty string"),
            )),
        }
    }
}

fn footprint(value: &Value, place: &str, at: At, out: &mut Vec<FlowViolation>) {
    let Some(entries) = value.as_array() else {
        out.push(shape(at, place, "has a `footprint` that is not an array"));
        return;
    };
    let mut seen = HashSet::new();
    for v in entries {
        let Some(path) = v.as_str() else {
            out.push(shape(at, place, "has a `footprint` entry that is not a string"));
            continue;
        };
        if !footprint_entry_ok(path) || !seen.insert(path) {
            if let At::Item(id) = at {
                out.push(super::field_rules::invalid_footprint(id, place, path));
            }
        }
    }
}

/// FGV-FR-RRND: a calendar date in the form `YYYY-MM-DD`.
pub fn is_calendar_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = &s[r];
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok()).flatten()
    };
    let (Some(y), Some(m), Some(d)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// An item as the structural rules read it.
pub struct RawItem {
    pub id: String,
    pub kind: Option<Kind>,
    pub parent: Option<String>,
    pub draft_id: Option<String>,
}

/// Read one item's shape (FGV-FR-08, FGV-FR-MZIA, FGV-FR-RQEM, FGV-FR-RRND,
/// FGV-FR-JDGO).
pub fn read_item(value: &Value, index: usize, out: &mut Vec<FlowViolation>) -> Option<RawItem> {
    let Some(obj) = value.as_object() else {
        out.push(FlowViolation::doc("shape", format!("item {index} is not an object")));
        return None;
    };
    let id = required_id(obj, "id", &format!("item {index}"), At::Doc, out)?;
    let place = format!("item `{id}`");
    let at = At::Item(&id);

    unknown_fields(obj, ITEM_FIELDS, &place, at, out);
    let kind = match obj.get("kind").and_then(Value::as_str) {
        Some(s) => {
            let k = Kind::parse(s);
            if k.is_none() {
                out.push(shape(at, &place, &format!("has a `kind` `{s}` that is no item kind")));
            }
            k
        }
        None => {
            out.push(shape(at, &place, "has no `kind` string"));
            None
        }
    };
    if !obj.get("name").is_some_and(Value::is_string) {
        out.push(shape(at, &place, "has no `name` string"));
    }
    optional_string(obj, "description", &place, at, out);
    let parent = optional_string(obj, "parentId", &place, at, out);
    let draft_id = optional_string(obj, "draftId", &place, at, out);
    reference_list(obj.get("artifactIds"), "artifactIds", &place, at, out);
    if let Some(fp) = obj.get("footprint") {
        footprint(fp, &place, at, out);
    }
    let due = optional_string(obj, "due", &place, at, out);
    if let Some(due) = &due {
        if !is_calendar_date(due) {
            out.push(at.violation(
                "invalid_due_date",
                format!("{place} has a `due` `{due}` that is not a date in the form YYYY-MM-DD"),
            ));
        }
    }

    if let Some(k) = kind {
        let not_allowed = |field: &str, out: &mut Vec<FlowViolation>| {
            out.push(at.violation(
                "field_not_allowed_for_kind",
                format!("{place} is a {} and carries `{field}`", k.as_str()),
            ));
        };
        if k.is_container() {
            for field in ["draftId", "footprint"] {
                if obj.contains_key(field) {
                    not_allowed(field, out);
                }
            }
        }
        if k != Kind::Milestone && obj.contains_key("due") {
            not_allowed("due", out);
        }
    }

    Some(RawItem { id, kind, parent, draft_id })
}

/// An edge as the structural rules read it.
pub struct RawEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub edge_type: Option<EdgeType>,
}

pub fn read_edge(value: &Value, index: usize, out: &mut Vec<FlowViolation>) -> Option<RawEdge> {
    let Some(obj) = value.as_object() else {
        out.push(FlowViolation::doc("shape", format!("edge {index} is not an object")));
        return None;
    };
    let id = required_id(obj, "id", &format!("edge {index}"), At::Doc, out)?;
    let place = format!("edge `{id}`");
    let at = At::Edge(&id);
    unknown_fields(obj, EDGE_FIELDS, &place, at, out);
    let edge_type = match obj.get("type").and_then(Value::as_str) {
        Some("blocks") => Some(EdgeType::Blocks),
        Some("relates") => Some(EdgeType::Relates),
        _ => {
            out.push(shape(at, &place, "has no `type` of `blocks` or `relates`"));
            None
        }
    };
    let from = required_id(obj, "from", &place, at, out);
    let to = required_id(obj, "to", &place, at, out);
    Some(RawEdge { id, from: from?, to: to?, edge_type })
}
