//! The fingerprint record of a Map discussion (MPR-FR-VGCX, MPR-FR-EHCG).
//!
//! One JSON file for each Map thread in `comments/map/references/` of the
//! comments store. It holds the id and the fingerprint of each plan reference
//! that the input of the last turn that delivered was assembled with. The next
//! turn compares its references with it, so the agent knows which reference
//! changed since that turn.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fs::RootFs;

use super::errors::store_unavailable;
use super::model::MapPlanError;
use super::store::is_valid_proposal_id;

const EXTENSION: &str = "json";

/// MPR-FR-EHCG: one plan reference and the SHA-256 of the bytes of its file.
/// A reference with no text has no fingerprint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceFingerprint {
    pub id: String,
    #[serde(default)]
    pub fingerprint: Option<String>,
}

/// MPR-FR-VGCX: the fingerprint record of one Map discussion.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintRecord {
    pub discussion_id: String,
    #[serde(default)]
    pub references: Vec<ReferenceFingerprint>,
}

/// MPR-FR-EHCG: the fingerprint of `bytes`, as lowercase hex.
pub fn fingerprint_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn path_of(store: &RootFs, discussion_id: &str) -> Result<std::path::PathBuf, MapPlanError> {
    // A discussion id is joined into a path, so it must be a bare token.
    if !is_valid_proposal_id(discussion_id) {
        return Err(store_unavailable());
    }
    let dir = crate::comments::map_fingerprints_dir(store).map_err(|_| store_unavailable())?;
    Ok(dir.join(format!("{discussion_id}.{EXTENSION}")))
}

/// MPR-FR-EHCG: the record of the discussion. No record, a record that does not
/// parse, and a record of another discussion all read as no record, so the
/// marks of the next turn are `first`.
pub fn read_fingerprints(store: &RootFs, discussion_id: &str) -> Option<FingerprintRecord> {
    let text = store.read_text(path_of(store, discussion_id).ok()?).ok()?;
    let record: FingerprintRecord = serde_json::from_str(&text).ok()?;
    (record.discussion_id == discussion_id).then_some(record)
}

/// MPR-FR-VGCX: replace the record of the discussion with one atomic write.
pub fn write_fingerprints(store: &RootFs, record: &FingerprintRecord) -> Result<(), MapPlanError> {
    let path = path_of(store, &record.discussion_id)?;
    let text = serde_json::to_string(record).map_err(|_| store_unavailable())?;
    store.write_text_atomic(path, &text).map_err(|_| store_unavailable())
}
