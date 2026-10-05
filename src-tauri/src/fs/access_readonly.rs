//! Read-only grants and the documents profile (FSA-FR-DMKC, FSA-FR-UKAE,
//! FSA-FR-KAQV, FSA-FR-GHBN).
//!
//! The documents instance reaches the files and folders a user selected for the
//! Documents collection and nothing else. It is built from read-only grants,
//! never from `allow_root`, and it refuses every operation that writes before
//! any filesystem access happens. A read-only grant and a read-write root never
//! share one instance, so no instance can both write and reach a selected path.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::access::{FsAccess, FsAccessBuilder};
use super::error::{FsAccessBuildError, GrantRefusal};
use super::{FsError, FsResult};

impl FsAccessBuilder {
    /// FSA-FR-DMKC: grant read-only reach to exactly one file. Repeatable.
    ///
    /// The ancestors of `path` are canonicalised at build time and its final
    /// component must be a regular file, not a symbolic link (FSA-FR-KAQV).
    pub fn read_only_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.read_only_files.push(path.into());
        self
    }

    /// FSA-FR-DMKC: grant read-only reach to one folder and its subtree.
    /// Repeatable, on the terms of [`FsAccessBuilder::read_only_file`].
    pub fn read_only_folder(mut self, path: impl Into<PathBuf>) -> Self {
        self.read_only_folders.push(path.into());
        self
    }
}

/// Which kind of path a grant names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GrantKind {
    File,
    Folder,
}

/// FSA-FR-KAQV: establish the identity of one granted path.
///
/// The ancestors are canonicalised, which is the one symbolic-link resolution an
/// instance performs whatever its policy. The final component is **not**
/// resolved: a link there is refused, and so is a missing path or a path of the
/// wrong kind. The second element is the spelling the caller gave, normalised
/// lexically, when it differs from the canonical one; both are allowlisted for
/// the reason `allow_root` allowlists both spellings of a root.
fn establish(
    raw: &Path,
    kind: GrantKind,
) -> Result<(PathBuf, Option<PathBuf>), FsAccessBuildError> {
    let refuse = |reason: GrantRefusal| FsAccessBuildError::GrantRefused {
        path: raw.to_path_buf(),
        reason,
    };
    if !raw.is_absolute() {
        return Err(refuse(GrantRefusal::Unreadable));
    }
    let lexical = FsAccess::normalize(raw);
    // A filesystem root has no leaf, so it cannot be granted.
    let (Some(parent), Some(leaf)) = (lexical.parent(), lexical.file_name()) else {
        return Err(refuse(GrantRefusal::Unreadable));
    };
    let canonical_parent = fs::canonicalize(parent).map_err(|e| {
        refuse(if e.kind() == io::ErrorKind::NotFound {
            GrantRefusal::Missing
        } else {
            GrantRefusal::Unreadable
        })
    })?;
    let canonical = canonical_parent.join(leaf);
    let meta = fs::symlink_metadata(&canonical).map_err(|e| {
        refuse(if e.kind() == io::ErrorKind::NotFound {
            GrantRefusal::Missing
        } else {
            GrantRefusal::Unreadable
        })
    })?;
    if meta.file_type().is_symlink() {
        return Err(refuse(GrantRefusal::Link));
    }
    let right_kind = match kind {
        GrantKind::File => meta.is_file(),
        GrantKind::Folder => meta.is_dir(),
    };
    if !right_kind {
        return Err(refuse(GrantRefusal::Unreadable));
    }
    let alias = (lexical != canonical).then_some(lexical);
    Ok((canonical, alias))
}

/// FSA-FR-DMKC / FSA-FR-UKAE: build the read-only instance.
///
/// Refused outright when the builder also holds a read-write root or a session
/// temp directory, so a read-only instance can never be one that writes.
pub(super) fn build_read_only(builder: FsAccessBuilder) -> Result<FsAccess, FsAccessBuildError> {
    if !builder.roots.is_empty() || !builder.owned.is_empty() || !builder.author.is_empty() || builder.session_temp {
        return Err(FsAccessBuildError::MixedProfile);
    }
    let mut roots = Vec::new();
    let mut file_grants = Vec::new();
    for raw in &builder.read_only_folders {
        let (canonical, alias) = establish(raw, GrantKind::Folder)?;
        roots.extend(alias);
        roots.push(canonical);
    }
    for raw in &builder.read_only_files {
        let (canonical, alias) = establish(raw, GrantKind::File)?;
        file_grants.extend(alias);
        file_grants.push(canonical);
    }
    Ok(FsAccess {
        roots,
        owned_roots: Vec::new(),
        author_roots: Vec::new(),
        follow_symlinks: builder.follow_symlinks,
        session_temp: None,
        read_only: true,
        file_grants,
    })
}

impl FsAccess {
    /// FSA-FR-UKAE: whether this instance was built from read-only grants.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// FSA-FR-GHBN: the exact files a read-only instance may reach. Empty on a
    /// read-write instance.
    pub fn file_grants(&self) -> &[PathBuf] {
        &self.file_grants
    }

    /// FSA-FR-UKAE: refuse an operation that writes, before any filesystem
    /// access. A no-op on a read-write instance.
    pub(super) fn deny_write(&self, operation: &'static str) -> FsResult<()> {
        if self.read_only {
            return Err(FsError::ReadOnly { operation });
        }
        Ok(())
    }
}
