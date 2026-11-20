//! The project index (`ASC-artifact-scanning.md` ASC-FR-HPUY, ASC-FR-UWMI,
//! ASC-FR-OGRB, ASC-FR-YSWU, ASC-FR-JYXO, ASC-FR-YLGN).
//!
//! The tree of the whole project with its classification, held in memory. It
//! is built once from a parallel walk, and a watcher burst then changes it for
//! the paths of the burst alone. Every tree read answers from it for one folder
//! at a time; see `index_query.rs`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use super::ignored::IgnoreRules;
use super::repos::{self, NestedRepo};
use super::walk::{self, FileClass, WalkOutput, Walked};
use super::{
    is_atomic_tmp_name, is_pruned_dir, load_assignments, ArtifactType, Candidate,
    LibraryAssignments, NodeKind, Scope, TreeNode, TypeSource, LIBRARY_TOML_REL,
};
use crate::fs::{EntryKind, RootFs};

/// ASC-FR-UWMI: a burst with more paths than this builds the index again, as a
/// branch checkout does. A walk is cheaper than as many single updates.
pub(crate) const REBUILD_THRESHOLD: usize = 2000;

/// One node of the index.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    pub name: String,
    pub kind: NodeKind,
    pub class: FileClass,
    /// Folders only: the names of the child folders and files. A `BTreeSet`
    /// gives the tree order of ASC-FR-KVIE.
    pub dirs: BTreeSet<String>,
    pub files: BTreeSet<String>,
    /// Folders only: the types present in the subtree, as a bit set over
    /// [`ArtifactType`] (ASC-FR-OGRB).
    pub subtree: u16,
    pub nested: Option<NestedRepo>,
}

impl Node {
    fn folder(name: &str) -> Node {
        Node {
            name: name.to_string(),
            kind: NodeKind::Folder,
            class: FileClass::default(),
            dirs: BTreeSet::new(),
            files: BTreeSet::new(),
            subtree: 0,
            nested: None,
        }
    }

    fn file(name: &str, class: FileClass) -> Node {
        Node { kind: NodeKind::File, class, ..Node::folder(name) }
    }
}

/// ASC-FR-HPUY: the project index of one content root.
#[derive(Clone, Debug)]
pub struct ProjectIndex {
    pub(crate) nodes: HashMap<String, Node>,
    pub(crate) assignments: Arc<LibraryAssignments>,
    /// ASC-FR-SFCR: the nested repository roots.
    pub(crate) repo_roots: BTreeSet<String>,
    /// ASC-FR-YLGN: the paths that a `.gitmodules` lists.
    pub(crate) listed: BTreeSet<String>,
    /// ASC-FR-17: the file nodes in tree order, built on first use.
    pub(crate) candidates: Arc<Vec<Candidate>>,
}

/// The bit of one type in [`Node::subtree`].
pub(crate) fn type_bit(t: ArtifactType) -> u16 {
    1 << (t as u16)
}

/// The types of a bit set, in the order of ASC-FR-02.
pub(crate) fn types_of(bits: u16) -> Vec<ArtifactType> {
    super::store::EVERY_TYPE.iter().copied().filter(|t| bits & type_bit(*t) != 0).collect()
}

/// The parent folder of a project-relative path; `""` for a top-level path.
pub(crate) fn parent_of(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn name_of(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

impl ProjectIndex {
    /// ASC-FR-HPUY, ASC-FR-TPJO: build the index from a parallel walk of the
    /// whole project.
    pub fn build(root: &RootFs) -> ProjectIndex {
        let assignments = Arc::new(load_assignments(root));
        let output = walk::walk_project(root, &assignments);
        ProjectIndex::from_walk(output, assignments)
    }

    /// Build the index from a walk's output (pure).
    pub(crate) fn from_walk(output: WalkOutput, assignments: Arc<LibraryAssignments>) -> Self {
        let mut index = ProjectIndex {
            nodes: HashMap::with_capacity(output.entries.len() + 1),
            assignments,
            repo_roots: output.repo_roots,
            listed: output.listed,
            candidates: Arc::new(Vec::new()),
        };
        index.nodes.insert(String::new(), Node::folder(""));
        let mut entries = output.entries;
        // Folders before their contents, so each parent exists when a child
        // is inserted. A shorter path is never below a longer one.
        entries.sort_by(|a, b| a.rel.len().cmp(&b.rel.len()).then_with(|| a.rel.cmp(&b.rel)));
        for entry in entries {
            index.insert(entry);
        }
        for (rel, kind) in output.nested {
            if let Some(node) = index.nodes.get_mut(&rel) {
                node.nested = Some(kind);
            }
        }
        index.apply_folder_assignments();
        index.roll_up_all();
        index.refresh_candidates();
        index
    }

    /// Insert one walked entry, and any missing parent folder above it.
    fn insert(&mut self, entry: Walked) {
        self.ensure_folder(parent_of(&entry.rel));
        let name = name_of(&entry.rel).to_string();
        let parent = self.nodes.get_mut(parent_of(&entry.rel)).expect("the parent exists");
        if entry.is_dir {
            parent.files.remove(&name);
            parent.dirs.insert(name.clone());
            self.nodes.entry(entry.rel).or_insert_with(|| Node::folder(&name));
        } else {
            parent.dirs.remove(&name);
            parent.files.insert(name.clone());
            self.nodes.insert(entry.rel, Node::file(&name, entry.class));
        }
    }

    /// Make `rel` and every folder above it a folder node.
    fn ensure_folder(&mut self, rel: &str) {
        if self.nodes.contains_key(rel) {
            return;
        }
        let parent = parent_of(rel).to_string();
        self.ensure_folder(&parent);
        let name = name_of(rel).to_string();
        if let Some(p) = self.nodes.get_mut(&parent) {
            p.dirs.insert(name.clone());
        }
        self.nodes.insert(rel.to_string(), Node::folder(&name));
    }

    /// ASC-FR-18: a folder's own type is its folder-scope assignment.
    fn apply_folder_assignments(&mut self) {
        for (path, assignment) in &self.assignments.assignments {
            if assignment.scope != Scope::Folder {
                continue;
            }
            if let Some(node) = self.nodes.get_mut(path) {
                if node.kind == NodeKind::Folder {
                    node.class.artifact_type = Some(assignment.artifact_type);
                    node.class.type_source = Some(TypeSource::Assigned);
                }
            }
        }
    }

    /// ASC-FR-08, ASC-FR-OGRB: compute the subtree types of every folder.
    fn roll_up_all(&mut self) {
        let mut folders: Vec<String> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.kind == NodeKind::Folder)
            .map(|(rel, _)| rel.clone())
            .collect();
        // Deepest first, so a folder's child folders are already done.
        folders.sort_by_key(|rel| std::cmp::Reverse(depth(rel)));
        for rel in folders {
            self.roll_up(&rel);
        }
    }

    /// Compute the subtree types of the folder `rel` from its children.
    fn roll_up(&mut self, rel: &str) {
        let Some(node) = self.nodes.get(rel) else {
            return;
        };
        let mut bits = node.class.artifact_type.map(type_bit).unwrap_or(0);
        for name in &node.dirs {
            if let Some(child) = self.nodes.get(&repos::join_rel(rel, name)) {
                bits |= child.subtree;
            }
        }
        for name in &node.files {
            if let Some(t) =
                self.nodes.get(&repos::join_rel(rel, name)).and_then(|c| c.class.artifact_type)
            {
                bits |= type_bit(t);
            }
        }
        if let Some(node) = self.nodes.get_mut(rel) {
            node.subtree = bits;
        }
    }

    /// ASC-FR-17: the file nodes in tree order.
    fn refresh_candidates(&mut self) {
        let mut out = Vec::new();
        self.visit(|rel, node| {
            if node.kind == NodeKind::File {
                out.push(Candidate {
                    id: rel.to_string(),
                    name: node.name.clone(),
                    path: rel.to_string(),
                    ordinal: out.len() as u64,
                    artifact_type: node.class.artifact_type,
                });
            }
            true
        });
        self.candidates = Arc::new(out);
    }

    /// Visit every node below the root in tree order: each folder before its
    /// contents, child folders before files, each group by name. `f` returns
    /// false to stop the walk.
    pub(crate) fn visit<F: FnMut(&str, &Node) -> bool>(&self, mut f: F) {
        fn go<F: FnMut(&str, &Node) -> bool>(index: &ProjectIndex, rel: &str, f: &mut F) -> bool {
            let Some(node) = index.nodes.get(rel) else {
                return true;
            };
            for name in &node.dirs {
                let child = repos::join_rel(rel, name);
                if let Some(c) = index.nodes.get(&child) {
                    if !f(&child, c) || !go(index, &child, f) {
                        return false;
                    }
                }
            }
            for name in &node.files {
                let child = repos::join_rel(rel, name);
                if let Some(c) = index.nodes.get(&child) {
                    if !f(&child, c) {
                        return false;
                    }
                }
            }
            true
        }
        go(self, "", &mut f);
    }

    /// The wire node of `rel`, with no `children` field.
    pub(crate) fn tree_node(&self, rel: &str) -> Option<TreeNode> {
        let node = self.nodes.get(rel)?;
        Some(match node.kind {
            NodeKind::File => TreeNode {
                id: rel.to_string(),
                name: node.name.clone(),
                path: rel.to_string(),
                node_kind: NodeKind::File,
                artifact_type: node.class.artifact_type,
                type_source: node.class.type_source,
                display_name: node.class.display_name.clone(),
                has_artifacts: None,
                subtree_types: None,
                has_children: None,
                nested_repo: None,
                children: None,
            },
            NodeKind::Folder => TreeNode {
                id: rel.to_string(),
                name: node.name.clone(),
                path: rel.to_string(),
                node_kind: NodeKind::Folder,
                artifact_type: node.class.artifact_type,
                type_source: node.class.type_source,
                display_name: None,
                has_artifacts: Some(node.subtree != 0),
                subtree_types: Some(types_of(node.subtree)),
                has_children: Some(!node.dirs.is_empty() || !node.files.is_empty()),
                nested_repo: node.nested,
                children: None,
            },
        })
    }

    /// The whole tree as nested nodes, the root first.
    pub fn full_tree(&self, root_name: &str) -> TreeNode {
        fn go(index: &ProjectIndex, rel: &str) -> TreeNode {
            let mut node = index.tree_node(rel).expect("a visited node exists");
            if node.node_kind == NodeKind::Folder {
                let raw = &index.nodes[rel];
                let mut children = Vec::with_capacity(raw.dirs.len() + raw.files.len());
                for name in raw.dirs.iter().chain(raw.files.iter()) {
                    children.push(go(index, &repos::join_rel(rel, name)));
                }
                node.children = Some(children);
            }
            node
        }
        let mut root = go(self, "");
        root.name = root_name.to_string();
        root
    }

    /// ASC-FR-17: the candidate list of this index.
    pub fn candidates(&self) -> Arc<Vec<Candidate>> {
        Arc::clone(&self.candidates)
    }

    /// The answer of `list_tree_children` for every folder, for a diff.
    fn all_answers(&self) -> BTreeMap<String, Vec<TreeNode>> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.kind == NodeKind::Folder)
            .filter_map(|(rel, _)| Some((rel.clone(), self.children(rel)?)))
            .collect()
    }

    /// ASC-FR-JYXO: the folders whose answer differs between `before` and
    /// `self`, in path order.
    pub(crate) fn changed_since(&self, before: &BTreeMap<String, Vec<TreeNode>>) -> Vec<String> {
        let after = self.all_answers();
        let mut out: BTreeSet<String> = BTreeSet::new();
        for (rel, answer) in &after {
            if before.get(rel) != Some(answer) {
                out.insert(rel.clone());
            }
        }
        for rel in before.keys() {
            if !after.contains_key(rel) {
                if let Some(parent) = (!rel.is_empty()).then(|| parent_of(rel)) {
                    if after.contains_key(parent) {
                        out.insert(parent.to_string());
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    /// ASC-FR-HPUY, ASC-FR-JYXO: build the index again and return the folders
    /// whose answer changed.
    pub fn rebuild(&mut self, root: &RootFs) -> Vec<String> {
        let before = self.all_answers();
        *self = ProjectIndex::build(root);
        self.changed_since(&before)
    }

    /// ASC-FR-UWMI, ASC-FR-JYXO: update the index for the paths of one
    /// watcher burst, and return the folders whose `list_tree_children` answer
    /// changed. `attribution` is true when `.synthesis/library.toml` changed.
    ///
    /// `boundaries` names each folder whose `.git` entry the burst touched.
    /// A burst that the index cannot apply on its own terms — a change of an
    /// ignore file, of `.gitmodules`, of a repository boundary, or too many
    /// paths — builds the index again.
    pub fn apply(
        &mut self,
        root: &RootFs,
        paths: &[String],
        attribution: bool,
        boundaries: &[String],
    ) -> Vec<String> {
        // Only a folder the index holds can move a boundary: a `.git` below an
        // ignored or pruned folder is outside the tree (ASC-FR-09).
        let boundary_moved = boundaries.iter().any(|b| {
            !b.is_empty()
                && self.nodes.get(b).is_some_and(|n| n.kind == NodeKind::Folder)
                && repos::is_repo_root(root, b) != self.repo_roots.contains(b)
        }) || self.boundary_moved_above(root, paths);
        if attribution
            || boundary_moved
            || paths.len() > REBUILD_THRESHOLD
            || paths.iter().any(|p| needs_rebuild(p))
        {
            return self.rebuild(root);
        }
        // Each folder above a path, and each path that is a folder: a folder
        // walked again answers differently itself (ASC-FR-JYXO).
        let mut affected = affected_folders(paths);
        for rel in paths {
            if self.nodes.get(rel).is_some_and(|n| n.kind == NodeKind::Folder)
                && !affected.contains(rel)
            {
                affected.push(rel.clone());
            }
        }
        let before: BTreeMap<String, Vec<TreeNode>> =
            affected.iter().filter_map(|rel| Some((rel.clone(), self.children(rel)?))).collect();
        let mut rules = IgnoreRules::new(root);
        let mut touched: BTreeSet<String> = BTreeSet::new();
        for rel in paths {
            if rel.is_empty() || is_pruned_dir(rel) || is_atomic_tmp_name(rel) {
                continue;
            }
            if rel == LIBRARY_TOML_REL || repos::has_git_segment(rel) {
                continue;
            }
            match self.update_path(root, rel, &mut rules) {
                Update::Done => {
                    touched.insert(parent_of(rel).to_string());
                }
                Update::Rebuild => {
                    // The paths applied before this one already changed the
                    // index, so the diff of the rebuild cannot see them. Each
                    // folder above a path of the burst is reported as well.
                    let mut changed: BTreeSet<String> = self.rebuild(root).into_iter().collect();
                    changed
                        .extend(affected.iter().filter(|f| self.nodes.contains_key(*f)).cloned());
                    return changed.into_iter().collect();
                }
            }
        }
        // ASC-FR-OGRB: the roll-up of each folder above a touched one.
        let mut chain: BTreeSet<String> = BTreeSet::new();
        for folder in &touched {
            let mut dir = folder.as_str();
            loop {
                chain.insert(dir.to_string());
                if dir.is_empty() {
                    break;
                }
                dir = parent_of(dir);
            }
        }
        let mut ordered: Vec<String> = chain.into_iter().collect();
        ordered.sort_by_key(|rel| std::cmp::Reverse(depth(rel)));
        for rel in &ordered {
            self.roll_up(rel);
        }
        self.refresh_candidates();
        let mut changed: BTreeSet<String> = BTreeSet::new();
        for rel in affected.iter().chain(paths.iter()) {
            let after = self.children(rel);
            if after.is_some() && before.get(rel) != after.as_ref() {
                changed.insert(rel.clone());
            }
        }
        changed.into_iter().collect()
    }

    /// ASC-FR-UWMI, ASC-FR-YLGN: whether a repository boundary above a path of
    /// the burst moved. The watcher can deliver the files of a submodule
    /// checkout in a burst that holds no event for its `.git` entry, so each
    /// folder above a path that is a repository root, or an uninitialised
    /// submodule, is checked on disk.
    fn boundary_moved_above(&self, root: &RootFs, paths: &[String]) -> bool {
        let mut checked: BTreeSet<&str> = BTreeSet::new();
        for rel in paths {
            let mut dir = parent_of(rel);
            while !dir.is_empty() && checked.insert(dir) {
                let marked = self.repo_roots.contains(dir)
                    || self
                        .nodes
                        .get(dir)
                        .is_some_and(|n| n.nested == Some(NestedRepo::Uninitialised));
                if marked && repos::is_repo_root(root, dir) != self.repo_roots.contains(dir) {
                    return true;
                }
                dir = parent_of(dir);
            }
        }
        false
    }

    /// Bring the node of one path in line with the disk.
    fn update_path(&mut self, root: &RootFs, rel: &str, rules: &mut IgnoreRules) -> Update {
        let info = root.file_info(rel);
        let present = match &info {
            Ok(i) => matches!(i.kind, EntryKind::File | EntryKind::Dir),
            Err(_) => false,
        };
        if !present || rules.is_ignored(rel) || self.below_ignored_or_missing(root, rel) {
            self.remove(rel);
            return Update::Done;
        }
        let is_dir = info.as_ref().is_ok_and(|i| i.kind == EntryKind::Dir);
        if is_dir {
            if repos::is_repo_root(root, rel) && !self.repo_roots.contains(rel) {
                return Update::Rebuild;
            }
            // A folder the index holds is walked again only when its entries on
            // disk are not the ones the index holds: a folder moved in over it.
            // A folder event that only reports a change of one of its files
            // costs one listing.
            if self.nodes.get(rel).is_some_and(|n| n.kind == NodeKind::Folder)
                && self.entries_match_disk(root, rel, rules)
            {
                return Update::Done;
            }
            self.remove(rel);
            self.insert(Walked { rel: rel.to_string(), is_dir: true, class: FileClass::default() });
            // ASC-FR-YLGN: a listed folder with no `.git` entry.
            if self.listed.contains(rel) {
                if let Some(node) = self.nodes.get_mut(rel) {
                    node.nested = Some(NestedRepo::Uninitialised);
                }
            }
            let output = walk::walk_from(root, rel, Some(&self.assignments));
            if !output.repo_roots.is_empty() {
                return Update::Rebuild;
            }
            let mut entries = output.entries;
            entries.sort_by(|a, b| a.rel.len().cmp(&b.rel.len()).then_with(|| a.rel.cmp(&b.rel)));
            for entry in entries {
                self.insert(entry);
            }
            self.apply_folder_assignments();
            let mut inner: Vec<String> = self
                .nodes
                .keys()
                .filter(|k| k.as_str() == rel || k.starts_with(&format!("{rel}/")))
                .cloned()
                .collect();
            inner.sort_by_key(|k| std::cmp::Reverse(depth(k)));
            for folder in inner {
                self.roll_up(&folder);
            }
            return Update::Done;
        }
        let repo = repos::nearest_root(rel, &self.repo_roots);
        let class = walk::classify_walked(root, rel, &repo, &self.assignments);
        self.remove_if_folder(rel);
        self.insert(Walked { rel: rel.to_string(), is_dir: false, class });
        Update::Done
    }

    /// Whether the entries of the folder `rel` on disk, as the tree admits
    /// them, are the entries the index holds for it, by name and by kind.
    fn entries_match_disk(&self, root: &RootFs, rel: &str, rules: &mut IgnoreRules) -> bool {
        let Some(node) = self.nodes.get(rel) else {
            return false;
        };
        let Ok(listing) = root.list_dir(rel) else {
            return false;
        };
        let mut dirs = BTreeSet::new();
        let mut files = BTreeSet::new();
        for entry in listing {
            let child = repos::join_rel(rel, &entry.name);
            if entry.name == repos::GIT_ENTRY
                || child == LIBRARY_TOML_REL
                || is_pruned_dir(&child)
                || rules.is_ignored(&child)
            {
                continue;
            }
            match entry.kind {
                EntryKind::Dir => dirs.insert(entry.name),
                EntryKind::File => files.insert(entry.name),
                _ => false,
            };
        }
        dirs == node.dirs && files == node.files
    }

    /// Whether a folder above `rel` is not in the index and not a folder on
    /// disk, which a pruned, ignored, or linked parent gives.
    fn below_ignored_or_missing(&self, root: &RootFs, rel: &str) -> bool {
        let mut dir = parent_of(rel);
        while !dir.is_empty() {
            if self.nodes.contains_key(dir) {
                return false;
            }
            let is_dir = root.file_info(dir).is_ok_and(|i| i.kind == EntryKind::Dir);
            if !is_dir || is_pruned_dir(dir) {
                return true;
            }
            dir = parent_of(dir);
        }
        false
    }

    fn remove_if_folder(&mut self, rel: &str) {
        if self.nodes.get(rel).is_some_and(|n| n.kind == NodeKind::Folder) {
            self.remove(rel);
        }
    }

    /// Remove `rel` and everything below it, and each repository root there.
    fn remove(&mut self, rel: &str) {
        let Some(node) = self.nodes.remove(rel) else {
            return;
        };
        if node.kind == NodeKind::Folder && self.repo_roots.contains(rel) {
            self.repo_roots.remove(rel);
        }
        if node.kind == NodeKind::Folder {
            for name in node.dirs.iter().chain(node.files.iter()) {
                self.remove(&repos::join_rel(rel, name));
            }
        }
        let name = name_of(rel);
        if let Some(parent) = self.nodes.get_mut(parent_of(rel)) {
            parent.dirs.remove(name);
            parent.files.remove(name);
        }
    }

    /// ASC-FR-05, ASC-FR-07: classify `rel` and its subtree again after an
    /// assignment that this application wrote, and return the folders whose
    /// answer changed.
    pub fn reclassify(&mut self, root: &RootFs, rel: &str) -> Vec<String> {
        let mut affected: BTreeSet<String> = BTreeSet::new();
        let mut dir = parent_of(rel);
        loop {
            affected.insert(dir.to_string());
            if dir.is_empty() {
                break;
            }
            dir = parent_of(dir);
        }
        if self.nodes.get(rel).is_some_and(|n| n.kind == NodeKind::Folder) {
            for key in self.nodes.keys() {
                if (key == rel || key.starts_with(&format!("{rel}/")))
                    && self.nodes[key].kind == NodeKind::Folder
                {
                    affected.insert(key.clone());
                }
            }
        }
        let before: BTreeMap<String, Vec<TreeNode>> =
            affected.iter().filter_map(|f| Some((f.clone(), self.children(f)?))).collect();
        self.assignments = Arc::new(load_assignments(root));
        let files: Vec<String> = self
            .nodes
            .iter()
            .filter(|(k, n)| {
                n.kind == NodeKind::File && (k.as_str() == rel || k.starts_with(&format!("{rel}/")))
            })
            .map(|(k, _)| k.clone())
            .collect();
        for file in files {
            let repo = repos::nearest_root(&file, &self.repo_roots);
            let class = walk::classify_walked(root, &file, &repo, &self.assignments);
            if let Some(node) = self.nodes.get_mut(&file) {
                node.class = class;
            }
        }
        for (_, node) in self.nodes.iter_mut().filter(|(_, n)| n.kind == NodeKind::Folder) {
            node.class = FileClass::default();
        }
        self.apply_folder_assignments();
        let mut ordered: Vec<String> = affected.iter().cloned().collect();
        ordered.sort_by_key(|f| std::cmp::Reverse(depth(f)));
        for folder in &ordered {
            self.roll_up(folder);
        }
        self.refresh_candidates();
        affected
            .into_iter()
            .filter(|f| {
                let after = self.children(f);
                after.is_some() && before.get(f) != after.as_ref()
            })
            .collect()
    }
}

/// The wire node of one path, read from the disk, for a caller with no index
/// mounted: a node that a command has just created. `None` for a path that the
/// tree does not admit: missing, a link, pruned, or ignored (ASC-FR-09).
pub fn node_at(root: &RootFs, rel: &str) -> Option<TreeNode> {
    let rel = rel.trim_matches('/');
    if rel.is_empty() || rel == LIBRARY_TOML_REL || is_pruned_dir(rel) {
        return None;
    }
    let info = root.file_info(rel).ok()?;
    let is_dir = match info.kind {
        EntryKind::Dir => true,
        EntryKind::File => false,
        _ => return None,
    };
    if IgnoreRules::new(root).is_ignored(rel) {
        return None;
    }
    let assignments = Arc::new(load_assignments(root));
    let mut output =
        if is_dir { walk::walk_from(root, rel, Some(&assignments)) } else { WalkOutput::default() };
    let class = if is_dir {
        FileClass::default()
    } else {
        let repo = repos::nearest_root_on_disk(root, rel);
        walk::classify_walked(root, rel, &repo, &assignments)
    };
    output.entries.push(Walked { rel: rel.to_string(), is_dir, class });
    ProjectIndex::from_walk(output, assignments).tree_node(rel)
}

enum Update {
    Done,
    Rebuild,
}

/// ASC-FR-UWMI: the paths whose change the index cannot apply on its own
/// terms: an ignore file and `.gitmodules`, each of which changes what the
/// walk admits or how a folder is marked.
fn needs_rebuild(rel: &str) -> bool {
    let name = name_of(rel);
    name == ".gitignore" || name == ".gitmodules"
}

/// Each folder whose answer a burst can change: every folder above each path.
pub fn affected_folders(paths: &[String]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for rel in paths {
        let mut dir = parent_of(rel);
        loop {
            out.insert(dir.to_string());
            if dir.is_empty() {
                break;
            }
            dir = parent_of(dir);
        }
    }
    out.into_iter().collect()
}

fn depth(rel: &str) -> usize {
    if rel.is_empty() {
        0
    } else {
        rel.matches('/').count() + 1
    }
}
