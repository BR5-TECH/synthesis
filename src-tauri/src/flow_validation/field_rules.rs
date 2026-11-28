//! The field rules of `FGV-flow-graph-validation.md` on typed values: a
//! footprint (FGV-FR-RQEM), a list of references (FGV-FR-JDGO), draft links
//! (FGV-FR-CTFX), and the ends of an edge (FGV-FR-11).
//!
//! The validator reads raw JSON and uses these rules for the parts that are
//! not about shape. The per-machine overlay of `DMS-delivery-machine-state.md`
//! (DMS-FR-QFEA) holds the same fields outside a document and obeys the same
//! rules, so it calls them too. Each rule is pure and appends its violations.

use std::collections::HashSet;

use super::FlowViolation;

/// FGV-FR-RQEM: whether one footprint entry is a project-relative path. An
/// entry is refused when it is empty, absolute (a leading `/` or `\`, or a
/// drive letter), or holds a `..` segment.
pub fn footprint_entry_ok(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return false;
    }
    !path.split(['/', '\\']).any(|segment| segment == "..")
}

/// FGV-FR-RQEM: the `invalid_footprint_path` violation of one entry.
pub(super) fn invalid_footprint(item_id: &str, place: &str, path: &str) -> FlowViolation {
    FlowViolation::element(
        "invalid_footprint_path",
        item_id,
        format!(
            "{place} has a footprint entry `{path}` that is empty, absolute, \
             climbs out with `..`, or repeats"
        ),
    )
}

/// FGV-FR-RQEM: each footprint entry of the item `item_id` that is not a
/// project-relative path, or that repeats an earlier entry of the item.
pub fn check_footprint(item_id: &str, entries: &[String], out: &mut Vec<FlowViolation>) {
    let place = format!("item `{item_id}`");
    let mut seen = HashSet::new();
    for path in entries {
        if !footprint_entry_ok(path) || !seen.insert(path.as_str()) {
            out.push(invalid_footprint(item_id, &place, path));
        }
    }
}

/// FGV-FR-JDGO, FGV-FR-08: a list of references. An entry that is empty is
/// `shape`, and an entry that repeats an earlier one is `duplicate_reference`.
/// `item_id` is the item that holds the list, or `None` for the plan's own
/// `references`.
pub fn check_references(
    item_id: Option<&str>,
    field: &str,
    ids: &[String],
    out: &mut Vec<FlowViolation>,
) {
    let place = match item_id {
        Some(id) => format!("item `{id}`"),
        None => "the document".to_string(),
    };
    let violation = |code: &'static str, message: String| match item_id {
        Some(id) => FlowViolation::element(code, id, message),
        None => FlowViolation::doc(code, message),
    };
    let mut seen = HashSet::new();
    for id in ids {
        if id.is_empty() {
            out.push(violation(
                "shape",
                format!("{place} has a `{field}` entry that is not a non-empty string"),
            ));
        } else if !seen.insert(id.as_str()) {
            out.push(violation(
                "duplicate_reference",
                format!("{place} references `{id}` more than once in `{field}`"),
            ));
        }
    }
}

/// FGV-FR-CTFX: `duplicate_draft_link` on the second and each later item, in
/// the order given, whose draft id an earlier item already holds. `links` are
/// `(item id, draft id)` pairs.
pub fn check_draft_links<'a>(
    links: impl IntoIterator<Item = (&'a str, &'a str)>,
    out: &mut Vec<FlowViolation>,
) {
    let mut drafts = HashSet::new();
    for (item_id, draft_id) in links {
        if !drafts.insert(draft_id) {
            out.push(FlowViolation::element(
                "duplicate_draft_link",
                item_id,
                format!(
                    "`{item_id}` links the change `{draft_id}`, which another item already links"
                ),
            ));
        }
    }
}

/// FGV-FR-11: `self_edge` on an edge whose ends are the same item, and
/// `duplicate_edge` on the second and each later edge between two items that
/// an earlier edge joins, whatever its direction. `edges` are `(label, from,
/// to)` in order; the label names the edge in the message. The violations are
/// about the list as a whole, so they carry no item or edge id.
pub fn check_edge_ends<'a>(
    edges: impl IntoIterator<Item = (String, &'a str, &'a str)>,
    out: &mut Vec<FlowViolation>,
) {
    let mut pairs = HashSet::new();
    for (label, from, to) in edges {
        if from == to {
            out.push(FlowViolation::doc("self_edge", format!("{label} joins `{from}` to itself")));
            continue;
        }
        let pair = if from < to { (from, to) } else { (to, from) };
        if !pairs.insert(pair) {
            out.push(FlowViolation::doc(
                "duplicate_edge",
                format!("{label} joins `{from}` and `{to}`, which an earlier edge joins"),
            ));
        }
    }
}
