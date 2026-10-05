//! Discovery of the documents behind the sources (DCL-FR-KMHY, DCL-FR-EWPO).
//!
//! Every read goes through the documents instance of `FSA-filesystem-access.md`
//! (DCL-FR-BAQY). Discovery follows no symbolic link and lists none: a link in a
//! folder is skipped, and a source that is itself a link is unavailable. A
//! subfolder it cannot read costs that subfolder alone, with a `WARN` record, and
//! the walk continues.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use crate::fs::{EntryKind, FsAccess, FsError, GrantRefusal};
use crate::logging::LogLevel;

use super::log::DocLog;
use super::model::{
    format_of, normalise_path, Availability, DocumentFormat, SourceKind, SourceReason,
    StoredSource,
};

/// What discovery found out about one source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceReport {
    pub source: StoredSource,
    pub status: Availability,
    pub reason: Option<SourceReason>,
}

/// The result of one walk over every source.
#[derive(Debug, Default)]
pub struct Discovery {
    /// One report per stored source, in stored order.
    pub reports: Vec<SourceReport>,
    /// The supported files an available source includes, by normalised path.
    pub files: BTreeMap<String, DocumentFormat>,
    /// The supported files that only an unavailable `file` source names. Each
    /// keeps its own entry as an unavailable document (DCL-FR-EWPO).
    pub unavailable_files: BTreeMap<String, DocumentFormat>,
}

/// Map the reason a grant was refused onto the reason a source reports.
pub fn reason_of(refusal: GrantRefusal) -> SourceReason {
    match refusal {
        GrantRefusal::Missing => SourceReason::Missing,
        GrantRefusal::Link => SourceReason::Link,
        GrantRefusal::Unreadable => SourceReason::Unreadable,
    }
}

/// Map a filesystem error onto the reason a source reports.
fn reason_of_error(error: &FsError) -> SourceReason {
    match error {
        FsError::NotFound { .. } => SourceReason::Missing,
        FsError::SymlinkRefused { .. } => SourceReason::Link,
        _ => SourceReason::Unreadable,
    }
}

/// Whether one source can be reached right now.
///
/// A grant the instance refused at build time is unavailable for the reason it
/// was refused. A grant that stood at build time is checked again, because the
/// path may have moved since: the instance is rebuilt per refresh, and the file
/// can still change between the rebuild and this walk.
fn probe(
    access: Option<&FsAccess>,
    source: &StoredSource,
    refused: &HashMap<String, GrantRefusal>,
) -> Result<(), SourceReason> {
    let key = normalise_path(&source.path);
    if let Some(refusal) = refused.get(&key) {
        return Err(reason_of(*refusal));
    }
    let Some(access) = access else {
        return Err(SourceReason::Unreadable);
    };
    match access.file_info(PathBuf::from(&key)) {
        Ok(info) => match (info.kind, source.kind) {
            (EntryKind::Symlink, _) => Err(SourceReason::Link),
            (EntryKind::File, SourceKind::File) | (EntryKind::Dir, SourceKind::Folder) => Ok(()),
            _ => Err(SourceReason::Unreadable),
        },
        Err(e) => Err(reason_of_error(&e)),
    }
}

/// DCL-FR-KMHY: walk one folder recursively, collecting the supported files.
///
/// The folder's own failure is returned to the caller, which makes the source
/// unavailable; a failure below it is logged and skipped.
fn walk(
    access: &FsAccess,
    dir: &str,
    files: &mut BTreeMap<String, DocumentFormat>,
    log: &dyn DocLog,
    depth: usize,
) -> Result<(), FsError> {
    let (entries, omitted) = access.list_dir_reporting(PathBuf::from(dir))?;
    if omitted > 0 {
        // DCL-FR-KMHY: an entry that vanished during the listing costs that
        // entry alone.
        log.log(
            LogLevel::Warn,
            "documents entry skipped because it vanished during the listing",
            crate::log_fields! { "omitted" => omitted, "depth" => depth },
        );
    }
    for entry in entries {
        let child = format!("{}/{}", dir.trim_end_matches('/'), entry.name);
        match entry.kind {
            // No link is followed and none is listed.
            EntryKind::Symlink => {}
            // DCL-FR-KMHY, DCL-FR-RXMB: a FIFO, socket, or device is not a
            // document, whatever its name. It is skipped and never read.
            EntryKind::Other => {
                if format_of(&entry.name).is_some() {
                    log.log(
                        LogLevel::Warn,
                        "documents entry skipped because it is not a regular file",
                        crate::log_fields! { "depth" => depth },
                    );
                }
            }
            EntryKind::File => {
                if let Some(format) = format_of(&entry.name) {
                    files.insert(child, format);
                }
            }
            EntryKind::Dir => {
                if walk(access, &child, files, log, depth + 1).is_err() {
                    log.log(
                        LogLevel::Warn,
                        "documents folder skipped because it cannot be read",
                        crate::log_fields! { "source_kind" => "folder", "depth" => depth + 1 },
                    );
                }
            }
        }
    }
    Ok(())
}

/// DCL-FR-KMHY / DCL-FR-EWPO: report every source and collect the documents the
/// available ones include.
pub fn discover(
    access: Option<&FsAccess>,
    refused: &HashMap<String, GrantRefusal>,
    sources: &[StoredSource],
    log: &dyn DocLog,
) -> Discovery {
    let mut found = Discovery::default();
    for source in sources {
        let key = normalise_path(&source.path);
        let mut outcome = probe(access, source, refused);
        if outcome.is_ok() {
            match source.kind {
                SourceKind::File => {
                    if let Some(format) = format_of(super::model::file_name_of(&key)) {
                        found.files.insert(key.clone(), format);
                    }
                }
                SourceKind::Folder => {
                    if let Some(access) = access {
                        if let Err(e) = walk(access, &key, &mut found.files, log, 0) {
                            outcome = Err(reason_of_error(&e));
                        }
                    }
                }
            }
        } else if source.kind == SourceKind::File {
            if let Some(format) = format_of(super::model::file_name_of(&key)) {
                found.unavailable_files.insert(key.clone(), format);
            }
        }
        found.reports.push(match outcome {
            Ok(()) => SourceReport {
                source: source.clone(),
                status: Availability::Available,
                reason: None,
            },
            Err(reason) => SourceReport {
                source: source.clone(),
                status: Availability::Unavailable,
                reason: Some(reason),
            },
        });
    }
    // A file an available source includes does not also appear as unavailable.
    let present: Vec<String> = found.files.keys().cloned().collect();
    for path in present {
        found.unavailable_files.remove(&path);
    }
    found
}
