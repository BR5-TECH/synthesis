//! Typed error variants for the filesystem primitives.
//!
//! Per the spec's "Functional requirements" section, callers must be able to
//! distinguish at least the following observable cases:
//! - missing file (FSA-FR-06)
//! - I/O failure (FSA-FR-06)
//! - invalid UTF-8 (FSA-FR-06 — text only)
//! - malformed TOML (FSA-FR-06 — TOML only)
//! - path escapes the caller-supplied root (FSA-FR-10)
//! - not a Synthesis project (FSA-FR-03 sentinel for `discover_project_root`)
//!
//! Note: this enum does NOT derive `PartialEq` because `std::io::Error` does
//! not implement it. Tests that need to inspect a specific variant pattern
//! against the value should use `matches!`.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::str::Utf8Error;

/// Single typed error enum for every filesystem primitive in this module.
#[derive(Debug)]
pub enum FsError {
    /// FSA-FR-06: the file does not exist.
    NotFound { path: PathBuf },
    /// FSA-FR-06: any I/O failure that is not "missing" (permissions, EIO, etc.).
    Io(io::Error),
    /// FSA-FR-06: file content is not valid UTF-8 (text helpers only).
    Utf8 { path: PathBuf, source: Utf8Error },
    /// FSA-FR-06: TOML deserialization or serialization failure.
    ///
    /// We carry a string message rather than `toml::de::Error` / `toml::ser::Error`
    /// to keep the error type stable across the two TOML crate variants — and
    /// because the spec only requires that "malformed" be distinguishable
    /// from "missing" and "unreadable", not that the exact toml-crate error
    /// be preserved.
    Toml { path: PathBuf, message: String },
    /// FSA-FR-10: a relative path resolved to a location outside the caller-
    /// supplied root.
    PathEscape { root: PathBuf, rel: PathBuf },
    /// FSA-FR-12 / FSA-FR-13: a `rename_path` / `copy_path` destination already
    /// exists. The mutation never overwrites — this is the typed "exists" case.
    AlreadyExists { path: PathBuf },
    /// FSA-FR-12: a `rename_path` `new_name` is not a bare basename (empty, a
    /// path separator, or a `.` / `..` component).
    InvalidName { name: String },
    /// FSA-FR-11: a non-recursive `delete_path` was asked to remove a directory
    /// that still holds entries. Nothing was removed. Distinct from `NotFound`
    /// and from `Io`, because it is also the module's emptiness *probe*: a
    /// caller learns a directory has contents by attempting the narrow delete
    /// and reading this, rather than by enumerating and deciding from a listing
    /// that may be filtered.
    NotEmpty { path: PathBuf },
    /// FSA-FR-17: the path is, or reaches through, a symbolic link. No primitive
    /// follows one, so the operation was refused before touching anything. The
    /// refusal is decided from the link itself and never from where it points.
    SymlinkRefused { path: PathBuf },
    /// FSA-FR-03: `discover_project_root` walked to the filesystem root without
    /// finding `.synthesis/project.toml`.
    NotAProject,
    /// FSA-FR-10: the path does not resolve inside any of the instance's
    /// allowlisted roots. Distinct from `PathEscape`, which is the
    /// single-root `(root, rel)` shape the pre-instance helpers use: this one
    /// is decided against the whole root set at once, so it carries the
    /// offending path alone — there is no single root it "escaped".
    EscapesAllowedRoots { path: PathBuf },
    /// FSA-FR-19: an instance method was handed a relative path. An instance
    /// holding several roots has no single directory to resolve one against,
    /// so it is refused rather than silently resolved against the process
    /// working directory (which belongs to no root at all).
    RelativePath { path: PathBuf },
    /// FSA-FR-22: `list_dir` was pointed at something that exists but is not a
    /// directory. Distinct from `NotFound` (nothing there) and from `Io`
    /// (a directory that could not be read).
    NotADirectory { path: PathBuf },
    /// FSA-FR-26: an operation was attempted through a `FileHandle` whose
    /// `OpenMode` does not permit it — a write through `ReadOnly`, or a read
    /// through `Append` / `Truncate`. Nothing was read or written.
    WrongMode {
        /// What the handle was opened as.
        mode: &'static str,
        /// What was attempted.
        attempted: &'static str,
    },
    /// FSA-FR-27: symbolic-link resolution revisited a link it had already
    /// followed, or exceeded the hop bound. Only reachable on an instance
    /// built with `follow_symlinks(true)` — the default policy refuses a link
    /// before there is any chain to walk.
    SymlinkCycle { path: PathBuf },
    /// FSA-FR-30: a `move_path` destination lies inside the source's own
    /// subtree, the source itself included. Nothing is ever moved into itself,
    /// so the pair is refused before anything is touched. Distinct from
    /// `AlreadyExists`, which is about the destination being occupied: this one
    /// is about *where* the destination sits, and it holds even for a
    /// destination nothing occupies.
    InvalidDestination { source: PathBuf, dest: PathBuf },
    /// FSA-FR-UKAE: the instance was built with a read-only grant, and the
    /// operation would write, create, copy into, rename, move, delete, or open
    /// for writing. It is refused before any filesystem access, so nothing was
    /// touched.
    ReadOnly { operation: &'static str },
    /// FSA-FR-NRQE: `read_regular_bytes` met an entry that is not a regular
    /// file — a FIFO, a socket, a device, or a directory.
    NotRegular { path: PathBuf },
    /// FSA-FR-NRQE: `read_regular_bytes` met a file larger than the limit it
    /// was given.
    TooLarge { path: PathBuf, limit: u64 },
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotFound { path } => write!(f, "file not found: {}", path.display()),
            FsError::Io(e) => write!(f, "io error: {e}"),
            FsError::Utf8 { path, source } => {
                write!(f, "invalid utf-8 at {}: {source}", path.display())
            }
            FsError::Toml { path, message } => {
                write!(f, "malformed toml at {}: {message}", path.display())
            }
            FsError::PathEscape { root, rel } => write!(
                f,
                "path {} escapes root {}",
                rel.display(),
                root.display()
            ),
            FsError::AlreadyExists { path } => {
                write!(f, "destination already exists: {}", path.display())
            }
            FsError::InvalidName { name } => write!(f, "invalid name: {name:?}"),
            FsError::NotEmpty { path } => {
                write!(f, "directory not empty: {}", path.display())
            }
            FsError::SymlinkRefused { path } => {
                write!(f, "symlink refused: {}", path.display())
            }
            FsError::NotAProject => write!(f, "not a synthesis project"),
            FsError::EscapesAllowedRoots { path } => {
                write!(f, "path escapes allowed roots: {}", path.display())
            }
            FsError::RelativePath { path } => {
                write!(f, "relative path not accepted: {}", path.display())
            }
            FsError::NotADirectory { path } => {
                write!(f, "not a directory: {}", path.display())
            }
            FsError::WrongMode { mode, attempted } => {
                write!(f, "cannot {attempted} a file opened {mode}")
            }
            FsError::SymlinkCycle { path } => {
                write!(f, "symlink cycle at: {}", path.display())
            }
            FsError::InvalidDestination { source, dest } => write!(
                f,
                "invalid destination: {} lies inside {}",
                dest.display(),
                source.display()
            ),
            FsError::ReadOnly { operation } => {
                write!(f, "read only: {operation} is not allowed on this instance")
            }
            FsError::NotRegular { path } => {
                write!(f, "not a regular file: {}", path.display())
            }
            FsError::TooLarge { path, limit } => {
                write!(f, "file larger than {limit} bytes: {}", path.display())
            }
        }
    }
}

/// Why an `FsAccess` could not be built (FSA-FR-18). Separate from `FsError`
/// because a construction failure is not an operation failure: there is no
/// instance yet, so none of the path-shaped variants above could apply.
#[derive(Debug)]
pub enum FsAccessBuildError {
    /// No `allow_root` was supplied. An instance with an empty allowlist could
    /// reach nothing, so it is refused rather than built useless.
    NoRoots,
    /// A root does not exist, or is not a directory.
    RootNotADirectory { path: PathBuf },
    /// A root could not be canonicalised (FSA-FR-18), so its identity — and
    /// therefore what "inside it" means — could not be established.
    RootNotCanonical { path: PathBuf, source: io::Error },
    /// The session temporary directory could not be created.
    SessionTemp(io::Error),
    /// The platform user-scoped data directory could not be determined or
    /// created, so it could not be allowlisted. Distinct from `SessionTemp`
    /// because they are different subsystems and a reader chasing one of these
    /// messages needs to be pointed at the right one.
    AppDataDir(String),
    /// FSA-FR-UKAE: a read-only grant and a read-write root were given to one
    /// builder. The two never share an instance, so neither is built.
    MixedProfile,
    /// FSA-FR-KAQV: a read-only grant that cannot be established — the path does
    /// not exist, its final component is a symbolic link, or it is the wrong
    /// kind (a folder where a file was granted, or the reverse).
    GrantRefused { path: PathBuf, reason: GrantRefusal },
}

/// Why a read-only grant was refused at build time (FSA-FR-KAQV).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantRefusal {
    /// Nothing exists at the path, or one of its ancestors does not.
    Missing,
    /// The final component is a symbolic link.
    Link,
    /// The path is the wrong kind for the grant, or could not be read.
    Unreadable,
}

impl fmt::Display for FsAccessBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsAccessBuildError::NoRoots => {
                write!(f, "an FsAccess needs at least one allowed root")
            }
            FsAccessBuildError::RootNotADirectory { path } => {
                write!(f, "root is not a directory: {}", path.display())
            }
            FsAccessBuildError::RootNotCanonical { path, source } => {
                write!(f, "root {} could not be resolved: {source}", path.display())
            }
            FsAccessBuildError::SessionTemp(e) => {
                write!(f, "session temp directory could not be created: {e}")
            }
            FsAccessBuildError::AppDataDir(e) => {
                write!(f, "user data directory could not be established: {e}")
            }
            FsAccessBuildError::MixedProfile => {
                write!(f, "a read-only grant and a read-write root cannot share one instance")
            }
            FsAccessBuildError::GrantRefused { path, reason } => {
                write!(f, "read-only grant refused ({reason:?}): {}", path.display())
            }
        }
    }
}

impl std::error::Error for FsAccessBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FsAccessBuildError::RootNotCanonical { source, .. } => Some(source),
            FsAccessBuildError::SessionTemp(e) => Some(e),
            _ => None,
        }
    }
}

impl std::error::Error for FsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FsError::Io(e) => Some(e),
            FsError::Utf8 { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_are_distinguishable_by_matches() {
        let nf = FsError::NotFound {
            path: PathBuf::from("/x"),
        };
        let io_e = FsError::Io(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        let pe = FsError::PathEscape {
            root: PathBuf::from("/r"),
            rel: PathBuf::from("../x"),
        };
        let nap = FsError::NotAProject;

        assert!(matches!(nf, FsError::NotFound { .. }));
        assert!(matches!(io_e, FsError::Io(_)));
        assert!(matches!(pe, FsError::PathEscape { .. }));
        assert!(matches!(nap, FsError::NotAProject));

        // FSA-FR-12 / FSA-FR-13 structural-mutation variants.
        let ae = FsError::AlreadyExists {
            path: PathBuf::from("/r/b"),
        };
        let inv = FsError::InvalidName {
            name: "a/b".into(),
        };
        assert!(matches!(ae, FsError::AlreadyExists { .. }));
        assert!(matches!(inv, FsError::InvalidName { .. }));
    }

    #[test]
    fn display_renders_path_information() {
        let nf = FsError::NotFound {
            path: PathBuf::from("/some/where"),
        };
        assert!(format!("{nf}").contains("/some/where"));
    }
}
