//! The view of a delivery canvas (`DMS-delivery-machine-state.md`
//! DMS-FR-TXMH to DMS-FR-XUYV): for each layout scope, which board statuses
//! the board hides, and whether it shows the blocked items only.

use serde::{Deserialize, Serialize};

use crate::fs::RootFs;
use crate::logging::{LogBuffer, LogSink};

use super::super::{DeliveryError, ERR_INVALID_VIEW};
use super::layout::parse_scope;
use super::records::{
    read_record, record_path, remove_record, scope_digest, write_record, VIEW_DIRNAME,
};

/// DMS-FR-TXMH: a board status (per `DBR-delivery-board.md` DBR-FR-QZPA).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BoardStatus {
    #[serde(rename = "backlog")]
    Backlog,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "in progress")]
    InProgress,
    #[serde(rename = "in review")]
    InReview,
    #[serde(rename = "done")]
    Done,
}

/// DMS contract surface `DeliveryView`. The empty view hides nothing and
/// shows every item, blocked or not.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryView {
    #[serde(default)]
    pub hidden: Vec<BoardStatus>,
    #[serde(default)]
    pub blocked_only: bool,
}

impl DeliveryView {
    /// Whether this is the empty view of DMS-FR-TXMH.
    pub fn is_empty(&self) -> bool {
        self.hidden.is_empty() && !self.blocked_only
    }

    /// The same view with each status once, in a fixed order, so the same
    /// view always writes the same file.
    fn normalized(&self) -> Self {
        let mut hidden = self.hidden.clone();
        hidden.sort();
        hidden.dedup();
        Self { hidden, blocked_only: self.blocked_only }
    }
}

/// The record file. It also holds the scope, so a reader can see which scope
/// a digest names.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewRecord {
    scope: String,
    #[serde(default)]
    view: DeliveryView,
}

fn invalid(message: &str) -> DeliveryError {
    DeliveryError::new(ERR_INVALID_VIEW, message)
}

/// DMS-FR-TICX: read the view argument of a command. A value that is not of
/// the shape of `DeliveryView`, for example a status that is not a board
/// status, is refused with `invalid_view`.
pub fn parse_view(value: &serde_json::Value) -> Result<DeliveryView, DeliveryError> {
    DeliveryView::deserialize(value)
        .map_err(|_| invalid("The view holds a status that is not a board status."))
}

/// DMS-FR-JBKN: the view of `scope`. A scope with no stored view, a record
/// that cannot be parsed (DMS-FR-ETLQ), a record of another scope, and a text
/// that is no scope each return the empty view.
pub fn load_view<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &RootFs,
    scope: &str,
) -> Result<DeliveryView, DeliveryError> {
    if parse_scope(scope).is_none() {
        return Ok(DeliveryView::default());
    }
    let path = record_path(store, VIEW_DIRNAME, &format!("{}.json", scope_digest(scope)))?;
    let record: Option<ViewRecord> = read_record(sink, buffer, store, &path, "view")?;
    Ok(match record {
        // A record of another scope under the same digest is not this scope's.
        Some(record) if record.scope == scope => record.view.normalized(),
        _ => DeliveryView::default(),
    })
}

/// DMS-FR-TICX, DMS-FR-XUYV: replace the view of `scope` with one atomic
/// whole-file write. The empty view removes the scope's view record. A scope
/// that is not of the form of DMS-FR-DMWD is refused with `invalid_view`, and
/// nothing is written. The layout record of the scope stays as it is.
pub fn save_view(store: &RootFs, scope: &str, view: &DeliveryView) -> Result<(), DeliveryError> {
    if parse_scope(scope).is_none() {
        return Err(invalid("The view scope is not `github:<id>` or `flow:<id>`."));
    }
    let path = record_path(store, VIEW_DIRNAME, &format!("{}.json", scope_digest(scope)))?;
    let view = view.normalized();
    if view.is_empty() {
        return remove_record(store, &path);
    }
    let record = ViewRecord { scope: scope.to_string(), view };
    write_record(store, &path, &record)
}
