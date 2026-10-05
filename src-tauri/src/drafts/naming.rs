//! Validation of the names and paths a draft operation accepts, and the
//! renames they turn into (`DRS-draft-storage.md` DRS-FR-25, DRS-FR-26,
//! DRS-FR-30).

use super::*;

// ---------------------------------------------------------------------------
// Validation (pure)
// ---------------------------------------------------------------------------

/// Reject a draft id that is not a bare, filename-safe token before it is
/// joined into a path. `resolve_under` refuses to escape the root, but a
/// separator or a dot segment would still name a directory elsewhere *inside*
/// `.synthesis/`, so it is rejected outright.
pub fn is_valid_draft_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A single path segment the author supplies for a new draft file or folder
/// (NAW-FR-08): non-empty, free of separators, and not a dot segment — so one
/// creation makes exactly one entry rather than a chain of nested ones.
pub fn is_valid_segment(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed == name
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

/// A draft-relative path naming a file or folder beneath `files/`: one or more
/// valid segments joined by `/`. Every segment is checked, so `a/../b` and a
/// leading `/` are both refused before `resolve_under` ever sees them.
pub fn is_valid_draft_path(path: &str) -> bool {
    !path.is_empty() && path.split('/').all(is_valid_segment)
}

/// DRS-FR-30: a name a **drafts folder** may carry — one of the author's own
/// organising directories under `.synthesis/drafts/`.
///
/// Stricter than [`is_valid_segment`] on one axis: the name becomes a directory
/// on disk, and the illegal set checked here is Windows's rather than this
/// platform's. A drafts folder is not committed today, but the author's
/// organisation is a shape they will read in a file manager and may well carry
/// to another machine, and a name only APFS tolerates is one that arrives there
/// broken. The sibling-collision rule is not here: it is about the destination
/// rather than the name, so it lives where the destination is known.
pub fn is_valid_folder_name(name: &str) -> bool {
    is_valid_segment(name)
        && name.len() <= 255
        && !name
            .chars()
            .any(|c| matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') || (c as u32) < 0x20)
        // Windows silently drops a trailing dot or space from a directory name,
        // so a folder created under one would come back under another and the
        // path the author was handed would stop resolving.
        && !name.ends_with('.')
        && !name.ends_with(' ')
}

/// A drafts-root-relative folder path: `""` for the implicit root, or one or
/// more valid folder names joined by `/` (DRS-FR-34).
pub fn is_valid_folder_path(path: &str) -> bool {
    path.is_empty() || path.split('/').all(is_valid_folder_name)
}

/// The containing folder of a drafts-root-relative path, `""` when it is
/// top-level.
pub(super) fn folder_parent(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent,
        None => "",
    }
}

/// Join a drafts-root-relative folder path with a name below it, treating `""`
/// as the root rather than producing a leading separator.
pub(super) fn folder_join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// DRS-FR-25: the name of the file a draft's name derives to — the name with
/// every path separator and every character illegal in a filename replaced by
/// `-`, followed by `.md`.
///
/// The derivation is deliberately **one-way**. The stored name stays the free
/// text the author typed, so `UI: pass 2/final` is held verbatim on the record
/// while the file on disk is `UI- pass 2-final.md`; the two legitimately read
/// differently, and trying to keep them byte-identical would mean refusing
/// names a draft is entitled to carry.
///
/// The illegal set is Windows's rather than this platform's, because a draft is
/// written on one machine and its files are graduated into a repository read on
/// others — a name that only macOS tolerates would produce a file no one else
/// can check out.
pub fn prompt_file_name(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if (c as u32) < 0x20 => '-',
            c => c,
        })
        .collect();
    // A leading dot is legal on every filesystem and useless here: the file rail
    // skips dot-entries (they are what the OS drops in beside an author's
    // files), so a draft called `.claude notes` would hold a primary file the
    // rail never shows, the empty state in front of it, and no way to publish it
    // at graduation. It joins the substitutions above rather than being refused,
    // because the name itself is the author's to choose.
    let stem = match stem.strip_prefix('.') {
        Some(rest) => format!("-{rest}"),
        None => stem,
    };
    format!("{stem}.{PROMPT_EXT}")
}

/// DRS-FR-25, the reverse direction: the draft name a primary file's own name
/// derives to — the filename without its extension, taken verbatim.
///
/// A name with no extension, and one that is nothing but an extension
/// (`.gitignore`), is taken whole: there is no stem to keep otherwise, and a
/// draft called nothing at all is not a name this can produce.
pub fn name_from_file_name(file: &str) -> String {
    match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => file.to_string(),
    }
}

/// The parent folder of a draft-relative path, and its basename.
pub(super) fn split_path(path: &str) -> (Option<&str>, &str) {
    match path.rsplit_once('/') {
        Some((parent, base)) => (Some(parent), base),
        None => (None, path),
    }
}

/// Where a case-only rename parks an entry between its two legs.
///
/// Dot-prefixed so the rail never shows it (DRS-FR-11 skips dot entries): the
/// window it exists in is two syscalls wide, and a process killed inside it must
/// not leave something that looks like one of the author's files.
pub(super) fn staging_name(new_name: &str) -> String {
    format!(".{new_name}.synthesis-rename")
}

/// The staging path a case-only rename of `primary` would have parked it at, if
/// one is sitting there — which means a rename was interrupted between its two
/// legs and the file is still present under a name nothing points at.
pub(super) fn staged_leftover(files_dir: &Path, primary: &str) -> Option<String> {
    let (_, current) = split_path(primary);
    let staged = with_basename(primary, &staging_name(current));
    files_dir.join(&staged).is_file().then_some(staged)
}

/// Whether renaming `current` to `new_name` changes nothing but capitalisation
/// — the one rename whose destination is its own source on a folding
/// filesystem, and so the one that has to go through a staging name.
///
/// Compared with Unicode lowercasing rather than `eq_ignore_ascii_case`: APFS
/// and NTFS fold the whole range, so `café spec` → `CAFÉ spec` collides with
/// itself exactly as an ASCII pair does.
///
/// Pulled out of [`rename_entry_named`] so the decision can be tested for what
/// it is — a question about two strings. Left inline, the Unicode half of it is
/// only observable through a filesystem that folds beyond ASCII, so CI on ext4
/// would report a pass for a fold it never performed.
pub(super) fn is_case_only_rename(current: &str, new_name: &str) -> bool {
    current != new_name && current.to_lowercase() == new_name.to_lowercase()
}

/// Rename one entry inside a draft's `files/`, reporting a collision in the
/// draft's own terms (DRS-FR-26).
///
/// A **case-only** change goes through a temporary name. On the case-insensitive
/// filesystems this application is developed and mostly run on (APFS, NTFS),
/// `Spec.md` already "exists" when `spec.md` does — it *is* `spec.md` — so the
/// destination check in `fs::rename_path` refuses the rename and names the file
/// with itself. Capitalising a draft's name is an ordinary act, and DRS-FR-26
/// refuses a path occupied by *another* file, not by the one being renamed. If
/// the second leg fails the entry is put back under its original name, so a
/// refusal still leaves the draft exactly as it was.
pub(super) fn rename_entry(root: &fs::RootFs, files_dir: &Path, path: &str, new_name: &str) -> Result<(), String> {
    rename_entry_named(root, files_dir, path, new_name, &|name| {
        format!("already exists in this draft: {}", with_basename(path, name))
    })
}

/// [`rename_entry`] with the collision message supplied by the caller, so a
/// drafts folder's rename reports a collision in the drafts root's terms rather
/// than in a draft's.
pub(super) fn rename_entry_named(
    root: &fs::RootFs,
    files_dir: &Path,
    path: &str,
    new_name: &str,
    collision: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let (_, current) = split_path(path);
    if is_case_only_rename(current, new_name) {
        let staging = staging_name(new_name);
        root.rename_under(files_dir, path, &staging).map_err(|e| match e {
            fs::FsError::AlreadyExists { .. } => collision(&staging),
            other => other.to_string(),
        })?;
        let staged = with_basename(path, &staging);
        return match root.rename_under(files_dir, &staged, new_name) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = root.rename_under(files_dir, &staged, current);
                Err(match e {
                    fs::FsError::AlreadyExists { .. } => collision(new_name),
                    other => other.to_string(),
                })
            }
        };
    }
    root.rename_under(files_dir, path, new_name).map_err(|e| match e {
        fs::FsError::AlreadyExists { .. } => collision(new_name),
        other => other.to_string(),
    })
}

/// Put `base` back where `path` sat — a rename keeps the entry in its current
/// parent (DRS-FR-14), so the primary file follows the draft's name wherever in
/// the tree it has been moved to.
pub(super) fn with_basename(path: &str, base: &str) -> String {
    match split_path(path) {
        (Some(parent), _) => format!("{parent}/{base}"),
        (None, _) => base.to_string(),
    }
}

