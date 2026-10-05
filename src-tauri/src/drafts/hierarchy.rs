//! Where a draft sits, and the walk of the drafts root that finds it
//! (`DRS-draft-storage.md` DRS-FR-29, DRS-FR-34, DRS-FR-37).

use super::*;

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub(super) fn drafts_dir(root: &Path) -> PathBuf {
    root.join(DRAFTS_REL)
}

/// The typed "not found" every operation on the drafts root answers a path that
/// no longer names what it named with (DRS-FR-35).
pub(super) fn no_such_folder(path: &str) -> String {
    format!("no such drafts folder: {path:?}")
}

/// Whether `dir` is a **draft folder** rather than one of the author's: it
/// directly contains `draft.toml` (DRS-FR-29). That marker is the whole of the
/// distinction — there is no manifest saying which is which.
pub(super) fn is_draft_folder(root: &fs::RootFs, dir: &Path) -> bool {
    root.file_info(dir.join(RECORD_FILE)).is_ok()
}

/// How deep the drafts-root walk descends before it stops.
///
/// The tree the author cuts is unbounded in principle and shallow in practice.
/// The bound is here because the walk runs over directories nothing in this
/// application created — a file manager, another tool, or a hand-made symlink
/// loop the kind check below misses on some platform — and an unbounded
/// recursion over one of those takes down every caller of `list_drafts`, the
/// panel included.
pub(super) const MAX_FOLDER_DEPTH: usize = 32;

/// One draft, and where it was found.
pub(super) struct DraftAt {
    pub(super) id: String,
    /// Drafts-root-relative folder path; `""` for a draft at the root.
    pub(super) folder: String,
    /// Absolute path of the draft's own directory.
    pub(super) dir: PathBuf,
}

/// DRS-FR-29: walk the drafts root, classifying every directory it meets.
///
/// A directory holding `draft.toml` is a draft and the walk stops there rather
/// than descending into the `files/` and `proposals/` beneath it;
/// anything else is one of the author's folders and is both recorded and
/// descended into. Symlinks are skipped rather than followed, for the reason
/// [`read_tree`] skips them: a link to an ancestor would recurse without end,
/// and a link out of the drafts root would put another directory's contents in
/// the panel.
pub(super) fn walk_drafts_root(
    root: &fs::RootFs,
    dir: &Path,
    rel: &str,
    depth: usize,
    folders: &mut Vec<DraftFolder>,
    drafts: &mut Vec<DraftAt>,
) {
    let Ok(entries) = root.list_dir(dir) else {
        return;
    };
    for entry in entries {
        if entry.kind != fs::EntryKind::Dir {
            continue;
        }
        let child = dir.join(&entry.name);
        let child_rel = folder_join(rel, &entry.name);
        if is_draft_folder(root, &child) {
            // A draft folder is named for the draft's id. One that is not is
            // not addressable by any command here, so it is left alone rather
            // than listed as something the author could act on.
            if is_valid_draft_id(&entry.name) {
                drafts.push(DraftAt {
                    id: entry.name.clone(),
                    folder: rel.to_string(),
                    dir: child,
                });
            }
            continue;
        }
        // A directory the author could not have created through this module is
        // not offered back as one they can rename or move into.
        if !is_valid_folder_name(&entry.name) {
            continue;
        }
        folders.push(DraftFolder {
            path: child_rel.clone(),
            parent: rel.to_string(),
        });
        if depth + 1 < MAX_FOLDER_DEPTH {
            walk_drafts_root(root, &child, &child_rel, depth + 1, folders, drafts);
        }
    }
}

/// Every folder and every draft under the drafts root, in one walk.
pub(super) fn scan_drafts_root(root: &fs::RootFs) -> (Vec<DraftFolder>, Vec<DraftAt>) {
    let mut folders = Vec::new();
    let mut drafts = Vec::new();
    walk_drafts_root(root, &drafts_dir(root), "", 0, &mut folders, &mut drafts);
    // Case-insensitively by path, so the hierarchy comes back in the order the
    // panel renders it (DRP-FR-21) rather than in readdir order, and a re-list
    // never reshuffles the tree beneath the author.
    folders.sort_by_key(|f| f.path.to_lowercase());
    (folders, drafts)
}

/// DRS-FR-37: reunite a draft with draft-owned storage left outside it.
///
/// A directory beneath the drafts root is **misplaced draft storage** rather
/// than one of the author's folders (DRS-FR-29) when three things hold at once:
/// its name is a well-formed draft id, a draft carrying exactly that id is filed
/// elsewhere in the hierarchy, and it holds nothing but the storage folders a
/// draft's own directory holds (DRS-FR-01). All three are
/// required because the second is the one that resolves to a fact rather than a
/// resemblance: an author may name a folder anything DRS-FR-30 accepts, so a
/// directory is adopted only where the draft whose storage it holds
/// demonstrably exists somewhere else.
///
/// Takes the scan its caller already ran, so a hierarchy with nothing to repair
/// — every hierarchy, after the write path that produced these was corrected —
/// costs no walk of its own. Returns the ids repaired, empty in that ordinary
/// case.
pub(super) fn reconcile_misplaced_storage(
    root: &fs::RootFs,
    folders: &[DraftFolder],
    drafts: &[DraftAt],
) -> Vec<String> {
    let mut repaired = Vec::new();
    for folder in folders {
        let Some(name) = folder.path.rsplit('/').next() else {
            continue;
        };
        if !is_valid_draft_id(name) {
            continue;
        }
        // The load-bearing condition: the draft whose storage this would be
        // exists, and is filed somewhere this directory is not.
        let Some(draft) = drafts.iter().find(|d| d.id == name) else {
            continue;
        };
        let dest_rel = folder_join(&draft.folder, &draft.id);
        if dest_rel == folder.path {
            continue;
        }
        // A husk nested inside one already merged is gone with its parent, and
        // the listing failing is how this notices rather than a second stat.
        if !holds_only_draft_storage(root, &drafts_dir(root).join(&folder.path)) {
            continue;
        }
        if merge_draft_storage(root, &folder.path, &dest_rel) && !repaired.contains(&draft.id) {
            // Named once however many husks a draft accumulated, so the count
            // in the log is drafts repaired rather than directories visited.
            repaired.push(draft.id.clone());
        }
    }
    repaired
}

/// Whether `dir` holds nothing but the storage folders of DRS-FR-01, and holds
/// at least one of them.
///
/// At least one, because an empty directory holds no misplaced storage to
/// reunite with anything: it is far likelier a folder the author made and has
/// not filled than the residue of a write that never happened, and removing one
/// they made would be the destructive reading of a rule whose whole purpose is
/// repair. A `draft.toml` cannot appear here — a directory holding one is
/// classified a draft folder before this is ever asked (DRS-FR-29).
pub(super) fn holds_only_draft_storage(root: &fs::RootFs, dir: &Path) -> bool {
    let Ok(entries) = root.list_dir(dir) else {
        return false;
    };
    !entries.is_empty()
        && entries.iter().all(|e| {
            e.kind == fs::EntryKind::Dir && HUSK_STORAGE_DIRS.contains(&e.name.as_str())
        })
}

/// DRS-FR-37: the folders a husk may hold and still be a draft's storage.
///
/// The storage folders of a draft's own directory (DRS-FR-01), and `comments/`
/// — which is not one of them. An earlier build kept a draft's review there, so
/// a husk written by that build holds nothing else; reuniting it is what lets
/// the import pass of `RMS-repository-machine-storage.md` (RMS-FR-JVEC) find
/// those conversations and carry them into the store, where this build reads a
/// draft's review from (per `CMS-comments-storage.md` CMS-FR-37).
const HUSK_STORAGE_DIRS: [&str; 4] = [PROPOSALS_DIR, HISTORY_DIR, ASSETS_DIR, "comments"];

/// Move everything under the husk at `husk_rel` into the draft at `dest_rel`,
/// then remove the emptied husk. Both are drafts-root-relative.
///
/// Returns whether anything moved — which is not the same as whether the husk is
/// gone. A husk that does not empty is left standing rather than removed
/// (DRS-FR-37), nothing being deleted that was not accounted for, so a file this
/// merge could not place survives where it survives for the author to find; but
/// the husk has still given up the children it did place, and the caller has to
/// re-walk to describe a tree that changed under it either way.
pub(super) fn merge_draft_storage(root: &fs::RootFs, husk_rel: &str, dest_rel: &str) -> bool {
    let moved = merge_dir_into(root, husk_rel, dest_rel, 0);
    // Non-recursively, deliberately: a husk that still holds something is one
    // this merge did not fully account for, and the delete failing is how it
    // stays on disk rather than being taken with everything under it.
    let removed = root.delete_under(drafts_dir(root), husk_rel, false).is_ok();
    moved || removed
}

/// Merge one drafts-root-relative directory's contents into another, recursively.
///
/// The merge needs no conflict rule of its own, which is what makes it safe to
/// perform without asking anyone: a log present in both places concatenates, the
/// fold applying each `event_id` once however many times a line appears
/// (CMS-FR-06), and an attachment is named by the digest of its own bytes, so a
/// file present in both places is the same file (CMS-FR-44). Anything that is
/// neither — a name in both places whose bytes differ and which is not a log —
/// is left where it is rather than guessed about, which leaves the husk
/// non-empty and therefore standing.
///
/// Every mutation goes through the drafts root as its base, so nothing here can
/// reach outside `.synthesis/drafts/` however the names on disk read (DRS-FR-34).
///
/// Bounded by the same [`MAX_FOLDER_DEPTH`] the drafts walk is bounded by, and
/// for the identical reason: what sits under a husk is content nothing in this
/// application created — `holds_only_draft_storage` vouches for the husk's own
/// children and for nothing nested beneath them — and an unbounded recursion
/// over a directory tree some other tool left there would overflow the stack and
/// abort the process, from a path that runs every time the panel lists. Returns
/// whether anything on disk moved.
pub(super) fn merge_dir_into(root: &fs::RootFs, from_rel: &str, to_rel: &str, depth: usize) -> bool {
    if depth >= MAX_FOLDER_DEPTH {
        return false;
    }
    let base = drafts_dir(root);
    let Ok(entries) = root.list_dir(base.join(from_rel)) else {
        return false;
    };
    let mut moved = false;
    for entry in entries {
        let src_rel = folder_join(from_rel, &entry.name);
        let dst_rel = folder_join(to_rel, &entry.name);
        if root.file_info(base.join(&dst_rel)).is_err() {
            // Nothing there to reconcile with: one rename carries the file, or
            // the whole subtree, however large.
            moved |= root.move_under(&base, &src_rel, &dst_rel).is_ok();
            continue;
        }
        if entry.kind == fs::EntryKind::Dir {
            moved |= merge_dir_into(root, &src_rel, &dst_rel, depth + 1);
            moved |= root.delete_under(&base, &src_rel, false).is_ok();
            continue;
        }
        // A log in both places: append what the husk holds to what the draft
        // holds. The fold dedups by `event_id`, so a line already in both
        // contributes nothing a second time.
        if entry.name.ends_with(LOG_EXT) {
            let Ok(text) = root.read_text(base.join(&src_rel)) else {
                continue;
            };
            let lines: Vec<String> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect();
            if lines.is_empty() || root.append_lines(base.join(&dst_rel), &lines).is_ok() {
                moved |= root.delete_under(&base, &src_rel, false).is_ok();
            }
            continue;
        }
        // Content-addressed, so the same name is the same bytes — verified
        // rather than assumed, because dropping a file on a naming convention
        // alone is how a repair becomes a loss.
        if let (Ok(a), Ok(b)) = (
            root.read_bytes(base.join(&src_rel)),
            root.read_bytes(base.join(&dst_rel)),
        ) {
            if a == b {
                moved |= root.delete_under(&base, &src_rel, false).is_ok();
            }
        }
    }
    moved
}

/// Where a draft's directory sits, found by walking rather than computed
/// (DRS-FR-29): the path is the state, and the id says nothing about it.
pub(super) fn find_draft(root: &fs::RootFs, id: &str) -> Result<DraftAt, String> {
    if !is_valid_draft_id(id) {
        return Err(format!("invalid draft id: {id:?}"));
    }
    let (_, drafts) = scan_drafts_root(root);
    drafts
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("draft not found: {id}"))
}

/// The directory a draft filed in `folder` would sit at, through the escape
/// gate. Used where the draft does not exist yet — every other caller finds an
/// existing draft with [`find_draft`].
pub(super) fn draft_dir_in(root: &fs::RootFs, folder: &str, id: &str) -> Result<PathBuf, String> {
    if !is_valid_draft_id(id) {
        return Err(format!("invalid draft id: {id:?}"));
    }
    Ok(resolve_folder_path(root, folder)?.join(id))
}

/// Every draft of the active worktree, as `(id, directory)`.
///
/// The one enumeration a module outside this one reaches a draft's directory
/// through without holding an id first, and the walk of DRS-FR-29 is the whole
/// of it. `RMS-repository-machine-storage.md`'s import pass reads it, because
/// the folder it has to look in for an earlier build's review logs is inside
/// each draft and only the walk knows where each draft is filed (DRS-FR-36).
pub fn draft_locations(root: &fs::RootFs) -> Vec<(String, PathBuf)> {
    let (_, drafts) = scan_drafts_root(root);
    drafts.into_iter().map(|d| (d.id, d.dir)).collect()
}

/// DRS-FR-36: the absolute path of an existing draft's own directory, for this
/// module and for the siblings holding storage inside it.
///
/// Found by the walk rather than composed from the id, and that distinction is
/// the whole point of exposing it. A draft sits at
/// `.synthesis/drafts/<folder-path>/<draft-id>/` (DRS-FR-01), and `<folder-path>`
/// is a fact only the walk holds — an id says nothing about where the draft
/// carrying it is filed (DRS-FR-02). A path composed from the id alone therefore
/// names the right directory for a draft that happens to sit at the root and a
/// directory that does not exist for every other, and *how* that fails is why no
/// caller is left to compose one: writing to a composed path does not error, it
/// creates the directories it names, so the draft's own storage stays empty
/// while a directory bearing its id accumulates beside the author's folders and
/// is classified as one of them (DRS-FR-29). [`reconcile_misplaced_storage`] is
/// what repairs a hierarchy where that has already happened.
pub fn draft_dir(root: &fs::RootFs, id: &str) -> Result<PathBuf, String> {
    Ok(find_draft(root, id)?.dir)
}

/// DRS-FR-36: the `history/` folder of DRS-FR-01 inside a draft's own directory
/// — the prompt's settled versions and the acceptance journal owned by
/// `crate::draft_history` (DHS-FR-01).
pub fn draft_history_dir(root: &fs::RootFs, id: &str) -> Result<PathBuf, String> {
    Ok(draft_dir(root, id)?.join(HISTORY_DIR))
}

/// DRS-FR-40: the `assets/` folder of DRS-FR-01 inside a draft's own directory
/// — the images the prompt embeds, owned by `crate::draft_assets` (DAS-FR-01).
///
/// Resolved on exactly the terms its three siblings are (DRS-FR-36): it **walks
/// to the draft** rather than composing a path from the id, so filing a draft
/// under a folder or moving it between folders carries its assets with it, and
/// it returns the typed "not found" for an id no draft in the active worktree
/// carries. It carries no precondition on status and none on the graduation
/// lock — the lock is enforced by the operations that write rather than by the
/// resolver (per `DAS-draft-assets.md` DAS-FR-25), and a graduated draft's
/// assets are read exactly as any other draft's are.
///
/// Registered as no Tauri command, so no frontend call reaches a draft's assets
/// through it.
pub fn draft_assets_dir(root: &fs::RootFs, id: &str) -> Result<PathBuf, String> {
    Ok(draft_dir(root, id)?.join(ASSETS_DIR))
}

/// DRS-FR-EJBM: the `publication.toml` of a draft's own directory — the
/// append-only GitHub publication history and the draft's one standing
/// publication attempt, owned by `crate::github_publication`.
///
/// Resolved on exactly the terms [`draft_assets_dir`] is (DRS-FR-36): it walks
/// to the draft rather than composing a path from the id, so a draft moved
/// between drafts folders keeps its publication store. The file is **absent
/// until the first attempt** (DRS-FR-PSCH), so this resolves a path rather than
/// asserting one exists.
pub fn draft_publication_path(root: &fs::RootFs, id: &str) -> Result<PathBuf, String> {
    Ok(draft_dir(root, id)?.join(PUBLICATION_FILE))
}

/// DRS-FR-34: resolve a drafts-root-relative folder path to an absolute one,
/// without requiring that it exist.
///
/// Syntactic only — the existence and drafts-folder checks belong to
/// [`resolve_drafts_folder`], which is what every caller acting on an existing
/// folder uses.
pub(super) fn resolve_folder_path(root: &fs::RootFs, folder: &str) -> Result<PathBuf, String> {
    if !is_valid_folder_path(folder) {
        return Err(format!("invalid drafts folder path: {folder:?}"));
    }
    if folder.is_empty() {
        return Ok(drafts_dir(root));
    }
    let abs = fs::resolve_under(drafts_dir(root), folder).map_err(|e| e.to_string())?;
    reject_symlinked_components(root, &drafts_dir(root), folder)?;
    Ok(abs)
}

/// DRS-FR-34 / DRS-FR-35: resolve a drafts folder that must exist and must be
/// one of the author's rather than a draft's own directory.
///
/// A path that no longer names what it named — renamed, moved, or deleted
/// underneath the caller — comes back as the typed "not found", which is the
/// whole of the staleness signal: the paths *are* the state, so there is no
/// revision token to carry.
pub(super) fn resolve_drafts_folder(root: &fs::RootFs, folder: &str) -> Result<PathBuf, String> {
    let abs = resolve_folder_path(root, folder)?;
    if folder.is_empty() {
        // The implicit root always exists (PST-FR-02 scaffolds it), and a
        // worktree whose drafts folder has simply never been written has no
        // drafts rather than a missing root.
        return Ok(abs);
    }
    // Every component, not only the last. `files`, `proposals` and a draft's own
    // id are all syntactically legal folder names, so a path like
    // `UI/<draft-id>/files/notes` reaches *inside a draft* while reading as an
    // ordinary drafts-folder path — and `walk_drafts_root` stops at a draft
    // folder (DRS-FR-29), so anything filed under one would be invisible to
    // `list_drafts` for good, with no way back through any command here.
    // Descending the way the walk itself descends is what keeps this module's
    // claim true: no operation on the drafts root reaches inside a draft.
    let mut current = drafts_dir(root);
    for segment in folder.split('/') {
        if is_draft_folder(root, &current) {
            return Err(no_such_folder(folder));
        }
        current.push(segment);
    }
    if !current.is_dir() || is_draft_folder(root, &current) {
        return Err(no_such_folder(folder));
    }
    Ok(abs)
}

/// Whether `dir` already holds an entry called `name`, compared
/// case-insensitively (DRS-FR-30).
///
/// Case-insensitively and on every platform, for the reason
/// [`validate_destinations`] compares its own destinations that way: `UI` and
/// `ui` are two directories on ext4 and one on APFS and NTFS, so accepting the
/// pair means a move on those filesystems lands in a directory the author
/// thought was a different one.
/// Whether `path` is `folder` or sits anywhere inside its subtree, compared
/// **case-insensitively**.
///
/// Case-folded for the reason [`name_taken`] is: on APFS and NTFS `ui/x` and
/// `UI/x` are one directory, so a byte-comparison lets
/// `move_drafts_folder("UI", "ui/Components")` past the "never inside itself"
/// refusal of DRS-FR-33 — the destination resolves to the real descendant, and
/// the caller gets a raw `EINVAL` from the kernel instead of the typed error the
/// contract promises. Folded on every platform, because a rule that holds only
/// where the filesystem happens to be case-sensitive is not a rule.
pub(super) fn folder_contains(folder: &str, path: &str) -> bool {
    if folder.is_empty() {
        return true;
    }
    let folder = folder.to_lowercase();
    let path = path.to_lowercase();
    path == folder || path.starts_with(&format!("{folder}/"))
}

/// Whether two drafts-root-relative folder paths name the same folder, on the
/// same case-folded terms as [`folder_contains`].
pub(super) fn same_folder(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

pub(super) fn name_taken(root: &fs::RootFs, dir: &Path, name: &str) -> bool {
    let needle = name.to_lowercase();
    root.list_dir(dir)
        .map(|entries| entries.iter().any(|e| e.name.to_lowercase() == needle))
        .unwrap_or(false)
}

/// Absolute on-disk path of a draft file, through the escape gate (DRS-FR-16).
pub(super) fn draft_file_path(root: &crate::fs::RootFs, id: &str, path: &str) -> Result<PathBuf, String> {
    draft_file_path_in(root, &draft_dir(root, id)?, path)
}

/// DRS-FR-16: refuse a path any component of which is a symlink.
///
/// `resolve_under` is deliberately **syntactic** — it refuses `..` and absolute
/// paths without touching the filesystem, and so cannot see a link. That is the
/// right division of labour for the primitive, but it leaves a real hole here: a
/// symlink sitting inside a draft's `files/` folder resolves to a path that is
/// textually within the draft and physically anywhere, so a read would return
/// another file's contents and a write would land on it.
///
/// Every component is checked rather than only the leaf, because a link one
/// level up redirects everything beneath it just as effectively. A component
/// that does not exist yet is not a link and is skipped, which is what lets a
/// creation resolve its own target.
pub(super) fn reject_symlinked_components(root: &fs::RootFs, files_dir: &Path, path: &str) -> Result<(), String> {
    let mut current = files_dir.to_path_buf();
    for segment in path.split('/') {
        current.push(segment);
        if let Ok(meta) = root.file_info(&current) {
            if meta.kind == fs::EntryKind::Symlink {
                return Err(format!("refusing to follow a symlink in this draft: {path}"));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Record I/O
// ---------------------------------------------------------------------------

