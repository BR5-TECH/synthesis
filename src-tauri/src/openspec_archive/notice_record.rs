//! The notice record of a change: `changes/<stream-id>/<change>/merge-notice.toml`
//! in the repository machine store (`OFP-openspec-failure-policy.md`
//! OFP-FR-HVHV; `RMS-repository-machine-storage.md`). It moves with the
//! change's store folder at an archive (OAR-FR-MSOM), and a new notice
//! replaces it.

use serde::{Deserialize, Serialize};

use crate::fs::RootFs;

/// RMS: the file name of the notice record.
pub const NOTICE_RECORD_FILENAME: &str = "merge-notice.toml";

/// OFP contract surface, storage: the notice record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeRecord {
    /// OFP-FR-KYXI: the digest of the change folder at the refusal.
    pub digest: String,
    /// The classifying code of the refusal.
    pub cause: String,
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_refusal: Option<String>,
    pub recorded_at: String,
}

/// The path of the record of `change` on `stream_id`, or `None` where the
/// pair does not name a change folder of the store.
fn record_path(store: &RootFs, stream_id: &str, change: &str) -> Option<std::path::PathBuf> {
    if !crate::comments::is_change_owner(stream_id, change) {
        return None;
    }
    crate::fs::resolve_under(
        store.path(),
        format!(
            "{}/{stream_id}/{change}/{NOTICE_RECORD_FILENAME}",
            crate::repository_store::CHANGES_DIRNAME
        ),
    )
    .ok()
}

/// OFP-FR-PSVR: the record of `change`, where one stands and reads.
pub fn read_notice_record(store: &RootFs, stream_id: &str, change: &str) -> Option<NoticeRecord> {
    store.read_toml(record_path(store, stream_id, change)?).ok()
}

/// OFP-FR-HVHV: write the record of `change`, in place of an earlier one.
pub(super) fn write_notice_record(
    store: &RootFs,
    stream_id: &str,
    change: &str,
    record: &NoticeRecord,
) -> Result<(), String> {
    let path = record_path(store, stream_id, change).ok_or("change_not_found")?;
    store.write_toml_atomic(path, record).map_err(|_| "write_failed".to_string())
}
