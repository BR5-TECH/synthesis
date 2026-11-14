//! The publish journal of a Flow file (`DMS-delivery-machine-state.md`
//! DMS-FR-RVJQ): each item id and each edge id of the Flow, mapped to what a
//! publish created for it on the tracker. `DSR-delivery-source.md`
//! (DSR-FR-ULON) writes an entry before its next request, reads the journal
//! to resume a failed publish, and removes it when the publish completes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fs::RootFs;
use crate::logging::{LogBuffer, LogSink};

use super::super::DeliveryError;
use super::records::{
    read_record, record_path, remove_record, scope_digest, write_record, PUBLISH_DIRNAME,
};

/// The kind of tracker item a publish created for a plan item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatedItemKind {
    Issue,
    Milestone,
}

/// The tracker item a publish created for one plan item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedItem {
    pub kind: CreatedItemKind,
    pub number: u64,
    pub url: String,
    /// The GitHub node ID, where the publish knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// `false` for an issue that a partial create made (DSR-FR-ULON): the
    /// issue exists, but it may not be on the Project or have its parent yet.
    /// A resumed publish finishes those steps before the entry counts as done.
    /// An entry with no such field, as an older journal has, is complete.
    #[serde(default = "complete_by_default", skip_serializing_if = "is_complete")]
    pub complete: bool,
}

fn complete_by_default() -> bool {
    true
}

fn is_complete(complete: &bool) -> bool {
    *complete
}

/// The relation a publish created for one `blocks` edge: the issue that is
/// blocked and the issue that blocks it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedRelation {
    pub blocked: u64,
    pub blocking: u64,
}

/// DMS-FR-RVJQ: the journal of one Flow file. The entries are kept by id in a
/// fixed order, so the same journal always writes the same file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishJournal {
    #[serde(default)]
    pub items: BTreeMap<String, CreatedItem>,
    #[serde(default)]
    pub edges: BTreeMap<String, CreatedRelation>,
}

/// The record file. It also holds the artifact id, so a reader can see which
/// Flow a digest names.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalRecord {
    flow_id: String,
    #[serde(flatten)]
    journal: PublishJournal,
}

fn journal_path(store: &RootFs, flow_id: &str) -> Result<std::path::PathBuf, DeliveryError> {
    record_path(store, PUBLISH_DIRNAME, &format!("{}.json", scope_digest(flow_id)))
}

/// DMS contract surface `read_publish_journal(flow_id)`: the journal of the
/// Flow. No journal, and a record that cannot be parsed (DMS-FR-ETLQ), read as
/// an empty journal.
pub fn read_publish_journal<S: LogSink + Clone + Send + 'static>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &RootFs,
    flow_id: &str,
) -> Result<PublishJournal, DeliveryError> {
    let path = journal_path(store, flow_id)?;
    let record: Option<JournalRecord> = read_record(sink, buffer, store, &path, "publish_journal")?;
    Ok(match record {
        Some(record) if record.flow_id == flow_id => record.journal,
        _ => PublishJournal::default(),
    })
}

/// DMS contract surface `write_publish_journal(flow_id, journal)`: replace the
/// journal of the Flow with one atomic whole-file write.
pub fn write_publish_journal(
    store: &RootFs,
    flow_id: &str,
    journal: &PublishJournal,
) -> Result<(), DeliveryError> {
    let path = journal_path(store, flow_id)?;
    let record = JournalRecord { flow_id: flow_id.to_string(), journal: journal.clone() };
    write_record(store, &path, &record)
}

/// DMS-FR-RVJQ: remove the journal of the Flow when its publish completes. No
/// journal is not an error.
pub fn remove_publish_journal(store: &RootFs, flow_id: &str) -> Result<(), DeliveryError> {
    remove_record(store, &journal_path(store, flow_id)?)
}
