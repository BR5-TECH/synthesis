//! The tests of the Library tree commands and file operations
//! (PST-project-storage.md / ASC-artifact-scanning.md / LIB-library.md).
//!
//! The topic files below all run against the same scope: `use super::*;`
//! brings in this module head, which re-exports the module under test.

use super::*;

/// Find the node at project-relative `rel` within a full tree.
fn find_node<'a>(node: &'a scanning::TreeNode, rel: &str) -> Option<&'a scanning::TreeNode> {
    if node.path == rel {
        return Some(node);
    }
    node.children.as_ref().and_then(|cs| cs.iter().find_map(|c| find_node(c, rel)))
}

mod files;
mod folders;
mod ops;
mod typed;
