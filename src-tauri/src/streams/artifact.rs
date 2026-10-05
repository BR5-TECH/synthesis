//! The bundle a semantic merge turn reads (GRB-FR-YPEX, GRB-FR-AAVK).
//!
//! A merge Git could not settle whole gives an agent three mirrors of one tree
//! — `base/`, `stream/` and `merged/` — for **the conflicting paths alone**,
//! plus one index document naming every other path the merge wrote. Three
//! renderings of every path the merge touched would cost more to read than the
//! reconciliation costs to do, and the turn is asked about the conflicts only.

use std::path::{Path, PathBuf};

use super::*;

/// GRB-FR-YPEX: the directory every update attempt's bundle stands under.
pub(super) const ATTEMPTS_DIR: &str = "m";
/// The mirror a path's content on the base branch stands in.
pub(super) const MIRROR_BASE: &str = "base";
/// The mirror a path's content on the stream branch stands in.
pub(super) const MIRROR_STREAM: &str = "stream";
/// The mirror a path's deterministic merge result stands in.
pub(super) const MIRROR_MERGED: &str = "merged";
/// GRB-FR-AAVK: the index document, beside the mirrors.
pub(super) const INDEX_DOCUMENT: &str = "index.md";

/// GRB-FR-FMCU: what one file of the bundle may cost one read.
pub(super) const FILE_LIMIT: usize = crate::tools::agent_exec::SEMANTIC_REBASE_FILE_LIMIT as usize;
/// GRB-FR-FMCU: what a cut file says about itself, on its own last line.
pub(super) const CUT_NOTICE: &str = "[synthesis] this file was cut at the 1 MiB bound and is incomplete";

/// Where one attempt's material lives, and what it holds.
pub(super) struct MergeArtifact {
    pub(super) attempt_id: String,
    /// `short_data_dir()/m/<attempt-id>/`.
    pub(super) root: PathBuf,
    /// The throwaway checkout the turn stands in, under the same directory.
    pub(super) checkout: PathBuf,
}

impl MergeArtifact {
    pub(super) fn new(store_root: &Path, attempt_id: &str) -> Self {
        let root = store_root.join(ATTEMPTS_DIR).join(attempt_id);
        Self {
            attempt_id: attempt_id.to_string(),
            checkout: root.join("wt"),
            root,
        }
    }

    /// The mirrors are mounted rather than the whole attempt directory: the
    /// checkout beside them is the working copy the turn already stands in,
    /// and mounting it twice would give the turn a second, read-only view of
    /// its own execution directory.
    pub(super) fn mirrors(&self) -> PathBuf {
        self.root.join("a")
    }
}

/// GRB-FR-QIHE side: what one side did to one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SideChange {
    Created,
    Updated,
    Deleted,
    Unchanged,
}

impl SideChange {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
            Self::Unchanged => "unchanged",
        }
    }
}

/// GRB contract surface: one path a semantic turn is asked about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MergeConflict {
    /// Project-relative.
    pub path: String,
    pub base_change: String,
    pub stream_change: String,
    /// Where the deterministic merge result of this path stands in the mount.
    pub merged_file: String,
}

/// What one conflicting path's three sides hold.
pub(super) struct ConflictSides {
    pub(super) path: String,
    pub(super) ancestor: Option<git2::Oid>,
    pub(super) ours: Option<git2::Oid>,
    pub(super) theirs: Option<git2::Oid>,
}

impl ConflictSides {
    /// Which of the four things the base branch did to this path.
    pub(super) fn base_change(&self) -> SideChange {
        change_of(self.ancestor, self.ours)
    }

    /// The same for the stream branch.
    pub(super) fn stream_change(&self) -> SideChange {
        change_of(self.ancestor, self.theirs)
    }
}

fn change_of(ancestor: Option<git2::Oid>, side: Option<git2::Oid>) -> SideChange {
    match (ancestor, side) {
        (None, Some(_)) => SideChange::Created,
        (Some(_), None) => SideChange::Deleted,
        (Some(a), Some(b)) if a != b => SideChange::Updated,
        _ => SideChange::Unchanged,
    }
}

/// GRB-FR-YPEX / GRB-FR-AAVK: write the bundle for one attempt.
///
/// `merged_now` holds the deterministic merge result of each conflicting path
/// as it stands in the throwaway checkout, which is what the turn corrects.
/// `other_paths` is every path the merge wrote that no question names.
pub(super) fn generate(
    fs: &fsa::FsAccess,
    repo: &git2::Repository,
    artifact: &MergeArtifact,
    conflicts: &[ConflictSides],
    other_paths: &[(String, SideChange, SideChange)],
) -> Result<(), String> {
    let mirrors = artifact.mirrors();
    for mirror in [MIRROR_BASE, MIRROR_STREAM, MIRROR_MERGED] {
        store::ensure_dir(fs, &mirrors.join(mirror))?;
    }

    for conflict in conflicts {
        write_side(fs, repo, &mirrors, MIRROR_BASE, &conflict.path, conflict.ours)?;
        write_side(fs, repo, &mirrors, MIRROR_STREAM, &conflict.path, conflict.theirs)?;
        // The merge result is read from the checkout rather than from an
        // object: a conflicting path has no merged object, and what the turn
        // must correct is the file it can see.
        let live = artifact.checkout.join(&conflict.path);
        if let Ok(bytes) = fs.read_bytes(&live) {
            write_cut(fs, &mirrors.join(MIRROR_MERGED).join(&conflict.path), &bytes)?;
        }
    }

    write_index(fs, &mirrors.join(INDEX_DOCUMENT), conflicts, other_paths)
}

/// One side of one path, where that side holds the path at all.
fn write_side(
    fs: &fsa::FsAccess,
    repo: &git2::Repository,
    mirrors: &Path,
    mirror: &str,
    path: &str,
    oid: Option<git2::Oid>,
) -> Result<(), String> {
    // GRB-FR-YPEX: a path a side did not hold has no file under that mirror,
    // which confirms what the question already states rather than leaving the
    // turn to read an empty file as a deletion.
    let Some(oid) = oid else { return Ok(()) };
    let Ok(blob) = repo.find_blob(oid) else {
        return Ok(());
    };
    write_cut(fs, &mirrors.join(mirror).join(path), blob.content())
}

/// GRB-FR-FMCU: write one file, cut to the bound, saying so where it was cut.
pub(super) fn write_cut(fs: &fsa::FsAccess, target: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        store::ensure_dir(fs, parent)?;
    }
    if bytes.len() <= FILE_LIMIT {
        return fs.write_bytes_atomic(target, bytes).map_err(|e| e.to_string());
    }
    let notice = format!("\n{CUT_NOTICE}\n");
    let keep = FILE_LIMIT.saturating_sub(notice.len());
    let mut cut = bytes[..keep].to_vec();
    cut.extend_from_slice(notice.as_bytes());
    fs.write_bytes_atomic(target, &cut).map_err(|e| e.to_string())
}

/// GRB-FR-AAVK: what moved around the questions.
///
/// One line per path rather than three mirrors of each, so the turn knows what
/// the merge wrote without reading it.
fn write_index(
    fs: &fsa::FsAccess,
    target: &Path,
    conflicts: &[ConflictSides],
    other_paths: &[(String, SideChange, SideChange)],
) -> Result<(), String> {
    let mut out = String::from("# What this merge holds\n\n");
    out.push_str(
        "Each mirror holds one side of a path: `base/` on the branch the stream merges into, \
`stream/` on the stream branch, and `merged/` the result you must correct. The content both \
sides started from is inside the merged file, between the conflict markers. Only the paths \
below carry a question.\n\n",
    );
    out.push_str("## Paths you are asked about\n\n");
    if conflicts.is_empty() {
        out.push_str("None.\n");
    }
    for conflict in conflicts {
        out.push_str(&format!(
            "- `{}` — the base branch {} it, the stream {} it\n",
            conflict.path,
            conflict.base_change().as_str(),
            conflict.stream_change().as_str(),
        ));
    }
    out.push_str("\n## Paths the merge settled on its own\n\n");
    if other_paths.is_empty() {
        out.push_str("None.\n");
    }
    for (path, base, stream) in other_paths {
        out.push_str(&format!(
            "- `{path}` — the base branch {} it, the stream {} it\n",
            base.as_str(),
            stream.as_str(),
        ));
    }
    out.push_str("\nThese are already correct. Do not open them and do not change them.\n");
    if let Some(parent) = target.parent() {
        store::ensure_dir(fs, parent)?;
    }
    fs.write_text_atomic(target, &out).map_err(|e| e.to_string())
}

/// The artifact is removed when its attempt completes, whatever it decided.
pub(super) fn release(fs: &fsa::FsAccess, artifact: &MergeArtifact) {
    // Unconditional: the guarded instance answers `NotFound` for a directory
    // that was never made, and this discards the result either way, so probing
    // first would only add a filesystem read outside `FsAccess`.
    let _ = fs.delete_owned_tree(&artifact.root);
}

/// Whether a name under the attempts root is an attempt's.
pub(super) fn is_attempt_id(name: &str) -> bool {
    match name.strip_prefix('m') {
        Some(rest) => !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

/// GRB-FR-YPEX: an attempt id, unique on the machine.
pub(super) fn new_attempt_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("m{nanos:x}{:x}", std::process::id())
}
