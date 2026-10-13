//! The export cache of the sandbox image fallback (EAC-FR-ASNS, EAC-FR-IGBY).
//!
//! Where a registry cannot serve an image, the verified Docker backend exports
//! it. The cache holds one file per exported image, named by the image ID, so
//! a changed image — which has a new ID — never reuses an outdated export.
//! After an export succeeds, every other export of the same reference is
//! deleted. Every file operation goes through `FsAccess`.

use std::path::PathBuf;

use super::ImageExporter;
use crate::fs::{EntryKind, FsAccess, FsError};

/// The longest readable part of a reference a file name carries.
const SLUG_LIMIT: usize = 64;

/// The reference-derived part of every name, so all exports of one reference
/// share one prefix. A readable slug and a digest of the whole reference, so
/// two references that slug alike never share a prefix.
pub fn reference_slug(reference: &str) -> String {
    let mut slug = String::new();
    for c in reference.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.len() >= SLUG_LIMIT {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    let digest = crate::fs::sha256_bytes(reference.as_bytes());
    if slug.is_empty() {
        digest[..12].to_string()
    } else {
        format!("{slug}-{}", &digest[..12])
    }
}

/// The hex of an image ID, without its `sha256:` prefix. Anything that is not
/// a lowercase hex digit is dropped, so an ID never names a path.
pub fn id_hex(id: &str) -> String {
    let hex = id.rsplit(':').next().unwrap_or(id);
    hex.chars().filter(|c| c.is_ascii_hexdigit()).map(|c| c.to_ascii_lowercase()).take(64).collect()
}

/// EAC-FR-IGBY: the file one export of one image is kept in.
pub fn export_file_name(reference: &str, id: &str) -> String {
    format!("{}--{}.tar", reference_slug(reference), id_hex(id))
}

/// EAC-FR-ASNS: the reference a loaded export is held under. Derived from the
/// image ID and never the configured reference, so a pull of the configured
/// reference never finds an export.
pub fn export_reference(reference: &str, id: &str) -> String {
    format!("localhost/synthesis-export/{}:{}", reference_slug(reference), id_hex(id))
}

/// EAC-FR-IGBY: the cache under one root.
pub struct ExportCache<'a> {
    fs: &'a FsAccess,
    root: PathBuf,
}

impl<'a> ExportCache<'a> {
    pub fn new(fs: &'a FsAccess, root: PathBuf) -> ExportCache<'a> {
        ExportCache { fs, root }
    }

    /// The export of `reference` at `id`, made where the cache does not hold
    /// it. Returns the file and whether it was already there.
    pub async fn ensure(
        &self,
        reference: &str,
        id: &str,
        exporter: &dyn ImageExporter,
    ) -> Result<(PathBuf, bool), String> {
        if id_hex(id).is_empty() {
            return Err("the Docker backend reported no image ID".to_string());
        }
        match self.fs.create_dir(&self.root) {
            Ok(()) | Err(FsError::AlreadyExists { .. }) => {}
            Err(error) => return Err(error.to_string()),
        }
        let name = export_file_name(reference, id);
        let path = self.root.join(&name);
        if self.fs.file_info(&path).is_ok_and(|info| info.kind == EntryKind::File) {
            return Ok((path, true));
        }
        // Written aside and renamed into place when whole, so a cut export is
        // never read as an image.
        let partial_name = format!(".{name}.partial");
        let partial = self.root.join(&partial_name);
        if self.fs.file_info(&partial).is_ok() {
            self.fs.delete_path(&partial, false).map_err(|e| e.to_string())?;
        }
        if let Err(error) = exporter.export(reference, &partial, self.fs).await {
            let _ = self.fs.delete_path(&partial, false);
            return Err(error);
        }
        if self.fs.file_info(&partial).map(|info| info.kind != EntryKind::File).unwrap_or(true) {
            return Err("the Docker backend wrote no export".to_string());
        }
        self.fs.rename_path(&partial, &name).map_err(|e| e.to_string())?;
        self.prune(reference, &name);
        Ok((path, false))
    }

    /// EAC-FR-IGBY: delete every other export of `reference`, and every
    /// partial one. A file that cannot be deleted is left for the next prune.
    fn prune(&self, reference: &str, keep: &str) {
        let prefix = format!("{}--", reference_slug(reference));
        let partial_prefix = format!(".{prefix}");
        let Ok(entries) = self.fs.list_dir(&self.root) else {
            return;
        };
        for entry in entries {
            let stale = (entry.name.starts_with(&prefix)
                || entry.name.starts_with(&partial_prefix))
                && entry.name != keep;
            if stale && entry.kind == EntryKind::File {
                let _ = self.fs.delete_path(self.root.join(&entry.name), false);
            }
        }
    }

    /// The root, for a test to read.
    #[cfg(test)]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}
