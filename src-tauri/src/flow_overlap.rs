//! `FOA-flow-overlap-analysis.md`: the tasks of a delivery plan that could run
//! at the same time and are expected to change the same files.
//!
//! Pure over the body (FOA-FR-YUPB): no file is read, no path is checked on
//! disk, and no item state is considered.

use std::time::Instant;

use serde::Serialize;

use crate::flow_delivery::FlowReadError;
use crate::flow_validation::model::{parse_valid, EdgeType, FlowDocument, Hierarchy, WaitGraph};
use crate::logging::{log_info, log_warn, Domain, BUFFER};

/// One overlapping pair (the contract surface's `Overlap`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Overlap {
    /// The two item ids, in document order (FOA-FR-ZFRA).
    pub items: [String; 2],
    /// The shared paths, unique and sorted (FOA-FR-KANP).
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OverlapReport {
    pub overlaps: Vec<Overlap>,
}

/// FOA-FR-XRBA: whether entry `a` covers entry `b`. A folder entry (trailing
/// `/`) covers itself and every path below it; a file entry covers itself.
/// Paths compare segment by segment, case-sensitively.
fn covers(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if !a.ends_with('/') {
        return false;
    }
    let folder: Vec<&str> = a.trim_end_matches('/').split('/').collect();
    let path: Vec<&str> = b.trim_end_matches('/').split('/').collect();
    path.len() > folder.len() && path[..folder.len()] == folder[..]
}

/// FOA-FR-KANP: the shared paths of two footprints — for each pair of entries
/// where one covers the other, the narrower entry.
fn shared_paths(a: &[String], b: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for x in a {
        for y in b {
            if covers(x, y) {
                out.push(y.clone());
            } else if covers(y, x) {
                out.push(x.clone());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// FOA-FR-RSBH to FOA-FR-JVDK: the overlaps of a valid plan.
pub fn overlaps(doc: &FlowDocument) -> Vec<Overlap> {
    let hierarchy = Hierarchy::new(&doc.items);
    let blocks: Vec<(&str, &str, &str)> = doc
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Blocks)
        .map(|e| (e.id.as_str(), e.from.as_str(), e.to.as_str()))
        .collect();
    let graph = WaitGraph::new(&hierarchy, &blocks);

    // FOA-FR-JVDK: leaves with a footprint, in document order.
    let leaves: Vec<(&str, &[String])> = doc
        .items
        .iter()
        .filter(|i| !i.kind.is_container())
        .filter_map(|i| {
            i.footprint.as_deref().filter(|f| !f.is_empty()).map(|f| (i.id.as_str(), f))
        })
        .collect();

    let mut out = Vec::new();
    for (i, (a, fa)) in leaves.iter().enumerate() {
        for (b, fb) in &leaves[i + 1..] {
            // FOA-FR-XOAR: an ordered pair is never an overlap.
            if graph.waits_for(b, a) || graph.waits_for(a, b) {
                continue;
            }
            let paths = shared_paths(fa, fb);
            if !paths.is_empty() {
                out.push(Overlap { items: [a.to_string(), b.to_string()], paths });
            }
        }
    }
    out
}

/// FOA-FR-LBTE: `"analyze flow overlap"`.
#[tauri::command]
pub fn analyze_flow_overlap(
    app: tauri::AppHandle,
    body: String,
) -> Result<OverlapReport, FlowReadError> {
    let started = Instant::now();
    let doc = parse_valid(&body).map_err(|report| {
        log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "flow overlap refused: invalid flow",
            crate::log_fields! { "violations" => report.violations.len() },
        );
        FlowReadError::invalid(report)
    })?;
    let overlaps = overlaps(&doc);
    // FOA-FR-LXAX: counts and a duration, never a path or an item name.
    let leaves = doc.items.iter().filter(|i| !i.kind.is_container()).count();
    log_info(
        &app,
        &BUFFER,
        &[Domain::Backend],
        "flow overlap analyzed",
        crate::log_fields! {
            "leaves" => leaves,
            "overlaps" => overlaps.len(),
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(OverlapReport { overlaps })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan(items: serde_json::Value, edges: serde_json::Value) -> FlowDocument {
        let body = json!({ "version": 2, "items": items, "edges": edges }).to_string();
        parse_valid(&body).unwrap_or_else(|r| panic!("fixture invalid: {:?}", r.violations))
    }

    fn container(id: &str, kind: &str, parent: Option<&str>) -> serde_json::Value {
        let mut v = json!({ "id": id, "kind": kind, "name": id, "position": {"x":0,"y":0},
                            "size": {"width":10,"height":10} });
        if let Some(p) = parent {
            v["parentId"] = json!(p);
        }
        v
    }

    fn leaf(id: &str, parent: &str, footprint: &[&str]) -> serde_json::Value {
        let mut v = json!({ "id": id, "kind": "task", "name": id, "parentId": parent,
                            "position": {"x":0,"y":0} });
        if !footprint.is_empty() {
            v["footprint"] = json!(footprint);
        }
        v
    }

    fn skeleton(leaves: Vec<serde_json::Value>) -> serde_json::Value {
        let mut items = vec![
            container("d1", "deliverable", None),
            container("m1", "milestone", Some("d1")),
            container("f1", "feature", Some("m1")),
            container("f2", "feature", Some("m1")),
        ];
        items.extend(leaves);
        json!(items)
    }

    /// FOA-FR-RSBH, FOA-FR-XRBA, FOA-FR-KANP: a folder covers the files below
    /// it, and the shared path is the narrower entry.
    #[test]
    fn folder_and_file_entries_overlap() {
        let doc = plan(
            skeleton(vec![
                leaf("t1", "f1", &["src/payments/", "docs/a.md"]),
                leaf("t2", "f2", &["src/payments/risk.rs", "docs/a.md", "docs/b.md"]),
            ]),
            json!([]),
        );
        assert_eq!(
            overlaps(&doc),
            vec![Overlap {
                items: ["t1".into(), "t2".into()],
                paths: vec!["docs/a.md".into(), "src/payments/risk.rs".into()],
            }]
        );
    }

    /// FOA-FR-XRBA: comparison is per segment and case-sensitive; a file entry
    /// covers only itself.
    #[test]
    fn segment_and_case_rules() {
        let doc = plan(
            skeleton(vec![
                leaf("t1", "f1", &["src/pay/", "README.md"]),
                leaf("t2", "f2", &["src/payments/x.rs", "readme.md", "README.md/x"]),
            ]),
            json!([]),
        );
        assert!(overlaps(&doc).is_empty());
    }

    /// FOA-FR-XOAR: an ordered pair never overlaps, directly or through a
    /// container; a `relates` edge orders nothing.
    #[test]
    fn ordered_pairs_do_not_overlap() {
        let leaves =
            || skeleton(vec![leaf("t1", "f1", &["src/"]), leaf("t2", "f2", &["src/a.rs"])]);
        let blocked =
            plan(leaves(), json!([{ "id": "e1", "from": "f1", "to": "f2", "type": "blocks" }]));
        assert!(overlaps(&blocked).is_empty());
        let related =
            plan(leaves(), json!([{ "id": "e1", "from": "t1", "to": "t2", "type": "relates" }]));
        assert_eq!(overlaps(&related).len(), 1);
    }

    /// FOA-FR-JVDK, FOA-FR-ZFRA: items with no footprint are in no overlap, and
    /// overlaps come in document order, each pair once.
    #[test]
    fn order_and_empty_footprints() {
        let doc = plan(
            skeleton(vec![
                leaf("t1", "f1", &["a/"]),
                leaf("t2", "f1", &[]),
                leaf("t3", "f2", &["a/x"]),
                leaf("t4", "f2", &["a/y"]),
            ]),
            json!([]),
        );
        let pairs: Vec<[String; 2]> = overlaps(&doc).into_iter().map(|o| o.items).collect();
        assert_eq!(
            pairs,
            vec![["t1".to_string(), "t3".to_string()], ["t1".to_string(), "t4".to_string()]]
        );
    }

    /// FOA-FR-YUPB, FOA-FR-LBTE: the same body always yields the same report,
    /// and an empty body yields none.
    #[test]
    fn pure_and_empty() {
        let doc =
            plan(skeleton(vec![leaf("t1", "f1", &["a/"]), leaf("t2", "f2", &["a/"])]), json!([]));
        assert_eq!(overlaps(&doc), overlaps(&doc));
        assert!(overlaps(&parse_valid("").unwrap()).is_empty());
        assert!(parse_valid(r#"{"version":1}"#).is_err());
    }
}
