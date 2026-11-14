//! The layout of a delivery canvas (`DMS-delivery-machine-state.md`
//! DMS-FR-DMWD to DMS-FR-TJSJ): for each item of a scope, where it sits, how
//! large a container is, and whether a container is collapsed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fs::RootFs;
use crate::logging::{LogBuffer, LogSink};

use super::super::{DeliveryError, ERR_INVALID_LAYOUT};
use super::records::{
    read_record, record_path, remove_record, scope_digest, write_record, LAYOUT_DIRNAME,
};

/// DMS-FR-DMWD: a layout scope. `github:<project-node-id>` is the GitHub plan
/// of a Project, and `flow:<artifact-id>` is a Flow file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutScope<'a> {
    Github(&'a str),
    Flow(&'a str),
}

impl LayoutScope<'_> {
    /// The kind of the scope, which is all that a log record names of it
    /// (DMS-FR-ROMF).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Github(_) => "github",
            Self::Flow(_) => "flow",
        }
    }
}

/// DMS-FR-DMWD, DMS-FR-TJSJ: read `scope` as a layout scope. The text after
/// the prefix is not empty.
pub fn parse_scope(scope: &str) -> Option<LayoutScope<'_>> {
    if let Some(id) = scope.strip_prefix("github:") {
        return (!id.is_empty()).then_some(LayoutScope::Github(id));
    }
    if let Some(id) = scope.strip_prefix("flow:") {
        return (!id.is_empty()).then_some(LayoutScope::Flow(id));
    }
    None
}

/// A position, measured from the origin of the item's container.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// The size of a container item.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Extent {
    pub width: f64,
    pub height: f64,
}

/// DMS-FR-DMWD: one item's entry in a scope. Each field is absent when unset.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Extent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
}

/// DMS contract surface `DeliveryLayout`. The entries are kept by item id in
/// a fixed order, so the same layout always writes the same file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeliveryLayout {
    #[serde(default)]
    pub items: BTreeMap<String, LayoutEntry>,
}

/// The record file. It also holds the scope, so a reader can see which scope
/// a digest names.
#[derive(Serialize, Deserialize)]
struct LayoutRecord {
    scope: String,
    #[serde(default)]
    items: BTreeMap<String, LayoutEntry>,
}

/// DMS-FR-TJSJ: whether every position and size of `layout` is finite, and
/// every size is positive.
fn layout_is_valid(layout: &DeliveryLayout) -> bool {
    layout.items.values().all(|entry| {
        let position_ok = entry.position.is_none_or(|p| p.x.is_finite() && p.y.is_finite());
        let size_ok = entry.size.is_none_or(|s| {
            s.width.is_finite() && s.height.is_finite() && s.width > 0.0 && s.height > 0.0
        });
        position_ok && size_ok
    })
}

fn invalid(message: &str) -> DeliveryError {
    DeliveryError::new(ERR_INVALID_LAYOUT, message)
}

/// DMS-FR-DPXR: the layout of `scope`. A scope with no stored layout, a
/// record that cannot be parsed (DMS-FR-ETLQ), and a text that is no scope
/// each return an empty layout. DMS-FR-EKKY: an entry of an item the plan
/// does not hold is returned as stored; the reader ignores it.
pub fn load_layout<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &RootFs,
    scope: &str,
) -> Result<DeliveryLayout, DeliveryError> {
    if parse_scope(scope).is_none() {
        return Ok(DeliveryLayout::default());
    }
    let path = record_path(store, LAYOUT_DIRNAME, &format!("{}.json", scope_digest(scope)))?;
    let record: Option<LayoutRecord> = read_record(sink, buffer, store, &path, "layout")?;
    Ok(match record {
        // A record of another scope under the same digest is not this scope's.
        Some(record) if record.scope == scope => DeliveryLayout { items: record.items },
        _ => DeliveryLayout::default(),
    })
}

/// DMS-FR-XAZV, DMS-FR-TJSJ: replace the layout of `scope` with one atomic
/// whole-file write. An empty layout removes the scope's record. A scope that
/// is not of the form of DMS-FR-DMWD, a position or a size that is not
/// finite, and a size that is not positive are refused with `invalid_layout`,
/// and nothing is written.
pub fn save_layout(
    store: &RootFs,
    scope: &str,
    layout: &DeliveryLayout,
) -> Result<(), DeliveryError> {
    if parse_scope(scope).is_none() {
        return Err(invalid("The layout scope is not `github:<id>` or `flow:<id>`."));
    }
    if !layout_is_valid(layout) {
        return Err(invalid("A position or a size is not finite, or a size is not positive."));
    }
    let path = record_path(store, LAYOUT_DIRNAME, &format!("{}.json", scope_digest(scope)))?;
    if layout.items.is_empty() {
        return remove_record(store, &path);
    }
    let record = LayoutRecord { scope: scope.to_string(), items: layout.items.clone() };
    write_record(store, &path, &record)
}
