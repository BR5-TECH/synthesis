//! Reading and writing a draft's record, and the listing and search the
//! Drafts panel is rendered from (`DRS-draft-storage.md` DRS-FR-08,
//! DRS-FR-17).

use super::*;

/// A draft's record, for a reader outside this module.
///
/// `crate::agent_conversations`' `draft_comment` context builder (AGC-FR-07)
/// names the draft to the agent it is about to ask, and the name lives here.
pub fn read_draft_record(root: &crate::fs::RootFs, id: &str) -> Result<DraftRecord, String> {
    read_record(root, id)
}

pub(super) fn read_record(root: &fs::RootFs, id: &str) -> Result<DraftRecord, String> {
    read_record_at(root, &draft_dir(root, id)?, id)
}

/// [`read_record`] for a caller that already knows where the draft's directory
/// sits — the drafts-root walk, which would otherwise pay for a second walk per
/// draft it just found.
pub(super) fn read_record_at(root: &fs::RootFs, dir: &Path, id: &str) -> Result<DraftRecord, String> {
    let path = dir.join(RECORD_FILE);
    let mut record =
        root.read_toml::<DraftRecord>(&path).map_err(|_| format!("draft not found: {id}"))?;
    // DRS-FR-16: `prompt_path` is the one path in this module that comes from a
    // file rather than from a caller, and `draft.toml` is an ordinary file an
    // author can edit. A pointer that is not a draft-relative path names no file
    // this module will act on, so the pointer is simply absent — which reads as
    // an inconsistent draft (DRS-FR-15), reached without ever stat-ing whatever
    // it pointed at.
    if record
        .prompt_path
        .as_deref()
        .is_some_and(|p| !is_valid_draft_path(p))
    {
        record.prompt_path = None;
    }
    Ok(record)
}

/// Write a draft's record into the directory the caller already resolved.
///
/// There is no id-taking counterpart on purpose: resolving a draft id means
/// walking the drafts root (DRS-FR-29), and every caller here has already done
/// that walk to read the record it is about to write back. One that took an id
/// would quietly double the cost of every mutation.
pub(super) fn write_record_at(root: &fs::RootFs, dir: &Path, record: &DraftRecord) -> Result<(), String> {
    root.write_toml_atomic(dir.join(RECORD_FILE), record)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Monotonic within the process, so two drafts created in the same millisecond
/// still get distinct ids.
pub(super) static ID_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(super) fn new_draft_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let counter = ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(millis);
    hasher.write_u32(counter);
    let entropy = hasher.finish() as u32;
    format!("{millis:011x}-{counter:04x}-{entropy:08x}")
}

// ---------------------------------------------------------------------------
// The single-prompt invariant (DRS-FR-11, DRS-FR-15)
// ---------------------------------------------------------------------------

/// DRS-FR-11: the draft's one prompt, or `None` where its `files/` is not the
/// single prompt the invariant requires.
///
/// The check is against the disk rather than against the record alone, because
/// the record is what this module writes and `files/` is what anything at all
/// could have put a second file into. Three things must hold together: the
/// record names a prompt, that name is a bare basename rather than a nested
/// path, and `files/` holds exactly that one entry and nothing else — no second
/// file, no folder, and no symlink standing in for either.
///
/// Entries whose name begins with a dot are skipped, and only those: they are
/// what the operating system drops in beside an author's files, and reading a
/// `.DS_Store` as a second document would make a draft unopenable for a reason
/// the author neither caused nor can see. Everything they did put there counts.
/// DRS-FR-41: the modification time of the draft's one prompt, RFC 3339 UTC.
///
/// One `file_info` of a name the caller already resolved — the prompt's bytes
/// are never read, which is what keeps `list_drafts` proportional to the walk
/// rather than to what the drafts hold. `None` where the filesystem reports no
/// modification time for it, which is the same answer an inconsistent draft
/// gives and needs no separate handling by any caller.
pub(super) fn prompt_activity_at(root: &fs::RootFs, dir: &Path, prompt: &str) -> Option<String> {
    root.file_info(dir.join(FILES_DIR).join(prompt))
        .ok()
        .and_then(|info| info.modified)
        .map(crate::notes::format_rfc3339_millis_from)
}

pub(super) fn prompt_of(root: &fs::RootFs, dir: &Path, record: &DraftRecord) -> Option<String> {
    let prompt = record.prompt_path.as_deref()?;
    if prompt.is_empty() || prompt.contains('/') || prompt.contains('\\') {
        return None;
    }
    let entries = root.list_dir(dir.join(FILES_DIR)).ok()?;
    let mut held: Option<String> = None;
    for entry in entries {
        if entry.name.starts_with('.') {
            continue;
        }
        // A folder, or a symlink pointing at one, is exactly what DRS-FR-13
        // forbids anyone to create — so finding one means something outside
        // this application made it, and the draft is reported rather than
        // reduced to whichever entry happens to look most like a prompt.
        if entry.kind != fs::EntryKind::File {
            return None;
        }
        if held.is_some() {
            return None;
        }
        held = Some(entry.name);
    }
    held.filter(|name| name == prompt)
}

/// [`prompt_of`] for a caller that holds the draft's directory and wants the
/// typed refusal rather than an absence (DRS-FR-15).
pub(super) fn require_prompt_at(root: &fs::RootFs, dir: &Path, id: &str) -> Result<String, String> {
    let record = read_record_at(root, dir, id)?;
    prompt_of(root, dir, &record).ok_or_else(|| ERR_NOT_SINGLE_FILE.to_string())
}

/// DRS-FR-11 / DRS-FR-15: the draft's one prompt, for a caller outside this
/// module that has to uphold the same invariant before it acts.
///
/// `crate::draft_proposals` (DCP-FR-22) and `crate::draft_history` (DHS-FR-25)
/// both refuse an inconsistent draft with the same typed error, and both would
/// otherwise have to re-derive what "the prompt" means from the record — which
/// is the one thing DRS-FR-36 exists to stop a caller doing.
pub fn require_prompt(root: &fs::RootFs, id: &str) -> Result<String, String> {
    let dir = draft_dir(root, id)?;
    require_prompt_at(root, &dir, id)
}

// ---------------------------------------------------------------------------
// Implementations (pure over a root, so they unit-test without a Tauri runtime)
// ---------------------------------------------------------------------------

/// DRS-FR-08: the active worktree's whole hierarchy, in one walk.
///
/// Every folder at every depth — empty ones included, because a folder is the
/// author's structure rather than a consequence of what is in it — and one
/// summary per draft, most-recent activity first. No draft file's contents are
/// read, so a worktree holding many large drafts costs the list nothing beyond
/// the walk.
pub fn list_drafts_impl(root: &crate::fs::RootFs) -> DraftHierarchy {
    hierarchy_from(root, scan_drafts_root(root))
}

/// [`list_drafts_impl`] preceded by the repair of DRS-FR-37, and the drafts it
/// reunited with their storage.
///
/// The repair hangs off the **Tauri command** rather than off `list_drafts_impl`,
/// and the distinction is load-bearing rather than tidiness. `list_drafts_impl`
/// is a read that half this module runs — the BM25 pass on its own thread
/// (`BMI-FR-19`), `search_drafts_impl`, `default_draft_name`, and
/// `move_draft_to_folder_impl` all call it — and a read that moves and deletes
/// files is one those callers never asked for. Hanging it here means a repair
/// happens once, on the author's own list, on the thread that asked: no
/// background thread races the panel to merge the same log, and the one place
/// the tree is rendered from is the one place it is put right.
pub(super) fn list_drafts_reporting(root: &crate::fs::RootFs) -> (DraftHierarchy, Vec<String>) {
    let scan = scan_drafts_root(root);
    let repaired = reconcile_misplaced_storage(root, &scan.0, &scan.1);
    // The husks and their subfolders were in that first scan as folders of the
    // author's. Re-walking is what makes the returned hierarchy the one on disk
    // rather than the one it was read from (DRP-FR-32) — and it runs whenever
    // the repair *moved* anything, not only where it went on to remove the husk,
    // because a husk left standing has still lost the children it gave up.
    let scan = if repaired.is_empty() { scan } else { scan_drafts_root(root) };
    (hierarchy_from(root, scan), repaired)
}

pub(super) fn hierarchy_from(
    root: &crate::fs::RootFs,
    (folders, found): (Vec<DraftFolder>, Vec<DraftAt>),
) -> DraftHierarchy {
    let mut drafts: Vec<DraftSummary> = Vec::with_capacity(found.len());
    for at in found {
        // DRS-FR-35: a draft whose record cannot be read is reported with what
        // could be recovered — its id, which is its directory's name — rather
        // than dropped. A draft that silently vanishes from the panel reads as
        // data loss, and the folder is neither moved nor deleted either way.
        let record = read_record_at(root, &at.dir, &at.id).unwrap_or_else(|_| DraftRecord {
            id: at.id.clone(),
            name: at.id.clone(),
            prompt_path: None,
            status: DraftStatus::Active,
            created_at: String::new(),
            updated_at: String::new(),
            github_issue: None,
        });
        // DRS-FR-15: reported, never repaired. The row is returned with every
        // field that could be recovered, so an inconsistent draft is visible and
        // deletable rather than missing.
        let prompt = prompt_of(root, &at.dir, &record);
        let inconsistent = prompt.is_none();
        // DRS-FR-08 / DRS-FR-41: one stat of the prompt the enumeration above
        // already found. No file's contents are read, so the walk still costs a
        // directory read per draft.
        let prompt_activity_at =
            prompt.and_then(|name| prompt_activity_at(root, &at.dir, &name));
        drafts.push(DraftSummary {
            id: record.id,
            name: record.name,
            status: record.status,
            folder: at.folder,
            inconsistent,
            updated_at: record.updated_at,
            prompt_activity_at,
            build: BuildState::Idle,
            has_pending_proposal: crate::draft_proposals::has_pending_at(root, &at.dir),
            // Filled in by `list_drafts` from one queue read (DRS-FR-18); the
            // walk itself knows nothing about graduation.
            graduation: None,
            // DRS-FR-XDWS: the list reports the link of a shadow draft.
            github_issue: record.github_issue,
        });
    }
    // DRP-FR-21: most-recent-activity first, tie-broken by id so the order is
    // total and a reload never reshuffles two drafts touched in the same second.
    drafts.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
    DraftHierarchy { folders, drafts }
}

/// DRS-FR-17: the drafts the text is found in — matched against each draft's
/// name and, failing that, against the text of its prompt.
///
/// The name is checked first and short-circuits the read, so the common case
/// (typing a name you remember) opens no file at all. Only a query that the name
/// does not answer pays for reading the prompt, which is what keeps the panel's
/// one expensive call proportional to what was actually asked. An inconsistent
/// draft's storage is not read: nothing there is the prompt (DRS-FR-15).
pub fn search_drafts_impl(root: &crate::fs::RootFs, text: &str) -> Vec<DraftMatch> {
    let needle = text.trim().to_lowercase();
    let mut out: Vec<DraftMatch> = Vec::new();
    for summary in list_drafts_impl(root).drafts {
        // An empty query is not a filter: every draft matches, on its name,
        // which is what leaves the unfiltered list exactly as `list_drafts` gave
        // it and annotates no row (DRP-FR-17).
        if needle.is_empty() || summary.name.to_lowercase().contains(&needle) {
            out.push(DraftMatch {
                draft_id: summary.id,
                matched_in: DraftMatchedIn::Name,
            });
            continue;
        }
        if summary.inconsistent {
            continue;
        }
        let Ok(dir) = draft_dir_in(root, &summary.folder, &summary.id) else {
            continue;
        };
        if contains_text(root, &dir.join(FILES_DIR), &needle) {
            out.push(DraftMatch {
                draft_id: summary.id,
                matched_in: DraftMatchedIn::Contents,
            });
        }
    }
    out
}

/// Whether any file beneath `dir` holds `needle`, compared case-insensitively.
///
/// A file that does not decode as UTF-8 contributes no match (DRS-FR-17): it is
/// not text the author wrote, and lower-casing arbitrary bytes to search them
/// would be meaningless. Oversized files are skipped for the same reason the
/// panel is not a project search — this runs per query, over every draft.
pub(super) fn contains_text(root: &fs::RootFs, dir: &Path, needle: &str) -> bool {
    let Ok(entries) = root.list_dir(dir) else {
        return false;
    };
    for entry in entries {
        let name = entry.name.clone();
        if name.starts_with('.') {
            continue;
        }
        // DRS-FR-16: a symlink is skipped rather than followed. Otherwise a link
        // to a home directory would have this read it on every settled query,
        // and a link to an ancestor would recurse without end.
        if entry.kind == fs::EntryKind::Symlink {
            continue;
        }
        let entry_path = dir.join(&entry.name);
        if entry.kind == fs::EntryKind::Dir {
            if contains_text(root, &entry_path, needle) {
                return true;
            }
        } else if entry.kind == fs::EntryKind::File
            && root
                .file_info(&entry_path)
                .map(|i| i.size)
                .unwrap_or(u64::MAX)
                <= SEARCH_MAX_BYTES
            && root
                .read_bytes(&entry_path)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .is_some_and(|body| body.to_lowercase().contains(needle))
        {
            return true;
        }
    }
    false
}

/// DRS-FR-27: `Untitled`, or the first `Untitled N` no draft in this worktree
/// already bears.
///
/// Only the auto-assigned default is disambiguated — a name the caller supplies
/// is stored verbatim however many drafts already carry it (DRS-FR-09), because
/// a draft is identified by its id and two drafts about the same thing are
/// entitled to say so. What this avoids is the other case: a worktree that
/// accumulates unnamed drafts listing them as a column of identical rows.
pub(super) fn default_draft_name(root: &crate::fs::RootFs) -> String {
    let taken: Vec<String> = list_drafts_impl(root).drafts.into_iter().map(|d| d.name).collect();
    if !taken.iter().any(|n| n == DEFAULT_NAME) {
        return DEFAULT_NAME.to_string();
    }
    for n in 2..=u32::MAX {
        let candidate = format!("{DEFAULT_NAME} {n}");
        if !taken.iter().any(|t| *t == candidate) {
            return candidate;
        }
    }
    DEFAULT_NAME.to_string()
}

