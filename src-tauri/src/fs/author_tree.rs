//! FSA-FR-OWVT: removal of a directory tree the author owns, one named tree at
//! a time.
//!
//! A linked worktree is the author's own directory, not one the application
//! made, so `delete_owned_tree` does not reach it. It still holds a dependency
//! store built of symbolic links, which FSA-FR-17 makes `delete_path` refuse.
//! This operation answers that without widening anything else: the caller names
//! the one directory the removal may happen under, a link is unlinked and never
//! followed, and no instance the application holds for a session names one.

use std::path::{Path, PathBuf};

use super::access::{FsAccess, FsAccessBuilder};
use super::owned_tree::remove_unlinking;
use super::FsResult;

impl FsAccessBuilder {
    /// FSA-FR-OWVT: add a root an author's tree may be removed from.
    ///
    /// The root is allowlisted exactly as `allow_root` makes one. It is also the
    /// only place `delete_author_tree` removes from, and the root itself is
    /// never removed. The caller names the **parent** of the one directory it
    /// means to remove, so the reach is that directory and nothing beside it.
    pub fn author_root(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        self.author.push(path.clone());
        self.roots.push(path);
        self
    }
}

impl FsAccess {
    /// FSA-FR-OWVT: remove a directory tree under an author root.
    ///
    /// A symbolic link it meets is unlinked rather than followed or refused, so
    /// no target outside the tree is read, written or removed. A path under no
    /// author root is the typed "escapes allowed roots" error, and so is the
    /// root itself. A link among the components on the way to the tree is
    /// refused (FSA-FR-17).
    pub fn delete_author_tree(&self, path: impl AsRef<Path>) -> FsResult<()> {
        self.deny_write("delete_author_tree")?;
        let target = self.tree_target(path.as_ref(), &self.author_roots)?;
        remove_unlinking(&target)
    }
}

#[cfg(test)]
mod tests;
