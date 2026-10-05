//! The guarded open file (FSA-FR-26) and the content root paired with the
//! instance that governs it (FSA-FR-19).

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::access::{CreateMode, DirEntry, FileInfo, FsAccess, OpenMode};
use super::{FsError, FsResult};

// ---------------------------------------------------------------------------
// FSA-FR-26: the guarded handle
// ---------------------------------------------------------------------------

/// An open file, bound to the instance that opened it and to the mode it was
/// opened with (FSA-FR-26).
///
/// It exposes no way to name another path, so it cannot be re-targeted after
/// the containment check that produced it. The `'a` lifetime is what makes
/// "does not outlive its instance" a compile-time fact rather than a runtime
/// hope — a handle held past its `FsAccess` does not build.
///
/// Writes through a handle reach the file directly and are therefore the one
/// write path in this module outside the temp-file-and-rename guarantee of
/// FSA-FR-04: a process that dies mid-write leaves whatever it had written.
/// That is the price of streaming a file too large to hold in memory, and of
/// wanting bytes on disk before the next record is formed.
///
/// The lifetime is load-bearing rather than decorative, so it is asserted here
/// rather than left to review: a handle used after its instance is gone does
/// not compile. The error code is pinned because a bare `compile_fail` also
/// passes on a typo, which would make this guard silently vacuous.
///
/// ```compile_fail,E0505
/// use synthesis_lib::fs::{FsAccess, OpenMode};
/// let dir = std::env::temp_dir();
/// let access = FsAccess::builder().allow_root(&dir).build().unwrap();
/// let handle = access.open_file(dir.join("x"), OpenMode::Truncate).unwrap();
/// drop(access);
/// let _ = handle.mode();
/// ```
#[derive(Debug)]
pub struct FileHandle<'a> {
    pub(super) file: fs::File,
    pub(super) mode: OpenMode,
    pub(super) _instance: PhantomData<&'a FsAccess>,
}

impl FileHandle<'_> {
    /// The mode this handle was opened with.
    pub fn mode(&self) -> OpenMode {
        self.mode
    }

    /// Read into `buf`. A read through an `Append` or `Truncate` handle is a
    /// typed `WrongMode` and changes nothing — the mode is enforced on every
    /// call rather than only at open.
    pub fn read(&mut self, buf: &mut [u8]) -> FsResult<usize> {
        if !self.mode.can_read() {
            return Err(FsError::WrongMode {
                mode: self.mode.label(),
                attempted: "read",
            });
        }
        self.file.read(buf).map_err(FsError::Io)
    }

    /// Read the rest of the file into `buf`.
    pub fn read_to_end(&mut self, buf: &mut Vec<u8>) -> FsResult<usize> {
        if !self.mode.can_read() {
            return Err(FsError::WrongMode {
                mode: self.mode.label(),
                attempted: "read",
            });
        }
        self.file.read_to_end(buf).map_err(FsError::Io)
    }

    /// Read the rest of the file as UTF-8 text.
    pub fn read_to_string(&mut self) -> FsResult<String> {
        let mut bytes = Vec::new();
        self.read_to_end(&mut bytes)?;
        String::from_utf8(bytes).map_err(|e| FsError::Utf8 {
            path: PathBuf::new(),
            source: e.utf8_error(),
        })
    }

    /// Write `buf`. A write through a `ReadOnly` handle is a typed `WrongMode`
    /// and leaves the file byte-for-byte unchanged.
    pub fn write_all(&mut self, buf: &[u8]) -> FsResult<()> {
        if !self.mode.can_write() {
            return Err(FsError::WrongMode {
                mode: self.mode.label(),
                attempted: "write",
            });
        }
        self.file.write_all(buf).map_err(FsError::Io)
    }

    /// Flush buffered bytes to the operating system.
    pub fn flush(&mut self) -> FsResult<()> {
        if !self.mode.can_write() {
            return Ok(());
        }
        self.file.flush().map_err(FsError::Io)
    }

    /// Flush and ask the OS to put the bytes on stable storage.
    pub fn sync(&mut self) -> FsResult<()> {
        self.flush()?;
        self.file.sync_data().map_err(FsError::Io)
    }

    /// Seek within the file. Only meaningful for a readable handle; an
    /// `Append` handle's writes always land at the end whatever the position.
    pub fn seek(&mut self, pos: SeekFrom) -> FsResult<u64> {
        self.file.seek(pos).map_err(FsError::Io)
    }
}

impl Drop for FileHandle<'_> {
    fn drop(&mut self) {
        // Flush on drop so a handle that goes out of scope without an explicit
        // flush still has its bytes on disk. Best-effort: a destructor must not
        // panic, and `File`'s own drop closes the descriptor regardless.
        if self.mode.can_write() {
            let _ = self.file.flush();
        }
    }
}

// ---------------------------------------------------------------------------
// FSA-FR-19: the content root, and the guarded access that governs it
// ---------------------------------------------------------------------------

/// A content root paired with the [`FsAccess`] that governs it.
///
/// This is what the backend's modules are handed instead of a bare `&Path`, and
/// pairing the two is the point: a root on its own is just a string a caller
/// could have composed, while a root that arrives with its access already
/// attached cannot be used to reach anything the instance does not allow. The
/// two are set together whenever the active worktree changes
/// (`project::remount_content_root`), so they cannot drift apart.
///
/// It derefs to `Path` for the same reason `PathBuf` does — callers overwhelmingly
/// want the root itself (`root.join(..)`, `root.display()`), and forcing a
/// `.path()` at every one of those sites would buy nothing. The guarded
/// operations are inherent methods, so they are never what you get by accident.
///
/// The `*_under` methods keep the two-level containment the modules already
/// relied on: `resolve_under` confines the path to the sub-root the caller
/// names (a draft's `files/`, the comments directory), and the instance then
/// confines it to the allowlist. Neither replaces the other — the first is
/// about a feature's own boundary, the second about the process's.
#[derive(Debug, Clone)]
pub struct RootFs {
    root: PathBuf,
    access: std::sync::Arc<FsAccess>,
}

impl std::ops::Deref for RootFs {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.root
    }
}

impl AsRef<Path> for RootFs {
    fn as_ref(&self) -> &Path {
        &self.root
    }
}

impl RootFs {
    /// Pair a content root with the instance that governs it.
    pub fn new(root: PathBuf, access: std::sync::Arc<FsAccess>) -> RootFs {
        RootFs { root, access }
    }

    /// A standalone root governed by an instance that allows exactly it.
    ///
    /// Test-only, and deliberately so: in production every module shares the one
    /// instance built at project open (FSA-FR-21), and a constructor that mints
    /// a fresh allowlist on demand would be a way to opt out of it. Under
    /// `cfg(test)` that is exactly what a unit test wants — its own tempdir,
    /// governed, with no application around it.
    #[cfg(test)]
    pub fn for_root(root: impl AsRef<Path>) -> RootFs {
        // The path is handed to `allow_root` exactly as given, NOT canonicalised
        // first. The builder canonicalises internally and allowlists both
        // spellings when they differ (FSA-FR-18), and that alias is the whole
        // point here: a test names its tempdir as `/var/folders/…` while its
        // canonical form is `/private/var/folders/…`. Canonicalising up front
        // registers only the second, and every path the test then composes from
        // the first is refused by the gate — which a test asserting on an error
        // will happily read as the error it was looking for.
        let raw = root.as_ref().to_path_buf();
        let access = FsAccess::builder()
            .allow_root(&raw)
            .build()
            .expect("a test root must be a real directory");
        RootFs {
            root: raw,
            access: std::sync::Arc::new(access),
        }
    }

    /// The root itself, for the callers that want it named rather than deref'd.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// The instance governing this root.
    pub fn access(&self) -> &FsAccess {
        &self.access
    }

    /// Resolve `path` to an absolute location under this root. An absolute
    /// input is taken as-is (the instance still judges it); a relative one is
    /// joined to the root, which is what the module call sites pass.
    fn abs(&self, path: impl AsRef<Path>) -> PathBuf {
        let p = path.as_ref();
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    // -- whole-file reads and atomic writes --------------------------------

    pub fn read_bytes(&self, path: impl AsRef<Path>) -> FsResult<Vec<u8>> {
        self.access.read_bytes(self.abs(path))
    }
    pub fn read_text(&self, path: impl AsRef<Path>) -> FsResult<String> {
        self.access.read_text(self.abs(path))
    }
    pub fn read_toml<T: DeserializeOwned>(&self, path: impl AsRef<Path>) -> FsResult<T> {
        self.access.read_toml(self.abs(path))
    }
    pub fn write_bytes_atomic(&self, path: impl AsRef<Path>, bytes: &[u8]) -> FsResult<()> {
        self.access.write_bytes_atomic(self.abs(path), bytes)
    }
    pub fn write_text_atomic(&self, path: impl AsRef<Path>, text: &str) -> FsResult<()> {
        self.access.write_text_atomic(self.abs(path), text)
    }
    pub fn write_toml_atomic<T: Serialize>(&self, path: impl AsRef<Path>, v: &T) -> FsResult<()> {
        self.access.write_toml_atomic(self.abs(path), v)
    }
    pub fn append_lines(&self, path: impl AsRef<Path>, lines: &[String]) -> FsResult<()> {
        self.access.append_lines(self.abs(path), lines)
    }
    pub fn sha256_file(&self, path: impl AsRef<Path>) -> FsResult<String> {
        self.access.sha256_file(self.abs(path))
    }
    pub fn ensure_gitignored(&self, dir: impl AsRef<Path>) -> FsResult<()> {
        self.access.ensure_gitignored(self.abs(dir))
    }

    // -- listing and metadata ----------------------------------------------

    pub fn list_dir(&self, path: impl AsRef<Path>) -> FsResult<Vec<DirEntry>> {
        self.access.list_dir(self.abs(path))
    }
    pub fn file_info(&self, path: impl AsRef<Path>) -> FsResult<FileInfo> {
        self.access.file_info(self.abs(path))
    }
    pub fn create_file(&self, path: impl AsRef<Path>, mode: CreateMode) -> FsResult<()> {
        self.access.create_file(self.abs(path), mode)
    }
    pub fn open_file(&self, path: impl AsRef<Path>, mode: OpenMode) -> FsResult<FileHandle<'_>> {
        self.access.open_file(self.abs(path), mode)
    }

    // -- structural mutations, confined to a caller-named sub-root ---------

    /// Remove `rel` under `base`. `base` is the feature's own boundary (a
    /// draft's `files/`, the project root); `rel` may not escape it, and the
    /// result may not escape the allowlist.
    pub fn delete_under(
        &self,
        base: impl AsRef<Path>,
        rel: impl AsRef<Path>,
        recursive: bool,
    ) -> FsResult<()> {
        let target = super::resolve_under(base.as_ref(), rel.as_ref())?;
        self.access.delete_path(target, recursive)
    }

    /// Rename `rel` under `base` to the bare basename `new_name`.
    pub fn rename_under(
        &self,
        base: impl AsRef<Path>,
        rel: impl AsRef<Path>,
        new_name: &str,
    ) -> FsResult<()> {
        let target = super::resolve_under(base.as_ref(), rel.as_ref())?;
        self.access.rename_path(target, new_name)
    }

    /// Move `source_rel` to `dest_rel`, both under `base` (FSA-FR-30). Unlike
    /// [`RootFs::rename_under`], the two need not share a parent.
    pub fn move_under(
        &self,
        base: impl AsRef<Path>,
        source_rel: impl AsRef<Path>,
        dest_rel: impl AsRef<Path>,
    ) -> FsResult<()> {
        let base = base.as_ref();
        let src = super::resolve_under(base, source_rel.as_ref())?;
        let dst = super::resolve_under(base, dest_rel.as_ref())?;
        self.access.move_path(src, dst)
    }

    /// Copy `source_rel` to `dest_rel`, both under `base`.
    pub fn copy_under(
        &self,
        base: impl AsRef<Path>,
        source_rel: impl AsRef<Path>,
        dest_rel: impl AsRef<Path>,
    ) -> FsResult<()> {
        let base = base.as_ref();
        let src = super::resolve_under(base, source_rel.as_ref())?;
        let dst = super::resolve_under(base, dest_rel.as_ref())?;
        self.access.copy_path(src, dst)
    }

    /// Create an empty directory at `rel` under `base`.
    pub fn create_dir_under(&self, base: impl AsRef<Path>, rel: impl AsRef<Path>) -> FsResult<()> {
        let target = super::resolve_under(base.as_ref(), rel.as_ref())?;
        self.access.create_dir(target)
    }
}
