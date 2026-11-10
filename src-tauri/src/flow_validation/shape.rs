//! FGV-FR-08 and the per-field rules of `FGV-flow-graph-validation.md`: what
//! each object of a Flow document must carry, and what each field may hold.
//! Every reader appends the violations it finds and returns what the structural
//! rules of `rules.rs` need from the object.

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::model::{EdgeType, Kind, RefKind};
use super::FlowViolation;

pub const DOC_FIELDS: &[&str] =
    &["version", "name", "description", "references", "items", "edges", "sync"];
const ITEM_FIELDS: &[&str] = &[
    "id",
    "kind",
    "name",
    "description",
    "artifactIds",
    "parentId",
    "localOnly",
    "draftId",
    "footprint",
    "due",
    "position",
    "size",
];
const EDGE_FIELDS: &[&str] = &["id", "from", "to", "type"];
const POSITION_FIELDS: &[&str] = &["x", "y"];
const SIZE_FIELDS: &[&str] = &["width", "height"];
const SYNC_FIELDS: &[&str] = &["tracker", "links", "ignored"];
const LINK_FIELDS: &[&str] = &["itemId", "ref", "base"];
const REF_FIELDS: &[&str] = &["kind", "host", "owner", "repo", "number", "url"];
const ISSUE_BASE_FIELDS: &[&str] =
    &["name", "description", "state", "type", "parent", "milestone", "blockedBy"];
const MILESTONE_BASE_FIELDS: &[&str] = &["name", "description", "state", "due"];

/// Where a violation is attributed: the document, an item, an edge, or a link.
#[derive(Clone, Copy)]
pub enum At<'a> {
    Doc,
    Item(&'a str),
    Edge(&'a str),
    Link(usize),
}

impl At<'_> {
    pub fn violation(self, code: &'static str, message: String) -> FlowViolation {
        match self {
            At::Doc => FlowViolation::doc(code, message),
            At::Item(id) => FlowViolation::element(code, id, message),
            At::Edge(id) => FlowViolation::edge(code, id, message),
            At::Link(i) => FlowViolation::link(code, i, message),
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

fn number_pair(
    value: Option<&Value>,
    field: &str,
    keys: &[&str],
    place: &str,
    at: At,
    out: &mut Vec<FlowViolation>,
) {
    let Some(value) = value else {
        out.push(shape(at, place, &format!("has no `{field}`")));
        return;
    };
    let Some(obj) = value.as_object() else {
        out.push(shape(at, place, &format!("has a `{field}` that is not an object")));
        return;
    };
    unknown_fields(obj, keys, &format!("{place} `{field}`"), at, out);
    for key in keys {
        if !obj.get(*key).is_some_and(Value::is_number) {
            out.push(shape(at, place, &format!("has no number `{field}.{key}`")));
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

/// FGV-FR-RQEM: one footprint entry is a project-relative path.
fn footprint_entry_ok(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return false;
    }
    !path.split(['/', '\\']).any(|segment| segment == "..")
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
            out.push(at.violation(
                "invalid_footprint_path",
                format!(
                    "{place} has a footprint entry `{path}` that is empty, absolute, \
                     climbs out with `..`, or repeats"
                ),
            ));
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
    if obj.get("localOnly").is_some_and(|v| !v.is_boolean()) {
        out.push(shape(at, &place, "has a `localOnly` that is not a boolean"));
    }
    let draft_id = optional_string(obj, "draftId", &place, at, out);
    reference_list(obj.get("artifactIds"), "artifactIds", &place, at, out);
    number_pair(obj.get("position"), "position", POSITION_FIELDS, &place, at, out);
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
            number_pair(obj.get("size"), "size", SIZE_FIELDS, &place, at, out);
        } else if obj.contains_key("size") {
            not_allowed("size", out);
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

/// What makes two tracker references the same: host, owner, repository,
/// kind, and number.
pub type RefKey = (String, String, String, &'static str, u64);

/// A sync link as the structural rules read it.
pub struct RawLink {
    pub index: usize,
    pub item_id: String,
    pub ref_kind: RefKind,
    pub ref_key: RefKey,
}

fn read_ref(
    value: Option<&Value>,
    place: &str,
    at: At,
    out: &mut Vec<FlowViolation>,
) -> Option<(RefKind, RefKey)> {
    let Some(obj) = value.and_then(Value::as_object) else {
        out.push(shape(at, place, "has no reference object"));
        return None;
    };
    unknown_fields(obj, REF_FIELDS, place, at, out);
    let kind = match obj.get("kind").and_then(Value::as_str) {
        Some("issue") => Some(RefKind::Issue),
        Some("milestone") => Some(RefKind::Milestone),
        _ => {
            out.push(shape(at, place, "has no `kind` of `issue` or `milestone`"));
            None
        }
    };
    let host = required_id(obj, "host", place, at, out);
    let owner = required_id(obj, "owner", place, at, out);
    let repo = required_id(obj, "repo", place, at, out);
    required_id(obj, "url", place, at, out);
    let number = match obj.get("number").and_then(Value::as_u64) {
        Some(n) if n > 0 => Some(n),
        _ => {
            out.push(shape(at, place, "has no `number` greater than zero"));
            None
        }
    };
    let kind = kind?;
    let tag = match kind {
        RefKind::Issue => "issue",
        RefKind::Milestone => "milestone",
    };
    Some((kind, (host?, owner?, repo?, tag, number?)))
}

fn read_base(
    value: Option<&Value>,
    kind: RefKind,
    place: &str,
    at: At,
    out: &mut Vec<FlowViolation>,
) {
    let Some(obj) = value.and_then(Value::as_object) else {
        out.push(shape(at, place, "has no `base` object"));
        return;
    };
    let place = format!("{place} `base`");
    let fields = match kind {
        RefKind::Issue => ISSUE_BASE_FIELDS,
        RefKind::Milestone => MILESTONE_BASE_FIELDS,
    };
    unknown_fields(obj, fields, &place, at, out);
    for field in ["name", "description"] {
        if !obj.get(field).is_some_and(Value::is_string) {
            out.push(shape(at, &place, &format!("has no `{field}` string")));
        }
    }
    if !matches!(obj.get("state").and_then(Value::as_str), Some("open" | "closed")) {
        out.push(shape(at, &place, "has no `state` of `open` or `closed`"));
    }
    let null_or =
        |field: &str, ok: fn(&Value) -> bool, what: &str, out: &mut Vec<FlowViolation>| {
            if let Some(v) = obj.get(field) {
                if !v.is_null() && !ok(v) {
                    out.push(shape(at, &place, &format!("has a `{field}` that is not {what}")));
                }
            }
        };
    match kind {
        RefKind::Issue => {
            null_or("type", Value::is_string, "a string or null", out);
            null_or("milestone", |v| v.as_u64().is_some(), "a number or null", out);
            if let Some(p) = obj.get("parent") {
                if !p.is_null() {
                    read_ref(Some(p), &format!("{place} `parent`"), at, out);
                }
            }
            if let Some(list) = obj.get("blockedBy") {
                match list.as_array() {
                    Some(refs) => {
                        for r in refs {
                            read_ref(Some(r), &format!("{place} `blockedBy` entry"), at, out);
                        }
                    }
                    None => out.push(shape(at, &place, "has a `blockedBy` that is not an array")),
                }
            }
        }
        RefKind::Milestone => {
            null_or("due", |v| v.as_str().is_some_and(is_calendar_date), "a date or null", out);
        }
    }
}

/// One `sync.ignored` entry as the structural rules read it.
pub struct RawIgnored {
    pub index: usize,
    pub ref_key: RefKey,
}

/// The `sync` section as the structural rules read it.
#[derive(Default)]
pub struct RawSync {
    pub links: Vec<RawLink>,
    pub ignored: Vec<RawIgnored>,
}

/// Read the `sync` section (FGV-FR-08, FGV-FR-01). Returns its links and its
/// ignored references.
pub fn read_sync(value: &Value, out: &mut Vec<FlowViolation>) -> RawSync {
    let Some(obj) = value.as_object() else {
        out.push(FlowViolation::doc("shape", "the document has a `sync` that is not an object"));
        return RawSync::default();
    };
    unknown_fields(obj, SYNC_FIELDS, "the sync section", At::Doc, out);
    if obj.get("tracker").and_then(Value::as_str) != Some("github") {
        out.push(FlowViolation::doc("shape", "the sync section has no `tracker` of `github`"));
    }
    let links = match obj.get("links").and_then(Value::as_array) {
        Some(links) => read_links(links, out),
        None => {
            out.push(FlowViolation::doc("shape", "the sync section has no `links` array"));
            Vec::new()
        }
    };
    RawSync { links, ignored: read_ignored(obj.get("ignored"), out) }
}

fn read_links(links: &[Value], out: &mut Vec<FlowViolation>) -> Vec<RawLink> {
    let mut raw = Vec::new();
    for (index, v) in links.iter().enumerate() {
        let at = At::Link(index);
        let place = format!("sync link {index}");
        let Some(link) = v.as_object() else {
            out.push(shape(at, &place, "is not an object"));
            continue;
        };
        unknown_fields(link, LINK_FIELDS, &place, at, out);
        let item_id = required_id(link, "itemId", &place, at, out);
        let reference = read_ref(link.get("ref"), &format!("{place} `ref`"), at, out);
        if let Some((kind, _)) = &reference {
            read_base(link.get("base"), *kind, &place, at, out);
        }
        if let (Some(item_id), Some((ref_kind, ref_key))) = (item_id, reference) {
            raw.push(RawLink { index, item_id, ref_kind, ref_key });
        }
    }
    raw
}

/// FGV-FR-01: `sync.ignored` is an optional list of references. A violation
/// of an entry is about the sync section as a whole, so it carries no index;
/// its message names the entry.
fn read_ignored(value: Option<&Value>, out: &mut Vec<FlowViolation>) -> Vec<RawIgnored> {
    let Some(value) = value else { return Vec::new() };
    let Some(entries) = value.as_array() else {
        out.push(FlowViolation::doc(
            "shape",
            "the sync section has an `ignored` that is not an array",
        ));
        return Vec::new();
    };
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, v)| {
            let place = format!("sync ignored entry {index}");
            read_ref(Some(v), &place, At::Doc, out)
                .map(|(_, ref_key)| RawIgnored { index, ref_key })
        })
        .collect()
}
