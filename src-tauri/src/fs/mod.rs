//! Filesystem access primitives — the shared foundation every other core
//! module sits on. See `specifications/core/FSA-filesystem-access.md`.
//!
//! This module is the internal Rust API described in the spec's "Contract
//! surface" section. Today it lives inside the `synthesis_lib` crate; per the
//! engineer handoff, when the workspace is later split into crates this module
//! migrates wholesale to `synthesis-core`. Accordingly, **no Tauri-specific
//! types leak into the internal API** — only `browse_for_folder`, which is a
//! `#[tauri::command]` defined separately in `lib.rs`, depends on Tauri.
//!
//! Module layout:
//! - `error::FsError` — the single typed error enum returned by every
//!   primitive (FSA-FR-03 / FSA-FR-06 / FSA-FR-10).
//! - `discover_project_root` (FSA-FR-02 / FSA-FR-03)
//! - byte / text / TOML read & atomic-write primitives (FSA-FR-04 / FSA-FR-05 / FSA-FR-06)
//! - `sha256_file` (FSA-FR-07)
//! - `ensure_gitignored` (FSA-FR-08)
//! - `app_data_dir` (FSA-FR-09)
//! - `resolve_under` — the path-escape helper backing FSA-FR-10.
//!
//! Concurrency: per the spec's NFR section, primitives are synchronous from
//! the caller's perspective; this module owns no concurrency. Atomicity is
//! delivered by temp-file + same-directory rename (POSIX & Windows atomic for
//! files in the same parent).
//!
//! ## FSA-FR-10 contract for future callers
//!
//! The primitives exposed here (`read_bytes`, `write_bytes_atomic`,
//! `read_text`, `read_toml`, etc.) take **absolute caller-owned paths** —
//! they do NOT accept `(root, relative)` pairs and therefore do not invoke
//! `resolve_under` themselves. The path-escape contract (FSA-FR-10) instead
//! applies to *future* convenience wrappers built on top of this module that
//! accept a `(root, relative)` shape (e.g., per-project artifact paths under
//! `<project>/.synthesis/...`). Such wrappers **MUST** call
//! `resolve_under(root, rel)` first and use only its returned absolute path
//! when delegating to the primitives. Skipping that gate would let a
//! crafted `rel` such as `"../../etc/passwd"` reach the primitives, which
//! by design do not re-validate. `resolve_under` is the single FSA-FR-10 gate.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub mod access;
mod access_build;
mod access_handles;
mod access_readonly;
mod access_state;
mod access_types;
mod bounded_read;
#[cfg(test)]
mod access_tests;
#[cfg(test)]
pub mod case_probe;
pub mod error;
pub mod author_tree;
pub mod owned_tree;
#[cfg(test)]
pub mod permission_probe;

pub use access::{
    CreateMode, DirEntry, EntryKind, FileHandle, FileInfo, FsAccess, FsAccessBuilder,
    FsAccessState, OpenMode, RootFs, UserChosenPath,
};
pub use error::{FsAccessBuildError, FsError, GrantRefusal};

/// Convenience alias used throughout this module and re-exported for
/// downstream callers.
pub type FsResult<T> = Result<T, FsError>;

// ---------------------------------------------------------------------------
// FSA-FR-02 / FSA-FR-03: project root discovery
// ---------------------------------------------------------------------------

/// Walks upward from `start_path` until a directory containing
/// `.synthesis/project.toml` is found. Returns that directory.
///
/// Walks inclusive of `start_path` itself: if `start_path` is the project
/// root, it is returned immediately.
///
/// FSA-FR-03: When the walk reaches the filesystem root without finding a project,
/// returns `Err(FsError::NotAProject)` — never an arbitrary fallback.
pub fn discover_project_root<P: AsRef<Path>>(start_path: P) -> FsResult<PathBuf> {
    let start = start_path.as_ref();
    // We walk over `Path` ancestors which yields `start` first, then each
    // parent, ending at the filesystem root. We do NOT canonicalize: the spec
    // is silent on symlinks and callers may legitimately pass a non-existent
    // deeper path (mirrors `git`'s behavior when run from a non-existent
    // subdir of a real repo — except git requires the dir to exist; we don't
    // need that, the existence check is per-candidate below).
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(".synthesis").join("project.toml");
        if candidate.is_file() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(FsError::NotAProject)
}

// ---------------------------------------------------------------------------
// FSA-FR-04 / FSA-FR-05: atomic writes; FSA-FR-06: typed read errors
// ---------------------------------------------------------------------------



/// The read mechanics alone, with no symlink check of their own.
///
/// `FsAccess` calls this after its own gate has run (FSA-FR-10 / FSA-FR-17 /
/// FSA-FR-27), so re-checking here would be redundant work and — under an
/// instance built with `follow_symlinks(true)` — actively wrong, since the gate
/// has already resolved and validated the link the caller named.
fn read_bytes_at(path: &Path) -> FsResult<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(FsError::NotFound {
            path: path.to_path_buf(),
        }),
        Err(e) => Err(FsError::Io(e)),
    }
}


/// The atomic-write mechanics alone, with no symlink check of their own — the
/// companion of `read_bytes_at`, called by `FsAccess` once its gate has run.
fn write_bytes_at(target: &Path, bytes: &[u8]) -> FsResult<()> {
    let parent = target
        .parent()
        .ok_or_else(|| FsError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("write target {target:?} has no parent directory"),
        )))?;

    // FSA-FR-05: create parent on demand. `create_dir_all` is a no-op if it
    // already exists, and is the cheapest correct option.
    if !parent.as_os_str().is_empty() && !parent.exists() {
        fs::create_dir_all(parent).map_err(FsError::Io)?;
    }

    // Build a temp file path in the same directory as the target. Using
    // process id + nanoseconds since UNIX_EPOCH is sufficiently unique for
    // our purposes (single process writes, brief contention window). If a
    // future caller needs hardened uniqueness we can swap in `tempfile`.
    let tmp = sibling_tmp_path(target);

    // Write the temp file. Scope the file handle so it's closed before the
    // rename (Windows requires the source file be closed for `rename` to
    // succeed when overwriting a target).
    {
        let mut f = fs::File::create(&tmp).map_err(FsError::Io)?;
        f.write_all(bytes).map_err(FsError::Io)?;
        // Flush + sync_data so the bytes are durably on disk before the
        // rename advertises them. Without this a crash between rename and
        // disk-flush could expose an empty file on some filesystems.
        f.flush().map_err(FsError::Io)?;
        // sync_data is fine here — we don't care about file metadata.
        let _ = f.sync_data();
    }

    // Atomic same-directory rename.
    if let Err(e) = fs::rename(&tmp, target) {
        // Best-effort cleanup so we don't leave orphaned temp files.
        let _ = fs::remove_file(&tmp);
        return Err(FsError::Io(e));
    }
    Ok(())
}

/// Build a sibling temp-file path for a target. The path lives in the same
/// parent directory as `target` so the subsequent rename is same-directory
/// (atomic on POSIX & Windows).
fn sibling_tmp_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let stem = target
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "synthesis".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    parent.join(format!(".{stem}.tmp.{pid}.{nanos}"))
}


/// The append mechanics alone, with no symlink check of their own — called by
/// `FsAccess::append_lines` once its gate has run.
fn append_lines_at(target: &Path, lines: &[String]) -> FsResult<()> {
    if lines.is_empty() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(FsError::Io)?;
        }
    }
    let mut buf = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    for line in lines {
        buf.push_str(line);
        buf.push('\n');
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(target)
        .map_err(FsError::Io)?;
    file.write_all(buf.as_bytes()).map_err(FsError::Io)?;
    file.flush().map_err(FsError::Io)?;
    // FSA-FR-15: the append answers only once the bytes are on the device. A
    // caller that is told a line was appended may rely on it surviving an
    // immediate loss of the process, so a sync that fails is a write that
    // failed rather than something to pass over.
    file.sync_data().map_err(FsError::Io)?;
    Ok(())
}


/// `read_text` without a symlink check of its own — called by
/// `FsAccess::read_text` once its gate has run.
fn read_text_at(path: &Path) -> FsResult<String> {
    let bytes = read_bytes_at(path)?;
    String::from_utf8(bytes).map_err(|e| FsError::Utf8 {
        path: path.to_path_buf(),
        source: e.utf8_error(),
    })
}




/// `read_toml` without a symlink check of its own — called by
/// `FsAccess::read_toml` once its gate has run.
fn read_toml_at<T: DeserializeOwned>(path: &Path) -> FsResult<T> {
    let text = read_text_at(path)?;
    toml::from_str(&text).map_err(|e| FsError::Toml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// `write_toml_atomic` without a symlink check of its own — called by
/// `FsAccess::write_toml_atomic` once its gate has run.
fn write_toml_at<T: Serialize>(path: &Path, value: &T) -> FsResult<()> {
    let s = toml::to_string(value).map_err(|e| FsError::Toml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    write_bytes_at(path, s.as_bytes())
}

// ---------------------------------------------------------------------------
// FSA-FR-07: streaming SHA-256 checksum
// ---------------------------------------------------------------------------


/// `sha256_file` without a symlink check of its own — called by
/// `FsAccess::sha256_file` once its gate has run.
fn sha256_file_at(path: &Path) -> FsResult<String> {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(FsError::NotFound {
                path: path.to_path_buf(),
            });
        }
        Err(e) => return Err(FsError::Io(e)),
    };

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(FsError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(hex_lower(&digest))
}

/// SHA-256 of an in-memory byte slice as lowercase hex. Companion to
/// `sha256_file` and byte-identical to it for the same content. Used where the
/// bytes are already in memory (e.g. a body about to be written atomically) so
/// the checksum can be recorded BEFORE the write becomes visible to the
/// filesystem watcher — closing the self-write race (PST-FR-16).
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// FSA-FR-08: gitignore bookkeeping
// ---------------------------------------------------------------------------

/// Required entries that `<synthesis_dir>/.gitignore` must always contain.
/// Order matters only for the "append in order" deterministic property
/// exercised by tests.
/// FSA-FR-08: what `.synthesis/.gitignore` must exclude **at minimum**.
///
/// `/proposals/` is here for `PCP-prompt-change-proposals.md` PCP-FR-02: a
/// proposed rewrite of a published file is private working material about that
/// file, and it must never be committed or reach anyone who clones the
/// repository.
///
/// Every entry is **anchored** with a leading slash, so each names one entry of
/// `.synthesis/` and nothing deeper. A pattern with no interior slash matches at
/// every depth below the file it is written in, so the unanchored `proposals/`
/// this once carried also excluded `.synthesis/drafts/<id>/proposals/` — a
/// folder that is committed draft-owned storage (per `DRS-draft-storage.md`
/// DRS-FR-04, DRS-FR-ZIVL, `DCP-draft-change-proposals.md` DCP-FR-02). An
/// ignored path is invisible to a working-tree status, so those proposals would
/// have travelled to no other checkout and nothing would have reported it.
///
/// `drafts/` is deliberately **absent**: draft storage is committed so a draft
/// and the whole collaboration around it reach every checkout through Git
/// (DRS-FR-04).
const REQUIRED_GITIGNORE_ENTRIES: &[&str] = &["/cache/", "/local.toml", "/proposals/"];

/// FSA-FR-08: the unanchored spellings of the required entries, which an
/// existing project's ignore file carries and which each entry above replaces.
///
/// They are the application's own lines rather than the author's, and each
/// excludes more than the entry that supersedes it: `cache/` also excludes a
/// drafts folder an author named `cache`, and `proposals/` excludes every
/// draft's own `proposals/`. Removing them is part of the same migration the
/// `drafts/` entry gets (per `DRS-draft-storage.md` DRS-FR-ISPI).
const SUPERSEDED_GITIGNORE_ENTRIES: &[&str] = &[
    "cache/",
    "cache",
    "local.toml",
    "proposals/",
    "proposals",
];

/// FSA-FR-08: the entries that ignore `.synthesis/drafts/`, removed wherever an
/// older project's ignore file still carries one (per `DRS-draft-storage.md`
/// DRS-FR-ISPI).
///
/// These spellings all say the same thing to Git, and a project scaffolded by an
/// earlier version of this application carries the first. A negation
/// (`!drafts/`) ignores nothing and is the author's own line, so it is left
/// exactly where it is.
const DRAFT_IGNORE_ENTRIES: &[&str] = &[
    "drafts/",
    "drafts",
    "/drafts/",
    "/drafts",
    "drafts/*",
    "drafts/**",
    "/drafts/*",
    "/drafts/**",
    "**/drafts/",
    "**/drafts",
];

/// Whether one line of an ignore file is one this module removes: an entry that
/// excludes the drafts folder, or an unanchored spelling of a required entry.
///
/// Only the **right** side is trimmed. Git ignores trailing whitespace on a
/// pattern but not leading whitespace, so `  drafts/` names a folder called
/// `  drafts` — a line of the author's rather than one of these, and it stays
/// exactly where it is. A trailing `\r` goes with the rest of the trailing
/// whitespace, so a CRLF file's entry is recognised as an LF file's is.
fn is_superseded_entry(line: &str) -> bool {
    let entry = line.trim_end();
    DRAFT_IGNORE_ENTRIES.contains(&entry) || SUPERSEDED_GITIGNORE_ENTRIES.contains(&entry)
}

/// FSA-FR-08: the ignore file without the entries this module supersedes.
///
/// Every other line is returned **byte-for-byte**, in the order it was in and
/// under the line ending it carried, so a migration touches the author's own
/// entries and comments at no point (per `DRS-draft-storage.md` DRS-FR-ISPI). A
/// CRLF file stays CRLF and a file with no trailing newline gains none, which a
/// rebuild from `lines()` would silently undo. A file holding no such entry is
/// returned unchanged, which is what keeps the ensure idempotent.
fn remove_superseded_entries(existing: &str) -> String {
    if !existing.lines().any(is_superseded_entry) {
        return existing.to_string();
    }
    let mut out = String::with_capacity(existing.len());
    let mut rest = existing;
    while !rest.is_empty() {
        // The line together with the terminator it carried, so removing a line
        // removes exactly its own bytes.
        let (line, remainder) = match rest.find('\n') {
            Some(end) => (&rest[..=end], &rest[end + 1..]),
            None => (rest, ""),
        };
        if !is_superseded_entry(line) {
            out.push_str(line);
        }
        rest = remainder;
    }
    out
}


/// Pure helper backing `ensure_gitignored`. Given existing gitignore content
/// and a list of required entries, returns the merged content: existing
/// content unchanged, with any missing entries appended (one per line, in
/// the order they appear in `required`).
///
/// The "missing" check is line-equality after trimming surrounding
/// whitespace and ignoring comment lines. Negation patterns (`!cache/`) or
/// scoped patterns (`/cache/` vs `cache/`) are NOT considered equivalent;
/// the spec talks about literal entries.
pub fn merge_gitignore(existing: &str, required: &[&str]) -> String {
    // FSA-FR-08: the entries this module supersedes go first, so what is left
    // is what the required entries are measured against and the author's own
    // lines keep their order and their bytes.
    let migrated = remove_superseded_entries(existing);
    let existing = migrated.as_str();

    let present: std::collections::HashSet<&str> = existing
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|e| !present.contains(*e))
        .collect();

    if missing.is_empty() {
        return existing.to_string();
    }

    let mut out = String::with_capacity(existing.len() + missing.iter().map(|e| e.len() + 1).sum::<usize>());
    out.push_str(existing);
    // Make sure we're starting on a fresh line before appending.
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for entry in missing {
        out.push_str(entry);
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// FSA-FR-09: platform-correct user data root
// ---------------------------------------------------------------------------

/// Returns the platform-correct user-scoped data root for Synthesis, creating
/// it if it does not exist.
///
/// On macOS: `~/Library/Application Support/synthesis`
/// On Linux: `$XDG_DATA_HOME/synthesis` (typically `~/.local/share/synthesis`)
/// On Windows: `%APPDATA%/synthesis` (Roaming)
///
/// Uses the `dirs` crate so the function is pure-Rust and testable without an
/// `AppHandle`.
pub fn app_data_dir() -> FsResult<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| {
        FsError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "platform user data directory could not be determined",
        ))
    })?;
    let dir = base.join("synthesis");
    // `create_dir_all` is idempotent — no need to gate on `dir.exists()`.
    fs::create_dir_all(&dir).map_err(FsError::Io)?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// FSA-FR-KVWD: the short user-scoped root
// ---------------------------------------------------------------------------

/// The OS user's home directory joined with `.synthesis`, creating it if it is
/// not there (FSA-FR-KVWD).
///
/// A second user-scoped root beside [`app_data_dir`], and it exists for one
/// reason: what stands under it is read as a **working directory by an agent in
/// a container** as well as by this application. A graduation run's store is
/// here (`GSU-graduation-start.md` GSU-FR-RJRF), the run's checkout is the
/// directory an execution turn stands in, and the container names the same path
/// the host names (`EAC-execute-agent-cli.md` EAC-FR-ZKMR) — so every segment
/// above that checkout is a segment the agent reads on each command it runs.
/// `app_data_dir()` is the wrong root for that: on macOS it is
/// `~/Library/Application Support/synthesis`, which is long and holds a space.
pub fn short_data_dir() -> FsResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        FsError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "user home directory could not be determined",
        ))
    })?;
    short_data_dir_in(&home)
}

/// The same root, under a home directory the caller names.
///
/// Split out because the root is created on first call, and a test that wants
/// to see that happen cannot delete the developer's own — it holds every
/// graduation run this machine has.
pub(crate) fn short_data_dir_in(home: &Path) -> FsResult<PathBuf> {
    let dir = home.join(".synthesis");
    // `create_dir_all` is idempotent, exactly as in `app_data_dir`.
    fs::create_dir_all(&dir).map_err(FsError::Io)?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// FSA-FR-10: path-escape rejection
// ---------------------------------------------------------------------------

/// Resolve `rel` against `root`, rejecting any input that escapes `root`
/// (FSA-FR-10).
///
/// This is the single shared helper that backs the FSA-FR-10 contract. The
/// primitives in this module take absolute paths owned by their caller and do
/// not invoke this directly; FSA-FR-10 instead applies to *future* convenience
/// wrappers that accept a (root, relative-path) pair (e.g., per-project
/// artifact paths). Those wrappers MUST call this helper first.
///
/// Semantics:
/// - If `rel` is absolute, it must canonically be a descendant of `root`
///   (after resolving `..` components syntactically). Otherwise:
///   `FsError::PathEscape`.
/// - If `rel` is relative, syntactic `..` segments are resolved against the
///   leading components of `rel`; any unmatched `..` (which would walk above
///   the joined root) is `FsError::PathEscape`.
///
/// We resolve syntactically (no filesystem canonicalization) for two
/// reasons:
/// 1. The primitives must work for paths that do not yet exist (creating
///    files in subdirs is a common case — see FSA-FR-05).
/// 2. Following symlinks would conflate "string-level path escape" with
///    "symlink target resolution"; the spec is talking about the former.
pub fn resolve_under<R: AsRef<Path>, P: AsRef<Path>>(root: R, rel: P) -> FsResult<PathBuf> {
    let root = root.as_ref();
    let rel = rel.as_ref();

    // Build the candidate as `root.join(rel)` and then resolve syntactic
    // components, tracking that we never "pop" above the root.
    let combined = if rel.is_absolute() {
        rel.to_path_buf()
    } else {
        root.join(rel)
    };

    // Normalize the root to component form so we can compare prefixes.
    let root_components: Vec<Component> = root.components().collect();

    // Resolve `combined` syntactically: walk components, push normals,
    // handle `..` by popping the last component (but never above the root's
    // own depth).
    let mut resolved: Vec<Component> = Vec::with_capacity(root_components.len() + 8);
    let mut depth_below_root: i32 = 0;
    let mut at_root = true;

    // Seed `resolved` with the root's components and treat that as the
    // floor. We then process `combined`'s tail components beyond the root
    // if it's a descendant; or process every component if it's absolute and
    // different from the root.
    //
    // Simpler: walk `combined`'s components, treating the cumulative result
    // as starting empty for absolute paths and starting at `root` for
    // relative paths.
    let walk: Box<dyn Iterator<Item = Component>> = if rel.is_absolute() {
        Box::new(combined.components())
    } else {
        Box::new(root.components().chain(rel.components()))
    };

    for c in walk {
        match c {
            Component::Prefix(_) | Component::RootDir => {
                if at_root {
                    resolved.push(c);
                } else {
                    // A `RootDir` in the middle of a relative tail would be
                    // bizarre but harmless to skip.
                }
            }
            Component::CurDir => {
                // skip
            }
            Component::ParentDir => {
                if depth_below_root <= 0 {
                    return Err(FsError::PathEscape {
                        root: root.to_path_buf(),
                        rel: rel.to_path_buf(),
                    });
                }
                resolved.pop();
                depth_below_root -= 1;
            }
            Component::Normal(_) => {
                resolved.push(c);
                if !at_root {
                    depth_below_root += 1;
                }
            }
        }
        // We're "at root" until we finish placing the root's own components.
        if at_root {
            // Have we placed all of the root's components yet?
            if resolved.len() >= root_components.len() {
                at_root = false;
            }
        }
    }

    // Sanity check: the resolved path must start with the root's components.
    let resolved_path: PathBuf = resolved.iter().collect();
    if !resolved_path.starts_with(root) {
        return Err(FsError::PathEscape {
            root: root.to_path_buf(),
            rel: rel.to_path_buf(),
        });
    }
    Ok(resolved_path)
}

// ---------------------------------------------------------------------------
// FSA-FR-11 / FSA-FR-12 / FSA-FR-13: structural mutations (delete / rename / copy)
//
// Unlike the read/write primitives above (which take absolute caller-owned
// paths), these mutators take a `(root, rel)` shape and are therefore the
// `(root, relative)` wrappers the module doc describes: each routes its inputs
// through `resolve_under` FIRST, so the FSA-FR-10 path-escape gate applies to
// every one of them (FSA-FR-10). They never overwrite an existing destination
// and are not transactional (a large recursive delete/copy that fails partway
// may leave a partial tree) — see the spec's non-functional notes.
// ---------------------------------------------------------------------------

/// FSA-FR-17: refuse any path that is, or reaches through, a symbolic link.
///
/// Only the components *below* `root` are examined. Components at or above the
/// root are the caller's own business and are routinely links on a real system
/// — `/tmp` is a symlink to `/private/tmp` on macOS, and a user's home may be
/// one too — so walking above the root would refuse ordinary, correct paths.
/// Below the root is exactly the region a caller composes from untrusted input,
/// which is the region FSA-FR-10 already governs; this is the same boundary.
///
/// The check never resolves a link to see where it points: the refusal is about
/// the link, not its target, which is what makes it total. A link into the root
/// is refused exactly as a link out of it is, and a link whose target moves
/// later cannot turn a previously-accepted path into an escape.
fn ensure_no_symlink_under(root: &Path, target: &Path) -> FsResult<()> {
    let tail = match target.strip_prefix(root) {
        Ok(t) => t,
        // Not under the root at all — `resolve_under` is the gate for that, and
        // it has already run by the time we get here.
        Err(_) => return Ok(()),
    };
    let mut probe = root.to_path_buf();
    for component in tail.components() {
        probe.push(component);
        match fs::symlink_metadata(&probe) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(FsError::SymlinkRefused { path: probe });
            }
            // A component that does not exist yet cannot be a link, and the
            // primitives that create it will error on their own terms.
            Ok(_) | Err(_) => {}
        }
    }
    Ok(())
}

/// PST-FR-26's containment probe: does `rel` resolve inside `root` without
/// leaving it and without reaching through a symbolic link?
///
/// Returns `Ok(())` when the path is safe to operate on, or `Err(reason)` with a
/// short machine-ish reason (`"escape"` / `"symlink"`) suitable for a log field.
/// Performs no mutation and reads no file content — it exists so a caller can
/// refuse *before* invoking any primitive, which is what makes the refusal
/// all-or-nothing for multi-step operations.
pub fn resolve_inside<R: AsRef<Path>, P: AsRef<Path>>(
    root: R,
    rel: P,
) -> Result<(), &'static str> {
    let target = resolve_under(&root, &rel).map_err(|_| "escape")?;
    ensure_no_symlink_under(root.as_ref(), &target).map_err(|_| "symlink")?;
    Ok(())
}




/// Reject a `rename_path` `new_name` that is not a bare basename: empty, a `.`
/// or `..` component, or containing any path separator. Pure so the validation
/// is unit-testable on its own (FSA-FR-12).
fn is_valid_basename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
}



/// Copy a single file from `src` to `dst`, failing with `FsError::AlreadyExists`
/// rather than overwriting if `dst` springs into existence after `copy_path`'s
/// pre-check (the TOCTOU window). `create_new` maps to `O_EXCL`, so the kernel —
/// not a racy userspace check — enforces "must not exist".
fn copy_file_no_overwrite(src: &Path, dst: &Path) -> FsResult<()> {
    let mut reader = fs::File::open(src).map_err(FsError::Io)?;
    let mut writer = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)
    {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(FsError::AlreadyExists {
                path: dst.to_path_buf(),
            });
        }
        Err(e) => return Err(FsError::Io(e)),
    };
    io::copy(&mut reader, &mut writer).map(|_| ()).map_err(FsError::Io)
}






// ---------------------------------------------------------------------------
// FSA-FR-01: dialog plugin return-shape helper (Tauri-touching, pure)
// ---------------------------------------------------------------------------

/// Public return shape for the folder picker, used by the Tauri command
/// `browse_for_folder` in `lib.rs`. Pulled out into the fs module so the
/// dialog -> public mapping is independently unit-testable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowseResult {
    /// User picked a folder. The path is absolute.
    Selected { path: String },
    /// User dismissed/cancelled the dialog. FSA-FR-01: this is never an error.
    Cancelled,
}

/// Map the dialog plugin's `Option<PathBuf>` return into the public
/// `BrowseResult` shape. Stays pure so the FSA-FR-01 cancel/select semantics are
/// covered by a unit test without invoking a real dialog.
pub fn map_dialog_result(maybe_path: Option<PathBuf>) -> BrowseResult {
    match maybe_path {
        Some(p) => BrowseResult::Selected {
            path: p.to_string_lossy().into_owned(),
        },
        None => BrowseResult::Cancelled,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
