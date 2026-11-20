//! The tree reads of the project index (`ASC-artifact-scanning.md`
//! ASC-FR-KVIE, ASC-FR-AGGG, ASC-FR-SKRN, ASC-FR-YNJR, ASC-FR-AVVY,
//! ASC-FR-QKXR). Each answer grows with what it returns, not with the size of
//! the project, except the two searches, which visit the index once.

use serde::{Deserialize, Serialize};

use super::index::{parent_of, ProjectIndex};
use super::repos;
use super::{ArtifactType, NodeKind, TreeNode};

/// ASC-FR-AGGG: one folder of a `list_tree_path` answer and its children.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderChildren {
    pub folder: String,
    pub children: Vec<TreeNode>,
}

/// The lens of `filter_project_tree` (`../ui/LIB-library.md` LIB-FR-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", untagged)]
pub enum Lens {
    Named(LensName),
    Type(ArtifactType),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LensName {
    AllArtifacts,
    AllFiles,
}

/// The kind of node `search_project_paths` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathKind {
    File,
    Folder,
    Any,
}

/// ASC-FR-SKRN: the answer of `filter_project_tree`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterResult {
    pub root: Vec<TreeNode>,
    pub truncated: bool,
}

/// ASC-FR-AVVY: the answer of `search_project_paths`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathSearch {
    pub paths: Vec<String>,
    pub truncated: bool,
}

impl Lens {
    /// LIB-FR-05: whether the lens admits a file of this type.
    fn admits_file(self, t: Option<ArtifactType>) -> bool {
        match self {
            Lens::Named(LensName::AllFiles) => true,
            Lens::Named(LensName::AllArtifacts) => t.is_some(),
            Lens::Type(want) => t == Some(want),
        }
    }

    /// LIB-FR-09: whether the lens admits a folder on its own terms.
    fn admits_folder(self, own: Option<ArtifactType>) -> bool {
        match self {
            Lens::Named(LensName::AllFiles) => true,
            Lens::Named(LensName::AllArtifacts) => own.is_some(),
            Lens::Type(want) => own == Some(want),
        }
    }
}

impl ProjectIndex {
    /// The wire node of `rel`, with no `children` field.
    pub fn node(&self, rel: &str) -> Option<TreeNode> {
        self.tree_node(rel)
    }

    /// ASC-FR-YLGN: each folder the index marks as a repository boundary, with
    /// its kind and how many direct children it holds, in path order. The
    /// project index logs it, so a submodule that does not open in the tree
    /// can be explained from the Logs panel.
    pub fn nested_folders(&self) -> Vec<(String, super::repos::NestedRepo, usize)> {
        let mut out: Vec<(String, super::repos::NestedRepo, usize)> = self
            .nodes
            .iter()
            .filter_map(|(rel, node)| {
                let kind = node.nested?;
                Some((rel.clone(), kind, node.dirs.len() + node.files.len()))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// ASC-FR-KVIE: the direct children of the folder `rel`, in tree order.
    /// `None` when the index holds no folder at `rel`.
    pub fn children(&self, rel: &str) -> Option<Vec<TreeNode>> {
        let node = self.nodes.get(rel)?;
        if node.kind != NodeKind::Folder {
            return None;
        }
        Some(
            node.dirs
                .iter()
                .chain(node.files.iter())
                .filter_map(|name| self.tree_node(&repos::join_rel(rel, name)))
                .collect(),
        )
    }

    /// ASC-FR-AGGG: the children of the root, of each folder above `rel`, and
    /// of `rel` itself when it is a folder. `None` when the index does not
    /// hold `rel`.
    pub fn path(&self, rel: &str) -> Option<Vec<FolderChildren>> {
        if !self.nodes.contains_key(rel) {
            return None;
        }
        let mut folders = Vec::new();
        let mut dir = if self.nodes[rel].kind == NodeKind::Folder { rel } else { parent_of(rel) };
        loop {
            folders.push(dir.to_string());
            if dir.is_empty() {
                break;
            }
            dir = parent_of(dir);
        }
        folders.reverse();
        Some(
            folders
                .into_iter()
                .filter_map(|folder| {
                    let children = self.children(&folder)?;
                    Some(FolderChildren { folder, children })
                })
                .collect(),
        )
    }

    /// ASC-FR-SKRN: the nodes whose name holds `text` and that `lens`
    /// admits, with their ancestor folders, as nested nodes. At most `limit`
    /// matches.
    pub fn filter(&self, text: &str, lens: Lens, limit: usize) -> FilterResult {
        let needle = text.trim().to_lowercase();
        let mut matched = 0usize;
        let mut truncated = false;
        let root = self.filter_children("", &needle, lens, limit, &mut matched, &mut truncated);
        FilterResult { root, truncated }
    }

    fn filter_children(
        &self,
        rel: &str,
        needle: &str,
        lens: Lens,
        limit: usize,
        matched: &mut usize,
        truncated: &mut bool,
    ) -> Vec<TreeNode> {
        let Some(node) = self.nodes.get(rel) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for name in node.dirs.iter().chain(node.files.iter()) {
            if *truncated {
                break;
            }
            let child = repos::join_rel(rel, name);
            let Some(raw) = self.nodes.get(&child) else {
                continue;
            };
            let name_hit = needle.is_empty() || raw.name.to_lowercase().contains(needle);
            match raw.kind {
                NodeKind::File => {
                    if name_hit && lens.admits_file(raw.class.artifact_type) {
                        if *matched >= limit {
                            *truncated = true;
                            break;
                        }
                        *matched += 1;
                        out.extend(self.tree_node(&child));
                    }
                }
                NodeKind::Folder => {
                    let self_hit = name_hit && lens.admits_folder(raw.class.artifact_type);
                    if self_hit {
                        if *matched >= limit {
                            *truncated = true;
                            break;
                        }
                        *matched += 1;
                    }
                    let kids =
                        self.filter_children(&child, needle, lens, limit, matched, truncated);
                    if self_hit || !kids.is_empty() {
                        if let Some(mut folder) = self.tree_node(&child) {
                            folder.children = Some(kids);
                            out.push(folder);
                        }
                    }
                }
            }
        }
        out
    }

    /// ASC-FR-YNJR: every typed file and every folder with a folder-scope
    /// assignment, in tree order, with no `children` field.
    pub fn artifacts(&self) -> Vec<TreeNode> {
        let mut out = Vec::new();
        self.visit(|rel, node| {
            if node.class.artifact_type.is_some() {
                out.extend(self.tree_node(rel));
            }
            true
        });
        out
    }

    /// ASC-FR-AVVY: the paths of `kind` that hold `text`, in tree order.
    pub fn search_paths(&self, text: &str, kind: PathKind, limit: Option<usize>) -> PathSearch {
        let needle = text.trim().to_lowercase();
        let mut paths = Vec::new();
        let mut truncated = false;
        self.visit(|rel, node| {
            let kind_ok = match kind {
                PathKind::Any => true,
                PathKind::File => node.kind == NodeKind::File,
                PathKind::Folder => node.kind == NodeKind::Folder,
            };
            if kind_ok && (needle.is_empty() || rel.to_lowercase().contains(&needle)) {
                if limit.is_some_and(|l| paths.len() >= l) {
                    truncated = true;
                    return false;
                }
                paths.push(rel.to_string());
            }
            true
        });
        PathSearch { paths, truncated }
    }

    /// ASC-FR-QKXR: each path of `paths` that the index holds, in input order.
    pub fn existing(&self, paths: &[String]) -> Vec<String> {
        paths
            .iter()
            .filter(|p| !p.is_empty() && self.nodes.contains_key(p.trim_matches('/')))
            .cloned()
            .collect()
    }
}
