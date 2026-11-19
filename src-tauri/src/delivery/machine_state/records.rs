//! The record files of the delivery machine state: where each one is, and how
//! one is read, written, and removed (`DMS-delivery-machine-state.md` contract
//! surface, DMS-FR-ETLQ, RMS-FR-PFOB).

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::fs::{self, RootFs};
use crate::logging::{log_warn, Domain, LogBuffer, LogSink};

use super::super::{DeliveryError, DELIVERY_DIRNAME};

/// The four folders of `delivery/` this module writes.
pub(super) const LAYOUT_DIRNAME: &str = "layout";
pub(super) const VIEW_DIRNAME: &str = "view";
pub(super) const OVERLAY_DIRNAME: &str = "overlay";
pub(super) const PUBLISH_DIRNAME: &str = "publish";

/// DMS contract surface: the first 32 lowercase hexadecimal characters of the
/// SHA-256 of a scope or of an artifact id. It names a record file, so a scope
/// or an artifact id never appears in a path.
pub fn scope_digest(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..32].to_string()
}

/// The path of the record `file` in the folder `kind` of `delivery/`, inside
/// the store. A name that would escape the store is `store_unavailable`
/// (RMS-FR-DGWY).
pub(in crate::delivery) fn record_path(
    store: &RootFs,
    kind: &str,
    file: &str,
) -> Result<PathBuf, DeliveryError> {
    let rel = PathBuf::from(DELIVERY_DIRNAME).join(kind).join(file);
    fs::resolve_under(store.path(), rel).map_err(|_| DeliveryError::store_unavailable())
}

/// The record at `path`. An absent record is `None`. A record that cannot be
/// parsed reads as `None` too, and the read logs one record with the record
/// kind and no content (DMS-FR-ETLQ). A record that the store cannot read is
/// `store_unavailable`.
pub(in crate::delivery) fn read_record<T, S>(
    sink: &S,
    buffer: &'static LogBuffer,
    store: &RootFs,
    path: &std::path::Path,
    record_kind: &str,
) -> Result<Option<T>, DeliveryError>
where
    T: DeserializeOwned,
    S: LogSink + Clone + Send + 'static,
{
    let text = match store.read_text(path) {
        Ok(text) => text,
        Err(fs::FsError::NotFound { .. }) => return Ok(None),
        Err(_) => {
            log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "delivery record could not be read",
                crate::log_fields! { "record" => record_kind.to_string() },
            );
            return Err(DeliveryError::store_unavailable());
        }
    };
    match serde_json::from_str::<T>(&text) {
        Ok(value) => Ok(Some(value)),
        Err(_) => {
            log_warn(
                sink,
                buffer,
                &[Domain::Backend],
                "delivery record is not readable and reads as empty",
                crate::log_fields! { "record" => record_kind.to_string(), "bytes" => text.len() },
            );
            Ok(None)
        }
    }
}

/// Write `value` to `path` as one atomic whole-file write (RMS-FR-PFOB). The
/// write creates the folders above the record.
pub(in crate::delivery) fn write_record<T: Serialize>(
    store: &RootFs,
    path: &std::path::Path,
    value: &T,
) -> Result<(), DeliveryError> {
    let text =
        serde_json::to_string_pretty(value).map_err(|_| DeliveryError::store_unavailable())?;
    store.write_text_atomic(path, &text).map_err(|_| DeliveryError::store_unavailable())
}

/// Remove the record at `path`. An absent record is not an error.
pub(super) fn remove_record(store: &RootFs, path: &std::path::Path) -> Result<(), DeliveryError> {
    let rel = path.strip_prefix(store.path()).map_err(|_| DeliveryError::store_unavailable())?;
    match store.delete_under(store.path(), rel, false) {
        Ok(()) | Err(fs::FsError::NotFound { .. }) => Ok(()),
        Err(_) => Err(DeliveryError::store_unavailable()),
    }
}
