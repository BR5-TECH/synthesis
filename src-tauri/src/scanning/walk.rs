//! The parallel walk that builds the project index (`ASC-artifact-scanning.md`
//! ASC-FR-01, ASC-FR-09, ASC-FR-TPJO, ASC-FR-SFCR, ASC-FR-TWVB, ASC-FR-LXOQ).
//!
//! One walk runs per repository root: the project root first, then each nested
//! repository root that a walk finds. A nested root is pruned from the walk of
//! its parent, so the ignore rules of the parent never reach below it, and its
//! own walk reads its own `.gitignore` files and its own `info/exclude`. Each
//! walk runs on several threads and classifies each file where it finds it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::{DirEntry, ParallelVisitor, ParallelVisitorBuilder, WalkBuilder, WalkState};

use super::ignored::IgnoreRules;
use super::repos::{self, NestedRepo, GIT_ENTRY};
use super::{
    classify_file_in_repo, is_pruned_dir, markdown_facts, to_forward, ArtifactType,
    LibraryAssignments, TypeSource, LIBRARY_TOML_REL,
};
use crate::fs::RootFs;

/// The classification of one file, as the walk resolved it (ASC-FR-06,
/// ASC-FR-19).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FileClass {
    pub artifact_type: Option<ArtifactType>,
    pub type_source: Option<TypeSource>,
    pub display_name: Option<String>,
}

/// One entry the walk surfaced: its project-relative path, whether it is a
/// folder, and, for a file, its classification.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Walked {
    pub rel: String,
    pub is_dir: bool,
    pub class: FileClass,
}

/// What a walk of the whole project gives the index.
#[derive(Debug, Default)]
pub(crate) struct WalkOutput {
    pub entries: Vec<Walked>,
    /// ASC-FR-SFCR: every nested repository root, project-relative.
    pub repo_roots: BTreeSet<String>,
    /// ASC-FR-YLGN: the boundary kind of each folder that is one.
    pub nested: Vec<(String, NestedRepo)>,
    /// ASC-FR-YLGN: every path a `.gitmodules` lists, project-relative.
    pub listed: BTreeSet<String>,
}

/// ASC-FR-01, ASC-FR-SFCR: walk the project and each repository inside it.
pub(crate) fn walk_project(root: &RootFs, assignments: &Arc<LibraryAssignments>) -> WalkOutput {
    walk_from(root, "", Some(assignments))
}

/// The entries of the whole project, on the terms of [`walk_project`], with no
/// classification and no content read.
pub(crate) fn walk_paths(root: &RootFs) -> Vec<Walked> {
    walk_from(root, "", None).entries
}

/// Walk the folder `start` (project-relative, `""` for the root) and every
/// nested repository root below it. `start` itself is not an entry.
pub(crate) fn walk_from(
    root: &RootFs,
    start: &str,
    assignments: Option<&Arc<LibraryAssignments>>,
) -> WalkOutput {
    let mut out = WalkOutput::default();
    let mut queue = vec![start.to_string()];
    while let Some(folder) = queue.pop() {
        let (entries, found) = if folder.is_empty() || repos::is_repo_root(root, &folder) {
            walk_one(root, &folder, assignments)
        } else {
            walk_subtree(root, &folder, assignments)
        };
        out.entries.extend(entries);
        for nested in found {
            out.entries.push(Walked {
                rel: nested.clone(),
                is_dir: true,
                class: FileClass::default(),
            });
            out.repo_roots.insert(nested.clone());
            queue.push(nested);
        }
    }
    // ASC-FR-YLGN: a path counts as listed only in the `.gitmodules` of the
    // repository that encloses it.
    let mut repos_here: Vec<String> = vec![String::new()];
    repos_here.extend(out.repo_roots.iter().cloned());
    let folders: BTreeSet<&str> =
        out.entries.iter().filter(|e| e.is_dir).map(|e| e.rel.as_str()).collect();
    for repo in &repos_here {
        for path in repos::submodule_paths(root, repo) {
            if repos::nearest_root(&path, &out.repo_roots_except(&path)) == *repo {
                out.listed.insert(path);
            }
        }
    }
    for rel in &out.repo_roots {
        let kind =
            if out.listed.contains(rel) { NestedRepo::Submodule } else { NestedRepo::Repository };
        out.nested.push((rel.clone(), kind));
    }
    for rel in &out.listed {
        if folders.contains(rel.as_str()) && !out.repo_roots.contains(rel) {
            out.nested.push((rel.clone(), NestedRepo::Uninitialised));
        }
    }
    out
}

impl WalkOutput {
    /// The nested repository roots other than `rel` itself.
    fn repo_roots_except(&self, rel: &str) -> BTreeSet<String> {
        self.repo_roots.iter().filter(|r| r.as_str() != rel).cloned().collect()
    }
}

/// One parallel walk of the repository root `repo`. Returns the entries below
/// it and the nested repository roots it pruned.
fn walk_one(
    root: &RootFs,
    repo: &str,
    assignments: Option<&Arc<LibraryAssignments>>,
) -> (Vec<Walked>, Vec<String>) {
    let abs = root.join(repo);
    let mut builder = WalkBuilder::new(&abs);
    builder
        .hidden(false) // surface dotdirs like `.claude`
        .git_ignore(true)
        .git_global(false)
        // ASC-FR-TWVB: the rules are `.gitignore` and `info/exclude` alone, so
        // a `.ignore` file of another tool is not read.
        .ignore(false)
        // ASC-FR-TWVB: the exclude file is added below, from the git
        // directory that the `.git` entry names, which a `.git` file needs.
        .git_exclude(false)
        .require_git(false)
        .parents(false)
        .current_dir(&abs);
    if let Some(exclude) = repos::exclude_file_of(root, repo) {
        let _ = builder.add_ignore(exclude);
    }
    let nested = Arc::new(Mutex::new(Vec::new()));
    let filter_root = root.clone();
    let filter_nested = Arc::clone(&nested);
    let root_abs: PathBuf = root.path().to_path_buf();
    builder.filter_entry(move |entry| keep_entry(entry, &root_abs, &filter_root, &filter_nested));

    let results = Arc::new(Mutex::new(Vec::new()));
    let mut visitors = VisitorBuilder {
        root: root.clone(),
        repo: repo.to_string(),
        assignments: assignments.cloned(),
        results: Arc::clone(&results),
    };
    builder.build_parallel().visit(&mut visitors);
    let entries = take(&results);
    let mut found = take(&nested);
    found.sort();
    (entries, found)
}

/// ASC-FR-UWMI: one walk of a folder that is not a repository root, such as a
/// folder that a watcher burst created. The ignore rules of its repository
/// apply from that root down (ASC-FR-TWVB), so each entry is matched by
/// [`IgnoreRules`] rather than by the walker, which reads no `.gitignore`
/// above where it starts.
fn walk_subtree(
    root: &RootFs,
    folder: &str,
    assignments: Option<&Arc<LibraryAssignments>>,
) -> (Vec<Walked>, Vec<String>) {
    let repo = repos::nearest_root_on_disk(root, &repos::join_rel(folder, "x"));
    let mut builder = WalkBuilder::new(root.join(folder));
    builder.standard_filters(false).hidden(false);
    let nested = Arc::new(Mutex::new(Vec::new()));
    let rules = Arc::new(Mutex::new(IgnoreRules::new(root)));
    let filter_root = root.clone();
    let filter_nested = Arc::clone(&nested);
    let filter_rules = Arc::clone(&rules);
    let root_abs: PathBuf = root.path().to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.depth() > 0 {
            let rel = entry.path().strip_prefix(&root_abs).map(to_forward).unwrap_or_default();
            let ignored = filter_rules.lock().is_ok_and(|mut r| r.is_ignored(&rel));
            if ignored {
                return false;
            }
        }
        keep_entry(entry, &root_abs, &filter_root, &filter_nested)
    });
    let mut visitor = Visitor {
        root: root.clone(),
        repo,
        assignments: assignments.cloned(),
        local: Vec::new(),
        results: Arc::new(Mutex::new(Vec::new())),
    };
    for result in builder.build() {
        visitor.visit(result);
    }
    let entries = std::mem::take(&mut visitor.local);
    let mut found = take(&nested);
    found.sort();
    (entries, found)
}

fn take<T>(shared: &Arc<Mutex<Vec<T>>>) -> Vec<T> {
    shared.lock().map(|mut guard| std::mem::take(&mut *guard)).unwrap_or_default()
}

/// The `filter_entry` rule: prune links, `.git` entries, the ASC-FR-09 prune
/// set, and nested repository roots, which are recorded and walked on their
/// own.
fn keep_entry(
    entry: &DirEntry,
    root_abs: &Path,
    root: &RootFs,
    nested: &Arc<Mutex<Vec<String>>>,
) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    // ASC-FR-23: never descend a symbolic link, and never surface one.
    if entry.file_type().is_some_and(|t| t.is_symlink()) {
        return false;
    }
    // ASC-FR-SFCR: the `.git` entry of every repository root is skipped.
    if entry.file_name() == GIT_ENTRY {
        return false;
    }
    let Ok(rel) = entry.path().strip_prefix(root_abs).map(to_forward) else {
        return false;
    };
    if is_pruned_dir(&rel) {
        return false;
    }
    if entry.file_type().is_some_and(|t| t.is_dir()) && repos::is_repo_root(root, &rel) {
        if let Ok(mut guard) = nested.lock() {
            guard.push(rel);
        }
        return false;
    }
    true
}

struct VisitorBuilder {
    root: RootFs,
    repo: String,
    /// `None` for a walk that classifies nothing.
    assignments: Option<Arc<LibraryAssignments>>,
    results: Arc<Mutex<Vec<Walked>>>,
}

impl<'s> ParallelVisitorBuilder<'s> for VisitorBuilder {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 's> {
        Box::new(Visitor {
            root: self.root.clone(),
            repo: self.repo.clone(),
            assignments: self.assignments.clone(),
            local: Vec::new(),
            results: Arc::clone(&self.results),
        })
    }
}

/// One walker thread. It keeps its entries in a local list and hands them
/// over once, when the thread ends, so the threads do not contend on a lock
/// per entry.
struct Visitor {
    root: RootFs,
    repo: String,
    assignments: Option<Arc<LibraryAssignments>>,
    local: Vec<Walked>,
    results: Arc<Mutex<Vec<Walked>>>,
}

impl ParallelVisitor for Visitor {
    fn visit(&mut self, result: Result<DirEntry, ignore::Error>) -> WalkState {
        // ASC-FR-11: an entry that errors mid-walk is skipped, not fatal.
        let Ok(entry) = result else {
            return WalkState::Continue;
        };
        if entry.depth() == 0 {
            return WalkState::Continue;
        }
        let Ok(rel) = entry.path().strip_prefix(self.root.path()).map(to_forward) else {
            return WalkState::Continue;
        };
        // ASC-FR-09: the attribution file is never a node of the tree.
        if rel.is_empty() || rel == LIBRARY_TOML_REL {
            return WalkState::Continue;
        }
        if entry.file_type().is_some_and(|t| t.is_symlink()) {
            return WalkState::Continue;
        }
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        let class = match (&self.assignments, is_dir) {
            (Some(assignments), false) => {
                classify_walked(&self.root, &rel, &self.repo, assignments)
            }
            _ => FileClass::default(),
        };
        self.local.push(Walked { rel, is_dir, class });
        WalkState::Continue
    }
}

impl Drop for Visitor {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.results.lock() {
            guard.append(&mut self.local);
        }
    }
}

/// ASC-FR-06, ASC-FR-19, ASC-FR-LXOQ: classify one file of the repository
/// `repo`.
pub(crate) fn classify_walked(
    root: &RootFs,
    rel: &str,
    repo: &str,
    assignments: &LibraryAssignments,
) -> FileClass {
    let content = markdown_facts(root);
    let repo_rel = repos::relative_to(rel, repo);
    let (artifact_type, type_source) = classify_file_in_repo(rel, repo_rel, assignments, &content);
    let display_name = if super::inference::names_itself(rel, artifact_type) {
        content(rel).declared_name
    } else {
        None
    };
    FileClass { artifact_type, type_source, display_name }
}
