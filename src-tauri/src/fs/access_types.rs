//! The payload shapes `FsAccess` reports with.
//!
//! See `specifications/core/FSA-filesystem-access.md` (Contract surface ->
//! Payload shapes). The types live beside the instance rather than inside it
//! because they carry no policy: they are what an operation answers with.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Payload shapes (spec: Contract surface -> Payload shapes)
// ---------------------------------------------------------------------------

/// What an entry *is*, as reported without resolving anything. A symbolic link
/// reports as `Symlink` whatever it points at — that is what makes `file_info`
/// usable for deciding whether a path is a link at all (FSA-FR-23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    /// FSA-FR-22: a FIFO, a socket, or a device. It is never read as a file.
    Other,
}

impl EntryKind {
    pub(super) fn of(meta: &fs::Metadata) -> EntryKind {
        let ft = meta.file_type();
        if ft.is_symlink() {
            EntryKind::Symlink
        } else if ft.is_dir() {
            EntryKind::Dir
        } else if ft.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        }
    }
}

/// One immediate child of a listed directory (FSA-FR-22).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DirEntry {
    /// The basename alone — never a path.
    pub name: String,
    /// The kind of this entry itself.
    pub kind: EntryKind,
}

/// The metadata of a single entry (FSA-FR-23).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileInfo {
    pub kind: EntryKind,
    pub size: u64,
    /// `None` on the platforms and filesystems that do not report one.
    pub modified: Option<SystemTime>,
    pub readonly: bool,
}

/// How `create_file` treats an entry already at the path (FSA-FR-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateMode {
    /// Create, or empty what is already there.
    Truncate,
    /// Create only if nothing occupies the path; otherwise a typed "exists".
    Exclusive,
}

/// How `open_file` opens (FSA-FR-25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Read only. The file must exist; nothing is created, parents included.
    ReadOnly,
    /// Write at the end. Creates the file and its parents when absent.
    Append,
    /// Write from empty. Creates the file and its parents when absent.
    Truncate,
}

impl OpenMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            OpenMode::ReadOnly => "read-only",
            OpenMode::Append => "append",
            OpenMode::Truncate => "truncate",
        }
    }
    pub(super) fn can_read(self) -> bool {
        matches!(self, OpenMode::ReadOnly)
    }
    pub(super) fn can_write(self) -> bool {
        matches!(self, OpenMode::Append | OpenMode::Truncate)
    }
}

/// An absolute path the *user* chose in a native dialog (FSA-FR-28).
///
/// Holding one is the key that opens the two writes able to land outside the
/// allowlist, and it is constructible only inside this crate, at one named
/// call site: `from_frontend_response`, where a dialog's answer re-enters the
/// backend across the IPC boundary. Read that function's doc before adding a
/// second caller — it is exact about what the token does and does not prove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserChosenPath(PathBuf);

impl UserChosenPath {
    /// Mint a token from a path the frontend is handing back after a
    /// dialog it drove (`browse_for_save_path` → `export_logs`).
    ///
    /// This is the token's whole mint point, and it is worth being exact about
    /// what it therefore proves. A `UserChosenPath` cannot survive the IPC
    /// round-trip — Tauri's wire format is JSON, so what comes back from the
    /// frontend is a string like any other, and no type can carry the user's
    /// click across that boundary. What the token still buys is real: the
    /// FSA-FR-28 exemption is reachable from exactly one command rather than
    /// from anywhere in the backend a path can be composed, and every
    /// in-process caller of every other operation stays bound to the allowlist
    /// with no way to opt out. What it does not buy is protection from a
    /// frontend naming a path the user never picked. Do not widen the set of
    /// callers.
    ///
    /// Returns `None` for a path that is not absolute. A dialog only ever
    /// returns an absolute path, so a relative one did not come from the user
    /// however it arrived — and letting one through would resolve it against
    /// the process working directory, which belongs to no root and to no
    /// choice anybody made. FSA-FR-19 refuses relative paths on every other
    /// operation; the exempt one is not the place to start accepting them.
    pub(crate) fn from_frontend_response(path: impl Into<PathBuf>) -> Option<UserChosenPath> {
        let path = path.into();
        path.is_absolute().then_some(UserChosenPath(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}
