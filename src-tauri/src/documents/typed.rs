//! The type sources of the Documents collection: the `doc` assignments of the
//! active worktree (`DCL-documents-collection.md` DCL-FR-DKXQ, DCL-FR-TQZB,
//! DCL-FR-NLCE, DCL-FR-HYAF).
//!
//! `worktree_type_sources` reads `.synthesis/library.toml` and the
//! classification of the active worktree. Everything else here is pure, and
//! merges what they say with the manual sources.

use std::collections::{BTreeMap, BTreeSet};

use crate::fs::{EntryKind, RootFs};
use crate::scanning::{self, ArtifactType, Scope};

use super::discovery::{Discovery, SourceReport};
use super::model::{normalise_path, DocumentFormat, SourceKind, SourceOrigin, StoredSource};

/// DCL-FR-DKXQ: what the active worktree adds to a refresh.
pub struct TypeSources<'a> {
    /// One source per `doc` assignment whose path exists, at its normalised
    /// absolute path in the active worktree.
    pub sources: Vec<StoredSource>,
    /// The normalised absolute path of the active worktree root (DCL-FR-HYAF).
    pub worktree_root: Option<String>,
    /// DCL-FR-TQZB: whether the file at a normalised absolute path resolves to
    /// `doc` (per `ASC-artifact-scanning.md` ASC-FR-06).
    pub is_doc: Box<dyn Fn(&str) -> bool + 'a>,
}

impl TypeSources<'static> {
    /// No type sources and no worktree root: the collection holds manual
    /// sources alone.
    pub fn none() -> Self {
        TypeSources { sources: Vec::new(), worktree_root: None, is_doc: Box::new(|_| false) }
    }
}

/// DCL-FR-DKXQ, DCL-FR-TQZB: the type sources of the worktree at `root`.
///
/// One source per `doc` assignment whose path exists as the kind its scope
/// names. An assignment for a missing path gives no source (ASC-FR-24). A file
/// is a document of a type folder when the one resolver of ASC-FR-06 gives it
/// the type `doc`.
pub fn worktree_type_sources(root: &RootFs) -> TypeSources<'static> {
    let assignments = scanning::load_assignments(root);
    let base = normalise_path(&root.path().to_string_lossy());
    let sources = assignments
        .assignments
        .iter()
        .filter(|(_, a)| a.artifact_type == ArtifactType::Doc)
        .filter_map(|(rel, a)| {
            let kind = match a.scope {
                Scope::File => SourceKind::File,
                Scope::Folder => SourceKind::Folder,
            };
            let info = root.file_info(rel).ok()?;
            let exists = matches!(
                (info.kind, kind),
                (EntryKind::File, SourceKind::File) | (EntryKind::Dir, SourceKind::Folder)
            );
            exists.then(|| StoredSource { kind, path: normalise_path(&format!("{base}/{rel}")) })
        })
        .collect();
    let content = scanning::markdown_facts(root);
    let prefix = base.clone();
    let is_doc = move |abs: &str| -> bool {
        let Some(rel) = abs.strip_prefix(prefix.as_str()).and_then(|r| r.strip_prefix('/')) else {
            return false;
        };
        scanning::classify_file(rel, &assignments, &content).0 == Some(ArtifactType::Doc)
    };
    TypeSources { sources, worktree_root: Some(base), is_doc: Box::new(is_doc) }
}

/// DCL-FR-ATNL: clear the `doc` assignment at the absolute `path`, when `path` is
/// below the worktree at `root` and carries one, through the attribution write
/// of ASC-FR-24. Returns whether an assignment was cleared. No other file than
/// `.synthesis/library.toml` is written.
pub fn clear_type_assignment(root: &RootFs, path: &str) -> crate::fs::FsResult<bool> {
    let base = normalise_path(&root.path().to_string_lossy());
    let Some(rel) = project_path_of(&normalise_path(path), Some(&base)) else {
        return Ok(false);
    };
    let is_doc = scanning::load_assignments(root)
        .assignments
        .get(&rel)
        .is_some_and(|a| a.artifact_type == ArtifactType::Doc);
    if !is_doc {
        return Ok(false);
    }
    scanning::clear(root, &rel)?;
    Ok(true)
}

/// The type sources that no manual source names. A path that is both is granted
/// and walked once, as the manual source.
pub(super) fn extra_type_sources(
    manual: &[StoredSource],
    typed: &TypeSources<'_>,
) -> Vec<StoredSource> {
    let named: BTreeSet<String> = manual.iter().map(|s| normalise_path(&s.path)).collect();
    typed.sources.iter().filter(|s| !named.contains(&normalise_path(&s.path))).cloned().collect()
}

/// DCL-FR-TQZB: add to `manual` the files the type sources include, which are
/// the supported files that resolve to `doc`.
pub(super) fn merge_files(manual: &mut Discovery, typed: Discovery, is_doc: &dyn Fn(&str) -> bool) {
    let keep = |files: BTreeMap<String, DocumentFormat>| {
        files.into_iter().filter(|(path, _)| is_doc(path)).collect::<Vec<_>>()
    };
    for (path, format) in keep(typed.files) {
        manual.unavailable_files.remove(&path);
        manual.files.insert(path, format);
    }
    for (path, format) in keep(typed.unavailable_files) {
        if !manual.files.contains_key(&path) {
            manual.unavailable_files.entry(path).or_insert(format);
        }
    }
}

/// DCL-FR-NLCE: the manual sources in stored order, then the type sources that
/// no manual source names, sorted by normalised path. A manual source whose path
/// is also a type source has the origin `both`.
pub(super) fn ordered_reports(
    manual: Vec<SourceReport>,
    typed_paths: &[StoredSource],
    extra: Vec<SourceReport>,
) -> Vec<(SourceOrigin, SourceReport)> {
    let typed: BTreeSet<String> = typed_paths.iter().map(|s| normalise_path(&s.path)).collect();
    let mut out: Vec<(SourceOrigin, SourceReport)> = manual
        .into_iter()
        .map(|report| {
            let origin = if typed.contains(&normalise_path(&report.source.path)) {
                SourceOrigin::Both
            } else {
                SourceOrigin::Manual
            };
            (origin, report)
        })
        .collect();
    let mut extra = extra;
    extra.sort_by_key(|report| normalise_path(&report.source.path));
    out.extend(extra.into_iter().map(|report| (SourceOrigin::Type, report)));
    out
}

/// DCL-FR-HYAF: the path of a document relative to the active worktree root,
/// with `/` separators, when the document is below that root.
pub(super) fn project_path_of(path: &str, worktree_root: Option<&str>) -> Option<String> {
    let root = worktree_root?.trim_end_matches('/');
    let rest = path.strip_prefix(root)?.strip_prefix('/')?;
    (!rest.is_empty()).then(|| rest.to_string())
}
