//! FSA-FR-ZUCF: removal of a directory tree the application itself owns.
//!
//! Every other removal in this module refuses a symbolic link (FSA-FR-17),
//! because a link is the one way out of an allowlisted root and the refusal is
//! the cheap and total reading of FSA-FR-10. That rule cannot govern the
//! application's own directories. A graduation run's review checkout holds a
//! dependency store built of symbolic links, so a removal that refuses over one
//! leaves the application unable to reclaim a directory it made itself.
//!
//! This operation answers that alone. It unlinks a link rather than following
//! it, so it reaches no target and stays inside the tree it was given, and it
//! accepts a path under an application-owned root and nowhere else.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::access::{FsAccess, FsAccessBuilder};
use super::error::FsError;
use super::{FsAccessBuildError, FsResult};

impl FsAccessBuilder {
    /// FSA-FR-ZUCF: add a root the **application itself owns**.
    ///
    /// An owned root is allowlisted exactly as `allow_root` makes one, and it is
    /// also the only place `delete_owned_tree` removes a tree from. In
    /// production the owned roots are `app_data_dir()` and `short_data_dir()`
    /// and nothing else (FSA-FR-21), so a directory the application made is
    /// reclaimable and a directory the user owns is not.
    pub fn owned_root(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        self.owned.push(path.clone());
        self.roots.push(path);
        self
    }
}

/// FSA-FR-ZUCF: the owned roots, canonicalised as the allowlist is.
///
/// Both spellings are carried for the reason the allowlist carries them. Every
/// owned path is in the builder's `roots` as well, so the allowlist's own loop
/// has already established that each one exists and is a directory.
pub(super) fn canonicalize_owned(
    owned: &[PathBuf],
) -> Result<Vec<PathBuf>, FsAccessBuildError> {
    let mut out = Vec::with_capacity(owned.len() + 1);
    for raw in owned {
        let canonical = super::access::canonicalize_root(raw)?;
        if raw.is_absolute() && *raw != canonical {
            out.push(raw.clone());
        }
        out.push(canonical);
    }
    Ok(out)
}

impl FsAccess {
    /// FSA-FR-ZUCF: the roots the application owns, canonicalised. Empty where
    /// it owns none.
    pub fn owned_roots(&self) -> &[PathBuf] {
        &self.owned_roots
    }

    /// FSA-FR-ZUCF: remove a directory tree this application owns.
    ///
    /// The path must lie **under** an owned root — in production
    /// `app_data_dir()` or `short_data_dir()` and nothing else (FSA-FR-21). An
    /// owned root itself is refused: this reclaims what the application put in
    /// one, never the root the instance was built on.
    ///
    /// A symbolic link it meets is unlinked rather than followed or refused, so
    /// no target outside the tree is read, written or removed.
    pub fn delete_owned_tree(&self, path: impl AsRef<Path>) -> FsResult<()> {
        self.deny_write("delete_owned_tree")?;
        let target = self.tree_target(path.as_ref(), &self.owned_roots)?;
        remove_unlinking(&target)
    }

    /// The root of `roots` holding `path`, or the typed FSA-FR-10 refusal.
    ///
    /// Every containment decision here is lexical and happens **before** any
    /// filesystem access, exactly as `contain` decides it for the allowlist. A
    /// path this instance does not hold a root for is therefore refused without
    /// a single read — including on an instance that holds none at all, which
    /// is what keeps these operations off an agent session (FSA-FR-29).
    pub(super) fn tree_target(&self, path: &Path, roots: &[PathBuf]) -> FsResult<PathBuf> {
        let contained = self.contain(path)?;
        let escapes = || FsError::EscapesAllowedRoots {
            path: contained.clone(),
        };
        let root = roots
            .iter()
            .find(|root| contained.starts_with(root))
            .ok_or_else(escapes)?;
        // Under a root, never the root. Removing the root would take the
        // instance's own allowlisted directory with it.
        if contained == *root {
            return Err(escapes());
        }
        // Only now, for a path this instance may remove, is the disk read.
        // FSA-FR-17 still governs the way **to** the target: only the tree below
        // it is exempt, so a link among the components between the root and the
        // target is refused as it is everywhere else. The leaf alone may be a
        // link, because unlinking one is what these operations do.
        self.refuse_links(&contained, true)?;
        Ok(contained)
    }
}

/// Remove the tree at `target`, unlinking every symbolic link and following
/// none (FSA-FR-ZUCF, FSA-FR-OWVT).
pub(super) fn remove_unlinking(target: &Path) -> FsResult<()> {
    let meta = match fs::symlink_metadata(target) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(FsError::NotFound {
                path: target.to_path_buf(),
            })
        }
        Err(e) => return Err(FsError::Io(e)),
    };
    // A link at the leaf is unlinked, not followed. `remove_dir_all` would
    // refuse it, and `remove_file` is what removes the link itself.
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return fs::remove_file(target).map_err(FsError::Io);
    }
    // No `check_tree` here, and that is the whole point of the operations:
    // `remove_dir_all` unlinks a link it meets rather than descending through
    // it, which is exactly what FSA-FR-ZUCF and FSA-FR-OWVT ask for.
    fs::remove_dir_all(target).map_err(FsError::Io)
}

#[cfg(test)]
mod tests;
