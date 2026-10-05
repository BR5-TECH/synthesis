//! `FsAccess` — the configured filesystem instance every backend module reaches
//! the disk through. See `specifications/core/FSA-filesystem-access.md`,
//! FSA-FR-18 through FSA-FR-28.
//!
//! The point of the type is stated in the spec's Intent: isolation is
//! *enforceable* rather than advisory because there is no unguarded primitive to
//! compose a path into. Every read, write, listing and structural mutation is a
//! method here, and every one of them runs the same gate first — absolute path,
//! lexical normalisation, containment against the allowlist, then the symlink
//! policy. A module holding an `&FsAccess` holds a capability of exactly known
//! reach, because the allowlist and the policy are fixed at construction and no
//! method widens either.
//!
//! Layout:
//! - `FsAccessBuilder` / `FsAccess::builder()` — construction (FSA-FR-18).
//! - `FsAccess::validate*` — the gate (FSA-FR-10, FSA-FR-17, FSA-FR-19, FSA-FR-27).
//! - session temp ownership (FSA-FR-20).
//! - `list_dir` / `file_info` (FSA-FR-22, FSA-FR-23).
//! - `create_file` / `open_file` + `FileHandle` (FSA-FR-24 – FSA-FR-26).
//! - `UserChosenPath` and the two writes that accept one (FSA-FR-28).
//!
//! The read/write/mutate methods delegate their *mechanics* to the free
//! functions in the parent module (atomic temp-and-rename, the gitignore merge,
//! the streaming checksum) once the gate has produced a validated absolute path.
//! Those free functions are the implementation; this type is the contract.

use std::marker::PhantomData;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::{FsError, FsResult};

/// Hop bound for symlink resolution under `follow_symlinks(true)` (FSA-FR-27).
/// Chosen to match the Linux `MAXSYMLINKS` of 40 — deep enough that no honest
/// chain approaches it, shallow enough that a cycle is reported promptly.
const MAX_SYMLINK_HOPS: usize = 40;

// ---------------------------------------------------------------------------
// The module is laid out over sibling files, and re-exports them here so
// `fs::access::*` names exactly what it always named.
// ---------------------------------------------------------------------------

pub use super::access_build::FsAccessBuilder;
pub use super::access_handles::{FileHandle, RootFs};
pub use super::access_state::FsAccessState;
pub use super::access_types::{CreateMode, DirEntry, EntryKind, FileInfo, OpenMode, UserChosenPath};

pub(super) use super::access_build::canonicalize_root;
use super::access_build::SessionTemp;

/// The guarded filesystem handle (FSA-FR-18).
///
/// Immutable once built: no method adds a root, removes one, or changes the
/// symlink policy, so a reference handed to a module is a capability whose
/// reach can be reasoned about from where it was built. `Send + Sync`, and
/// meant to be shared as an `Arc` — one instance per session is the point,
/// since a module holding its own would be a module holding its own allowlist.
#[derive(Debug)]
pub struct FsAccess {
    /// Canonical, existing directories. Never empty (FSA-FR-18).
    pub(super) roots: Vec<PathBuf>,
    /// FSA-FR-ZUCF: the subset of `roots` the application itself owns. Empty on
    /// an instance that owns none, which refuses `delete_owned_tree` outright.
    pub(super) owned_roots: Vec<PathBuf>,
    /// FSA-FR-OWVT: the subset of `roots` an author's tree may be removed from.
    /// Empty on every instance the application holds for its sessions.
    pub(super) author_roots: Vec<PathBuf>,
    pub(super) follow_symlinks: bool,
    pub(super) session_temp: Option<SessionTemp>,
    /// FSA-FR-UKAE: whether this instance was built from read-only grants. Such
    /// an instance refuses every operation that writes.
    pub(super) read_only: bool,
    /// FSA-FR-GHBN: the exact canonical files a read-only instance may reach,
    /// and nothing beside them. Empty on a read-write instance.
    pub(super) file_grants: Vec<PathBuf>,
}

impl FsAccess {
    pub fn builder() -> FsAccessBuilder {
        FsAccessBuilder::default()
    }

    /// The roots this instance may reach, canonicalised.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }


    /// Whether this instance resolves symbolic links (FSA-FR-17 / FSA-FR-27).
    pub fn follows_symlinks(&self) -> bool {
        self.follow_symlinks
    }

    /// The scratch directory this instance owns, or `None` when it was built
    /// without one (FSA-FR-20).
    pub fn session_temp_dir(&self) -> Option<&Path> {
        self.session_temp.as_ref().map(|t| t.path.as_path())
    }

    // -----------------------------------------------------------------------
    // The gate: FSA-FR-10 / FSA-FR-17 / FSA-FR-19 / FSA-FR-27
    // -----------------------------------------------------------------------

    /// Lexically normalise an absolute path: drop `.`, collapse `..` against
    /// the preceding component, and consult the disk for nothing. FSA-FR-10
    /// requires the *normalised* result to be the thing measured against the
    /// roots, so a path that climbs out and back in is judged by where it ends
    /// rather than by where it passed.
    pub(super) fn normalize(path: &Path) -> PathBuf {
        let mut out: Vec<Component> = Vec::new();
        for c in path.components() {
            match c {
                Component::Prefix(_) | Component::RootDir => out.push(c),
                Component::CurDir => {}
                Component::ParentDir => {
                    // Never pop the root itself; `/..` is `/` on every platform
                    // this targets.
                    let poppable = out
                        .last()
                        .is_some_and(|c| matches!(c, Component::Normal(_)));
                    if poppable {
                        out.pop();
                    }
                }
                Component::Normal(_) => out.push(c),
            }
        }
        out.iter().collect()
    }

    /// The allowlisted root containing `path`, or the typed FSA-FR-10 refusal.
    /// Containment is decided against the whole set, so a path inside any root
    /// is accepted.
    ///
    /// FSA-FR-GHBN: a file grant is a "root" that contains its exact path and
    /// nothing below it, so it is matched by equality rather than by prefix.
    fn root_for(&self, path: &Path) -> FsResult<&Path> {
        self.roots
            .iter()
            .find(|r| path.starts_with(r))
            .or_else(|| self.file_grants.iter().find(|g| path == g.as_path()))
            .map(|r| r.as_path())
            .ok_or_else(|| FsError::EscapesAllowedRoots {
                path: path.to_path_buf(),
            })
    }

    /// Absolute-path requirement (FSA-FR-19) plus normalisation plus
    /// containment (FSA-FR-10). No filesystem access whatsoever — a refusal
    /// here has touched nothing.
    pub(super) fn contain(&self, path: &Path) -> FsResult<PathBuf> {
        if !path.is_absolute() {
            return Err(FsError::RelativePath {
                path: path.to_path_buf(),
            });
        }
        let normalized = Self::normalize(path);
        self.root_for(&normalized)?;
        Ok(normalized)
    }

    /// The full gate, for every operation that *traverses* to the entry.
    /// Returns the absolute path the operation should act on.
    fn validate(&self, path: &Path) -> FsResult<PathBuf> {
        let contained = self.contain(path)?;
        self.refuse_grant_link(&contained)?;
        if self.follow_symlinks {
            self.resolve_following(&contained)
        } else {
            self.refuse_links(&contained, false)?;
            Ok(contained)
        }
    }

    /// The gate for `file_info`, which *reports on* an entry rather than
    /// traversing to it (FSA-FR-23).
    ///
    /// The parent chain is traversed, so the symlink policy applies to it in
    /// full under either setting. The final component is not: it is described
    /// as it is and never resolved, which is what lets `file_info` answer
    /// "is this path a link?" — the question `ASC-artifact-scanning` ASC-FR-23
    /// asks of every node it considers. That holds whatever the policy, because
    /// a caller that has to know whether an entry is a link is not helped by an
    /// instance that quietly answers about something else.
    fn validate_reporting(&self, path: &Path) -> FsResult<PathBuf> {
        let contained = self.contain(path)?;
        // An allowlisted root is describable as itself. Splitting it into
        // parent + leaf would ask whether its *parent* is reachable, which it is
        // not by construction — so `file_info` and `list_dir` on the root a
        // caller was given would refuse the one path it certainly holds. There
        // is nothing below the root to examine here either: the roots were
        // canonicalised at build (FSA-FR-18) and so carry no unresolved link.
        if self.roots.iter().any(|root| *root == contained)
            || self.file_grants.iter().any(|grant| *grant == contained)
        {
            return Ok(contained);
        }
        let (parent, leaf) = match (contained.parent(), contained.file_name()) {
            (Some(p), Some(l)) => (p.to_path_buf(), l.to_os_string()),
            // A root path has no leaf to hold back.
            _ => return Ok(contained),
        };
        let parent = if self.follow_symlinks {
            self.resolve_following(&parent)?
        } else {
            self.refuse_links(&parent, false)?;
            parent
        };
        let resolved = parent.join(leaf);
        // Following may have moved the parent, so containment is re-checked
        // against where the path actually landed.
        self.root_for(&resolved)?;
        Ok(resolved)
    }

    /// FSA-FR-LKTD: on a read-only instance, refuse when the granted path that
    /// contains `path` is a symbolic link now. The grant was checked at build
    /// time; this is the same check at the time of the operation, which is what
    /// stops a file grant that was swapped for a link from reaching its target.
    /// A read-write instance holds canonical roots by construction and is not
    /// asked.
    fn refuse_grant_link(&self, path: &Path) -> FsResult<()> {
        if !self.read_only {
            return Ok(());
        }
        let grant = self.root_for(path)?;
        match fs::symlink_metadata(grant) {
            Ok(meta) if meta.file_type().is_symlink() => Err(FsError::SymlinkRefused {
                path: grant.to_path_buf(),
            }),
            // A grant that is gone reports its own missing error on its own terms.
            Ok(_) | Err(_) => Ok(()),
        }
    }

    /// FSA-FR-17: refuse when any component below the containing root is a
    /// symbolic link.
    ///
    /// Only components *below* the root are examined, and that is exact rather
    /// than lax: the roots were canonicalised at build (FSA-FR-18), so they
    /// contain no unresolved link by construction, and everything below them is
    /// the region a caller composes. The check never resolves a link to see
    /// where it points — the refusal is about the link, not its target, which
    /// is what makes it total and what keeps a link whose target moves later
    /// from turning a previously-accepted path into an escape.
    pub(super) fn refuse_links(&self, path: &Path, allow_leaf_link: bool) -> FsResult<()> {
        let root = self.root_for(path)?;
        let tail: Vec<Component> = path
            .strip_prefix(root)
            .map(|t| t.components().collect())
            .unwrap_or_default();
        let last = tail.len().saturating_sub(1);
        let mut probe = root.to_path_buf();
        for (i, component) in tail.iter().enumerate() {
            probe.push(component);
            match fs::symlink_metadata(&probe) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    if allow_leaf_link && i == last {
                        return Ok(());
                    }
                    return Err(FsError::SymlinkRefused { path: probe });
                }
                // A component that does not exist yet cannot be a link, and the
                // operation reports its own missing error on its own terms.
                Ok(_) | Err(_) => {}
            }
        }
        Ok(())
    }

    /// FSA-FR-27: resolve each link and validate the resolution rather than
    /// accepting it.
    ///
    /// Recursive by construction: after a hop the walk restarts from the
    /// resolved path with the remaining components reattached, so a link
    /// pointing at another link is followed through the whole chain and *every*
    /// hop is measured against the allowlist. A chain that leaves the allowlist
    /// at any hop is refused even when its last target lands back inside.
    fn resolve_following(&self, path: &Path) -> FsResult<PathBuf> {
        let mut current = path.to_path_buf();
        let mut hops = 0usize;
        let mut visited: HashSet<PathBuf> = HashSet::new();

        'restart: loop {
            let root = self.root_for(&current)?.to_path_buf();
            let tail: Vec<Component> = current
                .strip_prefix(&root)
                .map(|t| t.components().collect())
                .unwrap_or_default();
            let mut probe = root;
            for (i, component) in tail.iter().enumerate() {
                probe.push(component);
                let meta = match fs::symlink_metadata(&probe) {
                    Ok(m) => m,
                    // Not there yet — nothing below it can be a link either.
                    Err(_) => break,
                };
                if !meta.file_type().is_symlink() {
                    continue;
                }
                hops += 1;
                if hops > MAX_SYMLINK_HOPS || !visited.insert(probe.clone()) {
                    return Err(FsError::SymlinkCycle { path: probe });
                }
                let target = fs::read_link(&probe).map_err(FsError::Io)?;
                let joined = if target.is_absolute() {
                    target
                } else {
                    probe
                        .parent()
                        .map(|p| p.join(&target))
                        .unwrap_or(target)
                };
                let resolved = Self::normalize(&joined);
                // Every hop is validated, not just the last one.
                self.root_for(&resolved)?;
                let mut next = resolved;
                for rest in &tail[i + 1..] {
                    next.push(rest);
                }
                current = next;
                continue 'restart;
            }
            return Ok(current);
        }
    }

    /// FSA-FR-17 / FSA-FR-27 for a whole subtree, run *before* a recursive
    /// delete or a directory copy rather than during it, so a link found deep in
    /// a tree produces a clean refusal instead of one of the partial results the
    /// module's non-functional contract otherwise permits.
    fn check_tree(&self, dir: &Path) -> FsResult<()> {
        for entry in fs::read_dir(dir).map_err(FsError::Io)? {
            let entry = entry.map_err(FsError::Io)?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).map_err(FsError::Io)?;
            if meta.file_type().is_symlink() {
                if self.follow_symlinks {
                    // Following: the link is allowed only if where it goes is
                    // allowed, on the same per-hop terms as a path argument.
                    self.resolve_following(&path)?;
                    continue;
                }
                return Err(FsError::SymlinkRefused { path });
            }
            if meta.is_dir() {
                self.check_tree(&path)?;
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // FSA-FR-22 / FSA-FR-23: listing and metadata
    // -----------------------------------------------------------------------

    /// FSA-FR-22: the immediate children of `path`, sorted by name, one level
    /// deep. No filter is applied — every child the OS reports is returned,
    /// hidden and ignored files included, because deciding what to omit belongs
    /// to the caller that knows why.
    pub fn list_dir(&self, path: impl AsRef<Path>) -> FsResult<Vec<DirEntry>> {
        self.list_dir_reporting(path).map(|(entries, _)| entries)
    }

    /// FSA-FR-22: `list_dir` that also reports how many entries disappeared
    /// between the directory read and their description. Those are omitted, and
    /// the listing still succeeds.
    pub fn list_dir_reporting(&self, path: impl AsRef<Path>) -> FsResult<(Vec<DirEntry>, usize)> {
        self.list_dir_described(path, &|entry| fs::symlink_metadata(entry))
    }

    /// `list_dir_reporting` with the description of each entry given, so a test
    /// can make an entry vanish between the directory read and its description.
    pub(super) fn list_dir_described(
        &self,
        path: impl AsRef<Path>,
        describe: &dyn Fn(&Path) -> io::Result<fs::Metadata>,
    ) -> FsResult<(Vec<DirEntry>, usize)> {
        // Listing opens the directory, so it traverses: a leaf link is refused
        // under the default policy and resolved under FSA-FR-27, exactly as for
        // a read. The *entries* are still each described without resolution
        // below, which is where FSA-FR-22's "a link is listed as a link" lives.
        let target = self.validate(path.as_ref())?;
        let meta = match fs::symlink_metadata(&target) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(FsError::NotFound { path: target })
            }
            Err(e) => return Err(FsError::Io(e)),
        };
        if !meta.is_dir() {
            return Err(FsError::NotADirectory { path: target });
        }
        let mut out = Vec::new();
        let mut omitted = 0usize;
        for entry in fs::read_dir(&target).map_err(FsError::Io)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    omitted += 1;
                    continue;
                }
                Err(e) => return Err(FsError::Io(e)),
            };
            let meta = match describe(&entry.path()) {
                Ok(meta) => meta,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    omitted += 1;
                    continue;
                }
                Err(e) => return Err(FsError::Io(e)),
            };
            out.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                kind: EntryKind::of(&meta),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((out, omitted))
    }

    /// FSA-FR-23: the metadata of the entry at `path`, without reading its
    /// content, so the cost does not scale with file size. Describes the entry
    /// itself and resolves nothing: a link reports as a link with its own size
    /// and modification time, never its target's.
    pub fn file_info(&self, path: impl AsRef<Path>) -> FsResult<FileInfo> {
        let target = self.validate_reporting(path.as_ref())?;
        let meta = match fs::symlink_metadata(&target) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(FsError::NotFound { path: target })
            }
            Err(e) => return Err(FsError::Io(e)),
        };
        Ok(FileInfo {
            kind: EntryKind::of(&meta),
            size: meta.len(),
            modified: meta.modified().ok(),
            readonly: meta.permissions().readonly(),
        })
    }

    // -----------------------------------------------------------------------
    // FSA-FR-24 / FSA-FR-25 / FSA-FR-26: creation and open handles
    // -----------------------------------------------------------------------

    /// FSA-FR-24: create a zero-length file, creating missing parents.
    ///
    /// Under `Exclusive` the check and the creation are one operation at the
    /// filesystem level (`O_EXCL`) rather than a test followed by a write, so
    /// two callers racing to create the same path cannot both succeed.
    pub fn create_file(&self, path: impl AsRef<Path>, mode: CreateMode) -> FsResult<()> {
        self.deny_write("create_file")?;
        let target = self.validate(path.as_ref())?;
        // A directory at the destination is "exists" under both modes:
        // truncating a directory means nothing.
        if let Ok(meta) = fs::symlink_metadata(&target) {
            if meta.is_dir() {
                return Err(FsError::AlreadyExists { path: target });
            }
        }
        create_parents(&target)?;
        let mut opts = fs::OpenOptions::new();
        opts.write(true);
        match mode {
            CreateMode::Exclusive => {
                opts.create_new(true);
            }
            CreateMode::Truncate => {
                opts.create(true).truncate(true);
            }
        }
        match opts.open(&target) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                Err(FsError::AlreadyExists { path: target })
            }
            Err(e) => Err(FsError::Io(e)),
        }
    }

    /// FSA-FR-25: open `path` and return a guarded handle.
    ///
    /// The path is validated before the file is opened, so a refused path never
    /// yields a handle.
    pub fn open_file(&self, path: impl AsRef<Path>, mode: OpenMode) -> FsResult<FileHandle<'_>> {
        if mode != OpenMode::ReadOnly {
            self.deny_write("open_file")?;
        }
        let target = self.validate(path.as_ref())?;
        let mut opts = fs::OpenOptions::new();
        match mode {
            OpenMode::ReadOnly => {
                // Creates nothing at all, parents included.
                opts.read(true);
            }
            OpenMode::Append => {
                create_parents(&target)?;
                opts.append(true).create(true);
            }
            OpenMode::Truncate => {
                create_parents(&target)?;
                opts.write(true).create(true).truncate(true);
            }
        }
        let file = match opts.open(&target) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(FsError::NotFound { path: target })
            }
            Err(e) => return Err(FsError::Io(e)),
        };
        Ok(FileHandle {
            file,
            mode,
            _instance: PhantomData,
        })
    }

    // -----------------------------------------------------------------------
    // Whole-file reads and atomic writes
    // -----------------------------------------------------------------------

    /// FSA-FR-06: read the file's entire content into memory as raw bytes.
    pub fn read_bytes(&self, path: impl AsRef<Path>) -> FsResult<Vec<u8>> {
        let target = self.validate(path.as_ref())?;
        super::read_bytes_at(&target)
    }

    /// FSA-FR-NRQE: read the whole content of a regular file, at most
    /// `max_bytes` of it. Refuses a FIFO, a device, a directory, a link, and a
    /// larger file, and never blocks on the entry it opens.
    pub fn read_regular_bytes(&self, path: impl AsRef<Path>, max_bytes: u64) -> FsResult<Vec<u8>> {
        let target = self.validate(path.as_ref())?;
        super::bounded_read::read_regular_at(&target, max_bytes)
    }

    /// FSA-FR-06: read as UTF-8 text.
    pub fn read_text(&self, path: impl AsRef<Path>) -> FsResult<String> {
        let target = self.validate(path.as_ref())?;
        super::read_text_at(&target)
    }

    /// FSA-FR-06: read as TOML and deserialise into `T`.
    pub fn read_toml<T: DeserializeOwned>(&self, path: impl AsRef<Path>) -> FsResult<T> {
        let target = self.validate(path.as_ref())?;
        super::read_toml_at(&target)
    }

    /// FSA-FR-04 / FSA-FR-05: atomic whole-file write, parents created on demand.
    pub fn write_bytes_atomic(&self, path: impl AsRef<Path>, bytes: &[u8]) -> FsResult<()> {
        self.deny_write("write_bytes_atomic")?;
        let target = self.validate(path.as_ref())?;
        super::write_bytes_at(&target, bytes)
    }

    /// FSA-FR-04 / FSA-FR-05: atomic UTF-8 text write.
    pub fn write_text_atomic(&self, path: impl AsRef<Path>, text: &str) -> FsResult<()> {
        self.write_bytes_atomic(path, text.as_bytes())
    }

    /// FSA-FR-04 / FSA-FR-05: atomic TOML write.
    pub fn write_toml_atomic<T: Serialize>(
        &self,
        path: impl AsRef<Path>,
        value: &T,
    ) -> FsResult<()> {
        self.deny_write("write_toml_atomic")?;
        let target = self.validate(path.as_ref())?;
        super::write_toml_at(&target, value)
    }

    /// FSA-FR-15: append newline-terminated lines in a single write.
    pub fn append_lines(&self, path: impl AsRef<Path>, lines: &[String]) -> FsResult<()> {
        self.deny_write("append_lines")?;
        if lines.is_empty() {
            return Ok(());
        }
        let target = self.validate(path.as_ref())?;
        super::append_lines_at(&target, lines)
    }

    /// FSA-FR-07: streaming SHA-256 of the file's exact bytes, lowercase hex.
    pub fn sha256_file(&self, path: impl AsRef<Path>) -> FsResult<String> {
        let target = self.validate(path.as_ref())?;
        super::sha256_file_at(&target)
    }

    /// FSA-FR-08: guarantee `<synthesis_dir>/.gitignore` ignores the required
    /// entries and holds no entry that ignores `drafts/`, preserving whatever
    /// the user already put there.
    ///
    /// Removing the drafts entry is the migration of `DRS-draft-storage.md`
    /// DRS-FR-ISPI: draft storage is committed now, and this is the whole of
    /// what an existing project needs. No file outside
    /// `<synthesis_dir>/.gitignore` is read or written, so not one draft file is
    /// deleted, moved, rewritten, staged, or committed by it.
    pub fn ensure_gitignored(&self, synthesis_dir: impl AsRef<Path>) -> FsResult<()> {
        self.deny_write("ensure_gitignored")?;
        let dir = self.validate(synthesis_dir.as_ref())?;
        let target = dir.join(".gitignore");
        // The join stays inside the validated directory, so it needs no second
        // containment pass — but it does need the symlink policy applied, since
        // `.gitignore` itself could be a link.
        let target = self.validate(&target)?;
        let existing = match super::read_text_at(&target) {
            Ok(s) => s,
            Err(FsError::NotFound { .. }) => String::new(),
            Err(e) => return Err(e),
        };
        let updated = super::merge_gitignore(&existing, super::REQUIRED_GITIGNORE_ENTRIES);
        if updated == existing {
            return Ok(());
        }
        super::write_bytes_at(&target, updated.as_bytes())
    }

    // -----------------------------------------------------------------------
    // FSA-FR-11 / FSA-FR-12 / FSA-FR-13 / FSA-FR-14: structural mutations
    // -----------------------------------------------------------------------

    /// FSA-FR-11: remove the entry at `path`.
    ///
    /// A file goes whatever `recursive` says. A directory goes with its whole
    /// subtree when `recursive` is true, and only when already empty when it is
    /// false — a directory holding any entry at all yields `NotEmpty` with
    /// nothing removed. That error is the module's emptiness *probe* as much as
    /// its refusal: it reads the filesystem rather than any caller's picture of
    /// it, so it counts entries the caller may never have been shown.
    pub fn delete_path(&self, path: impl AsRef<Path>, recursive: bool) -> FsResult<()> {
        self.deny_write("delete_path")?;
        let target = self.validate(path.as_ref())?;
        let meta = match fs::symlink_metadata(&target) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(FsError::NotFound { path: target })
            }
            Err(e) => return Err(FsError::Io(e)),
        };
        if !meta.is_dir() {
            return fs::remove_file(&target).map_err(FsError::Io);
        }
        if !recursive {
            let mut entries = fs::read_dir(&target).map_err(FsError::Io)?;
            if entries.next().is_some() {
                return Err(FsError::NotEmpty { path: target });
            }
            return fs::remove_dir(&target).map_err(FsError::Io);
        }
        // Refuse up front if the subtree holds a link, so the removal is
        // all-or-nothing rather than stopping halfway.
        self.check_tree(&target)?;
        fs::remove_dir_all(&target).map_err(FsError::Io)
    }

    /// FSA-FR-12: rename within the existing parent. `new_name` is a bare
    /// basename; the operation never overwrites.
    pub fn rename_path(&self, path: impl AsRef<Path>, new_name: &str) -> FsResult<()> {
        self.deny_write("rename_path")?;
        if !super::is_valid_basename(new_name) {
            return Err(FsError::InvalidName {
                name: new_name.to_string(),
            });
        }
        let src = self.validate(path.as_ref())?;
        if fs::symlink_metadata(&src).is_err() {
            return Err(FsError::NotFound { path: src });
        }
        let parent = src.parent().ok_or_else(|| {
            FsError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("rename source {src:?} has no parent directory"),
            ))
        })?;
        let dest = parent.join(new_name);
        if fs::symlink_metadata(&dest).is_ok() {
            return Err(FsError::AlreadyExists { path: dest });
        }
        // See the parent module's note on the TOCTOU window here: POSIX
        // `rename(2)` atomically *replaces* and has no portable
        // "fail if exists" variant. Single-user desktop app; concurrency is the
        // caller's per this module's NFR.
        fs::rename(&src, &dest).map_err(FsError::Io)
    }

    /// FSA-FR-30: relocate the entry at `source` to `dest`, which need not share
    /// `source`'s parent.
    ///
    /// This is `rename_path`'s counterpart across parents, and the only
    /// primitive that relocates a subtree **without copying it**: a directory
    /// arrives whole in one operation whose cost does not grow with what it
    /// holds. That is the whole point of having it — a caller reorganising a
    /// tree of files it must not duplicate cannot express the move as
    /// copy-then-delete without briefly holding two copies and risking losing
    /// the original to a failure in between.
    ///
    /// The refusals are settled before anything is touched: an occupied `dest`
    /// is the typed "exists" error, a `dest` inside `source`'s own subtree is
    /// the typed "invalid destination", a missing `source` is "missing", and a
    /// link anywhere in the subtree is refused under the instance's policy.
    pub fn move_path(&self, source: impl AsRef<Path>, dest: impl AsRef<Path>) -> FsResult<()> {
        self.deny_write("move_path")?;
        let src = self.validate(source.as_ref())?;
        let dst = self.validate(dest.as_ref())?;
        let meta = match fs::symlink_metadata(&src) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(FsError::NotFound { path: src })
            }
            Err(e) => return Err(FsError::Io(e)),
        };
        // Checked before the occupancy test, unlike `copy_path`'s ordering: a
        // destination inside the source is wrong about *where* it is rather than
        // about what occupies it, and reporting it as a plain collision would
        // send a caller looking for an entry to remove that removing would not
        // help. `dst == src` satisfies this too, which is the honest reading —
        // moving something onto itself is not a move.
        if dst == src || dst.starts_with(&src) {
            return Err(FsError::InvalidDestination {
                source: src,
                dest: dst,
            });
        }
        if fs::symlink_metadata(&dst).is_ok() {
            return Err(FsError::AlreadyExists { path: dst });
        }
        // Up front, so a link found deep in the tree does not produce a
        // partially-moved subtree — the same bargain `copy_path` and the
        // recursive `delete_path` strike.
        if meta.is_dir() {
            self.check_tree(&src)?;
        } else if meta.file_type().is_symlink() && !self.follow_symlinks {
            return Err(FsError::SymlinkRefused { path: src });
        }
        create_parents(&dst)?;
        match fs::rename(&src, &dst) {
            Ok(()) => Ok(()),
            // Across filesystems `rename(2)` cannot relocate anything, so the
            // only way to honour the move is to copy and then remove. It is not
            // atomic, and FSA-FR-30 says so; the source is removed only once
            // every byte has landed, so an interrupted move loses nothing and
            // leaves a partial destination the caller can see.
            Err(e) if is_cross_device(&e) => {
                self.copy_path(&src, &dst)?;
                self.delete_path(&src, true)
            }
            Err(e) => Err(FsError::Io(e)),
        }
    }

    /// FSA-FR-13: copy a file or directory tree to a destination that must not
    /// already exist. Both paths go through the gate.
    pub fn copy_path(&self, source: impl AsRef<Path>, dest: impl AsRef<Path>) -> FsResult<()> {
        self.deny_write("copy_path")?;
        let src = self.validate(source.as_ref())?;
        let dst = self.validate(dest.as_ref())?;
        let meta = match fs::symlink_metadata(&src) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(FsError::NotFound { path: src })
            }
            Err(e) => return Err(FsError::Io(e)),
        };
        // An occupied destination is "exists" whatever else is true of it —
        // checked BEFORE the self-copy guard below, because `dst == src`
        // satisfies both and FSA-FR-13 promises the typed "exists" for it. The
        // other order reports a plain destination collision as an internal
        // invalid-input error, which no caller can match on.
        if fs::symlink_metadata(&dst).is_ok() {
            return Err(FsError::AlreadyExists { path: dst });
        }
        // Copying a directory into a *descendant* of itself would create the
        // destination inside the source and recurse without bound.
        if meta.is_dir() && dst.starts_with(&src) {
            return Err(FsError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "cannot copy directory {} into itself or a descendant",
                    src.display()
                ),
            )));
        }
        if meta.is_dir() {
            // Before `create_parents`, not after: a refusal must leave nothing
            // behind at all, and a created-then-abandoned parent scaffold is
            // still something.
            self.check_tree(&src)?;
            create_parents(&dst)?;
            self.copy_dir_guarded(&src, &dst)
        } else {
            create_parents(&dst)?;
            super::copy_file_no_overwrite(&src, &dst)
        }
    }

    /// Recursive directory copy that honours this instance's symlink policy.
    ///
    /// `check_tree` has already run, so under the default policy the tree holds
    /// no link and the `is_symlink` branch below is unreachable. Under
    /// FSA-FR-27 it *is* reachable and must not refuse: `check_tree` accepted
    /// those links precisely because each resolves inside the allowlist, and
    /// refusing here would abandon a half-written destination — the outcome
    /// FSA-FR-27's up-front check exists to prevent. The link is dereferenced
    /// and its target copied, which is what following a link means.
    fn copy_dir_guarded(&self, src: &Path, dst: &Path) -> FsResult<()> {
        fs::create_dir_all(dst).map_err(FsError::Io)?;
        for entry in fs::read_dir(src).map_err(FsError::Io)? {
            let entry = entry.map_err(FsError::Io)?;
            let from = entry.path();
            let to = dst.join(entry.file_name());
            let meta = fs::symlink_metadata(&from).map_err(FsError::Io)?;
            if meta.file_type().is_symlink() {
                if !self.follow_symlinks {
                    // The tree changed under us since `check_tree`. Refused
                    // rather than skipped: silently dropping an entry would
                    // make the copy quietly lossy.
                    return Err(FsError::SymlinkRefused { path: from });
                }
                let resolved = self.resolve_following(&from)?;
                let target_meta = fs::symlink_metadata(&resolved).map_err(FsError::Io)?;
                if target_meta.is_dir() {
                    self.copy_dir_guarded(&resolved, &to)?;
                } else {
                    super::copy_file_no_overwrite(&resolved, &to)?;
                }
                continue;
            }
            if meta.is_dir() {
                self.copy_dir_guarded(&from, &to)?;
            } else {
                super::copy_file_no_overwrite(&from, &to)?;
            }
        }
        Ok(())
    }

    /// FSA-FR-14: create an empty directory, creating missing parents. An entry
    /// already at the path — directory or file alike — is an "exists" error, so
    /// this never adopts, clears, or merges into an existing directory.
    pub fn create_dir(&self, path: impl AsRef<Path>) -> FsResult<()> {
        self.deny_write("create_dir")?;
        let target = self.validate(path.as_ref())?;
        if fs::symlink_metadata(&target).is_ok() {
            return Err(FsError::AlreadyExists { path: target });
        }
        fs::create_dir_all(&target).map_err(FsError::Io)
    }

    // -----------------------------------------------------------------------
    // FSA-FR-28: the user-chosen destination
    // -----------------------------------------------------------------------

    /// FSA-FR-28: write to a destination the user picked in a dialog.
    ///
    /// Exempt from FSA-FR-10 and from FSA-FR-10 alone: atomicity (FSA-FR-04)
    /// and parent creation (FSA-FR-05) still hold, the symlink policy still
    /// applies to the destination itself, and an existing file there is
    /// replaced — choosing an occupied path in a save dialog is how a user asks
    /// for that.
    pub fn write_bytes_at_user_choice(
        &self,
        chosen: &UserChosenPath,
        bytes: &[u8],
    ) -> FsResult<()> {
        self.deny_write("write_bytes_at_user_choice")?;
        let target = chosen.as_path();
        if !self.follow_symlinks {
            // No root to measure a component walk against here, so the leaf is
            // what is checked — and the leaf is what a write would land on.
            if let Ok(meta) = fs::symlink_metadata(target) {
                if meta.file_type().is_symlink() {
                    return Err(FsError::SymlinkRefused {
                        path: target.to_path_buf(),
                    });
                }
            }
        }
        super::write_bytes_at(target, bytes)
    }

    /// FSA-FR-28: the UTF-8 text form of [`Self::write_bytes_at_user_choice`].
    pub fn write_text_at_user_choice(
        &self,
        chosen: &UserChosenPath,
        text: &str,
    ) -> FsResult<()> {
        self.write_bytes_at_user_choice(chosen, text.as_bytes())
    }
}

/// Whether a failed `rename(2)` failed because the two paths sit on different
/// filesystems (FSA-FR-30's copy-then-remove fallback).
///
/// Matched on the raw errno as well as the kind: `EXDEV` is what every Unix
/// reports and `ERROR_NOT_SAME_DEVICE` (17) is Windows's, and reading them
/// directly keeps the fallback working on a toolchain whose `ErrorKind` mapping
/// has not caught up. A misread here would surface a routine cross-device move
/// as an unexplained I/O failure.
fn is_cross_device(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::CrossesDevices {
        return true;
    }
    #[cfg(unix)]
    let code = 18; // EXDEV
    #[cfg(windows)]
    let code = 17; // ERROR_NOT_SAME_DEVICE
    #[cfg(not(any(unix, windows)))]
    let code = i32::MIN;
    e.raw_os_error() == Some(code)
}

/// FSA-FR-05: create a path's missing parent directories.
fn create_parents(target: &Path) -> FsResult<()> {
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(FsError::Io)?;
        }
    }
    Ok(())
}
