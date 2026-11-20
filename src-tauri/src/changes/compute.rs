//! The change sets of the repository set (CHC-FR-03, CHC-FR-04, CHC-FR-TMNY).
//!
//! Each member of the repository set (`GSM-git-submodules.md` GSM-FR-VPAS) is
//! diffed against its own base, its paths are mapped to the project, and the
//! submodules whose checked-out commit moved add one pointer entry to their
//! parent (CHC-FR-EBLI).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use git2::{Diff, Oid};

use super::repo_set::{Base, Bases, Member, RepoSet};
use super::*;
use crate::scanning;

/// A change as Git sees it, before project-relative mapping and classification.
#[derive(Clone, Debug, PartialEq)]
struct RawChange {
    /// Repository-relative current path.
    path: String,
    /// Repository-relative pre-rename path (renames/copies only).
    previous_path: Option<String>,
    status: ChangeStatus,
    added: u32,
    removed: u32,
    is_binary: bool,
}

/// Walk a diff into per-file changes with line counts.
///
/// Rename detection runs first (CHC-FR-09), then a single `foreach` pass
/// accumulates insertions and deletions per path. Binary deltas produce no line
/// callbacks, so their counts stay unset and are reported as `null`
/// (CHC-FR-08). The counting rule of CHC-FR-07 falls out of the diff itself: an
/// added or untracked file is all `+`, a deleted file is all `-`, and a modified
/// or renamed one is its unified diff against the comparison base.
fn collect_changes(diff: &mut Diff<'_>) -> Result<Vec<RawChange>, String> {
    find_renames(diff)?;

    // Both callbacks need to write; `foreach` hands each out as a separate
    // `&mut dyn FnMut`, so the shared accumulators go behind `RefCell`.
    let stats: RefCell<BTreeMap<String, (u32, u32)>> = RefCell::new(BTreeMap::new());
    let binary: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());

    let key_of = |delta: &git2::DiffDelta<'_>| -> Option<String> {
        delta.new_file().path().or_else(|| delta.old_file().path()).map(to_forward)
    };

    {
        let mut file_cb = |_delta: git2::DiffDelta<'_>, _progress: f32| true;
        let mut binary_cb = |delta: git2::DiffDelta<'_>, _bin: git2::DiffBinary<'_>| {
            if let Some(k) = key_of(&delta) {
                binary.borrow_mut().insert(k);
            }
            true
        };
        let mut line_cb = |delta: git2::DiffDelta<'_>,
                           _hunk: Option<git2::DiffHunk<'_>>,
                           line: git2::DiffLine<'_>| {
            let Some(k) = key_of(&delta) else { return true };
            let mut stats = stats.borrow_mut();
            let counts = stats.entry(k).or_insert((0, 0));
            match line.origin() {
                '+' => counts.0 += 1,
                '-' => counts.1 += 1,
                _ => {}
            }
            true
        };
        diff.foreach(&mut file_cb, Some(&mut binary_cb), None, Some(&mut line_cb))
            .map_err(|e| format!("failed to read diff: {e}"))?;
    }

    let stats = stats.into_inner();
    let binary = binary.into_inner();

    let mut out = Vec::new();
    for delta in diff.deltas() {
        let Some(status) = map_status(delta.status()) else {
            continue;
        };
        let Some(path) = key_of(&delta) else { continue };
        let previous_path = if status == ChangeStatus::Renamed {
            delta.old_file().path().map(to_forward).filter(|p| *p != path)
        } else {
            None
        };
        // `foreach` populates the delta's BINARY flag while generating each
        // patch; the binary callback above is the belt to that brace.
        let is_binary = binary.contains(&path)
            || delta.flags().is_binary()
            || delta.new_file().is_binary()
            || delta.old_file().is_binary();
        let (added, removed) = stats.get(&path).copied().unwrap_or((0, 0));
        out.push(RawChange { path, previous_path, status, added, removed, is_binary });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Classification (CHC-FR-10 / CHC-FR-11 / CHC-FR-RYNS)
// ---------------------------------------------------------------------------

/// The raw changes of one member.
struct MemberChanges<'s> {
    member: &'s Member,
    raw: Vec<RawChange>,
}

/// Turn raw Git changes into contract entries: map every path into the
/// project's coordinate system, classify it with the Library's own classifier
/// so a changed file carries exactly the type the Library shows (CHC-FR-10),
/// and key it by the stable id of ASC-FR-13 (CHC-FR-11). The path conventions
/// match the path relative to the root of the member that owns the file
/// (CHC-FR-RYNS).
///
/// A deleted path is classified from path convention and stored assignments
/// only — the content tiebreak of ASC-FR-04 needs a file that is no longer on
/// disk, so no content read is attempted for it (CHC-FR-10).
fn to_entries(root: &crate::fs::RootFs, changes: Vec<MemberChanges<'_>>) -> Vec<ChangeEntry> {
    let assignments = scanning::load_assignments(root);

    // Project-relative paths whose file is gone, so the content tiebreak is
    // skipped for them rather than attempted and failed.
    let deleted: BTreeSet<String> = changes
        .iter()
        .flat_map(|c| {
            c.raw
                .iter()
                .filter(|r| r.status == ChangeStatus::Deleted)
                .filter_map(|r| c.member.to_project(&r.path))
        })
        .collect();

    let root_owned = root.to_path_buf();
    let content = move |rel: &str| -> scanning::ContentFacts {
        if deleted.contains(rel) {
            return scanning::ContentFacts::default();
        }
        if !rel.to_ascii_lowercase().ends_with(".md") {
            return scanning::ContentFacts::default();
        }
        let path = root_owned.join(rel);
        if !path.is_file() {
            return scanning::ContentFacts::default();
        }
        match root.read_text(path) {
            Ok(text) => scanning::read_content_facts(&text),
            Err(_) => scanning::ContentFacts::default(),
        }
    };

    let mut entries = Vec::new();
    for MemberChanges { member, raw } in changes {
        for r in raw {
            let Some(path) = member.to_project(&r.path) else { continue };
            let previous_path = r.previous_path.as_deref().and_then(|p| member.to_project(p));
            let in_repo = scanning::repos::relative_to(&path, &member.path);
            let (artifact_type, type_source) =
                scanning::classify_file_in_repo(&path, in_repo, &assignments, &content);
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            entries.push(ChangeEntry {
                id: path.clone(),
                name,
                repository: member.path.clone(),
                entry_kind: EntryKind::File,
                previous_commit: None,
                current_commit: None,
                change_status: r.status,
                previous_path,
                added_lines: if r.is_binary { None } else { Some(r.added) },
                removed_lines: if r.is_binary { None } else { Some(r.removed) },
                is_binary: r.is_binary,
                artifact_type,
                type_source,
                path,
            });
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// Pointer entries (CHC-FR-EBLI)
// ---------------------------------------------------------------------------

/// CHC-FR-EBLI / GSM-FR-FCOF: one entry of the parent for each submodule whose
/// checked-out `HEAD` is not the commit the parent base records.
fn pointer_entries(set: &RepoSet, bases: &Bases<'_>) -> Vec<ChangeEntry> {
    let mut out = Vec::new();
    for (i, member) in set.members.iter().enumerate().skip(1) {
        let Some(current) = member.head_id() else { continue };
        let previous = set.recorded_pointer(bases, i);
        if previous == Some(current) {
            continue;
        }
        out.push(pointer_entry(member, previous, current));
    }
    out
}

fn pointer_entry(member: &Member, previous: Option<Oid>, current: Oid) -> ChangeEntry {
    let added = previous.is_none();
    ChangeEntry {
        id: member.path.clone(),
        path: member.path.clone(),
        name: member.name.clone(),
        repository: member.parent.clone().unwrap_or_default(),
        entry_kind: EntryKind::Submodule,
        previous_commit: previous.map(|oid| oid.to_string()),
        current_commit: Some(current.to_string()),
        change_status: if added { ChangeStatus::Added } else { ChangeStatus::Modified },
        previous_path: None,
        // The two "Subproject commit" lines that Git shows for a pointer.
        added_lines: Some(1),
        removed_lines: Some(if added { 0 } else { 1 }),
        is_binary: false,
        artifact_type: None,
        type_source: None,
    }
}

// ---------------------------------------------------------------------------
// Change-set computation
// ---------------------------------------------------------------------------

/// The diff of one member against `base`, scoped to the project.
fn member_diff<'r>(member: &'r Member, base: Option<&git2::Tree<'_>>) -> Result<Diff<'r>, String> {
    let mut opts = diff_options();
    scope_to_project(&mut opts, &member.prefix);
    member
        .repo
        .diff_tree_to_workdir_with_index(base, Some(&mut opts))
        .map_err(|e| format!("failed to diff working tree: {e}"))
}

/// The entries of every member against its base, the pointer entries
/// included, sorted by path (CHC-FR-12).
fn set_entries(
    root: &crate::fs::RootFs,
    set: &RepoSet,
    bases: &Bases<'_>,
) -> Result<Vec<ChangeEntry>, String> {
    let mut changes = Vec::with_capacity(set.members.len());
    for (member, base) in set.members.iter().zip(&bases.of) {
        // CHC-FR-AIZV: a submodule with no base gives its pointer entry alone.
        if matches!(base, repo_set::MemberBase::Missing) {
            continue;
        }
        let mut diff = member_diff(member, base.tree())?;
        changes.push(MemberChanges { member, raw: collect_changes(&mut diff)? });
    }
    let mut entries = to_entries(root, changes);
    // CHC-FR-EBLI: the parent diff reports an added pointer as a file at the
    // submodule root. The pointer entry stands for it, so the root of a member
    // is never a file entry.
    entries.retain(|e| set.submodule_at(&e.path).is_none());
    entries.extend(pointer_entries(set, bases));
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// CHC-FR-03 / CHC-FR-JFNC: each member's working tree against its `HEAD`.
/// Staged and unstaged modifications are both included and reported
/// identically — the returned entries carry no index state.
pub fn uncommitted_change_set(root: &crate::fs::RootFs) -> Result<ChangeSet, String> {
    let set = RepoSet::open(root)?;
    let bases = set.bases(Base::Head)?;
    let entries = set_entries(root, &set, &bases)?;
    Ok(ChangeSet {
        comparison: Comparison::Uncommitted,
        entries,
        repositories: set.infos(),
        unopened: set.unopened.clone(),
    })
}

/// CHC-FR-04 / CHC-FR-AIZV: the main working tree against the merge-base of
/// the current branch and `target_branch`, and each submodule against the
/// commit that its parent records at its own base.
pub fn branch_change_set(
    root: &crate::fs::RootFs,
    target_branch: &str,
) -> Result<ChangeSet, String> {
    let set = RepoSet::open(root)?;
    let bases = set.bases(Base::MergeBase(target_branch))?;
    let entries = set_entries(root, &set, &bases)?;
    let merge_base = bases.merge_base.map(|oid| oid.to_string()).unwrap_or_default();
    Ok(ChangeSet {
        comparison: Comparison::Branch { target_branch: target_branch.to_string(), merge_base },
        entries,
        repositories: set.infos(),
        unopened: set.unopened.clone(),
    })
}
